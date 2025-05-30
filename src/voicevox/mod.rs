use std::borrow::Cow;
use std::sync::Arc;

use bytes::Bytes;
use futures::future;
use moka::future::Cache;
use reqwest::{header::CONTENT_TYPE, Url};
use tap::prelude::*;

use self::model::SpeakerStyleView;
use crate::voicevox::model::{Speaker, SpeakerStyle};

pub mod model;

#[derive(Debug, Clone)]
pub struct Client {
    inner: Arc<InnerClient<'static>>,
}

#[derive(Debug)]
struct InnerClient<'a> {
    host: Url,
    client: reqwest::Client,
    speakers: Vec<model::Speaker<'a>>,
    cache: Cache<(String, model::SpeakerId), Bytes>,
}

impl Client {
    pub async fn new(host: Url, client: reqwest::Client) -> Client {
        let url = host.clone().tap_mut(|u| {
            u.path_segments_mut().unwrap().push("speakers");
        });

        let speakers: Vec<model::api::Speaker> = client
            .get(url)
            .send()
            .await
            .expect("Failed to get speakers")
            .json()
            .await
            .expect("JSON was not well-formatted");

        let speaker_infos: Vec<_> = speakers
            .iter()
            .map(|s| {
                let url = host.clone().tap_mut(|u| {
                    u.path_segments_mut().unwrap().push("speaker_info");
                    u.query_pairs_mut()
                        .clear()
                        .append_pair("speaker_uuid", &s.speaker_uuid);
                });

                let client = client.clone();

                async move {
                    client
                        .get(url)
                        .send()
                        .await
                        .unwrap_or_else(|_| {
                            panic!("Failed to get speaker information of {}", &s.speaker_uuid)
                        })
                        .json::<model::api::SpeakerInfo>()
                        .await
                        .expect("JSON was not well-formatted")
                }
            })
            .collect();

        let speaker_infos = future::join_all(speaker_infos).await;

        let speakers: Vec<model::Speaker> = speakers
            .into_iter()
            .zip(speaker_infos.into_iter())
            .map(|(speaker, speaker_info)| {
                let speaker_styles: Vec<model::SpeakerStyle> = speaker
                    .styles
                    .into_iter()
                    .zip(speaker_info.style_infos.into_iter())
                    .map(|(style, style_info)| SpeakerStyle {
                        icon: Cow::Owned(style_info.icon.bin),
                        id: style_info.id,
                        voice_samples: style_info
                            .voice_samples
                            .into_iter()
                            .map(|sample| Cow::Owned(sample.bin))
                            .collect(),
                        name: style.name,
                    })
                    .collect();

                Speaker {
                    name: speaker.name,
                    policy: speaker_info.policy,
                    styles: speaker_styles,
                }
            })
            .collect();

        let cache = Cache::new(10000);

        Client {
            inner: Arc::new(InnerClient {
                host,
                client,
                speakers,
                cache,
            }),
        }
    }

    pub fn query_style_by_id(&self, speaker_id: model::SpeakerId) -> Option<SpeakerStyleView<'_>> {
        for (speaker_i, speaker) in self.inner.speakers.iter().enumerate() {
            for (style_i, style) in speaker.styles.iter().enumerate() {
                if style.id != speaker_id {
                    continue;
                }

                return Some(SpeakerStyleView {
                    speaker_i,
                    speaker_name: &speaker.name,
                    speaker_policy: &speaker.policy,
                    style_i,
                    style_id: style.id,
                    style_icon: style.icon.clone(),
                    style_name: &style.name,
                    style_voice_samples: &style.voice_samples,
                });
            }
        }

        None
    }

    pub fn get_speakers(&self) -> &[model::Speaker] {
        &self.inner.speakers
    }

    pub async fn tts(&self, text: &str, speaker_id: model::SpeakerId) -> Result<Bytes, ()> {
        if let Some(cached) = &self.inner.cache.get(&(text.to_string(), speaker_id)).await {
            println!("cached! {text}");
            return Ok(cached.clone());
        }

        let url = self.inner.host.clone().tap_mut(|u| {
            u.path_segments_mut().unwrap().push("audio_query");
            u.query_pairs_mut()
                .clear()
                .append_pair("text", text)
                .append_pair("speaker", &speaker_id.to_string());
        });

        let resp = self.inner.client.post(url).send().await.map_err(|_| ())?;
        let query_text = resp
            .error_for_status()
            .map_err(|_| ())?
            .text()
            .await
            .map_err(|_| ())?;

        let url = self.inner.host.clone().tap_mut(|u| {
            u.path_segments_mut().unwrap().push("synthesis");
            u.query_pairs_mut()
                .clear()
                .append_pair("speaker", &speaker_id.to_string());
        });

        let resp = self
            .inner
            .client
            .post(url)
            .header(CONTENT_TYPE, "application/json")
            .body(query_text)
            .send()
            .await
            .map_err(|_| ())?
            .error_for_status()
            .map_err(|_| ())?;

        let bytes = resp.bytes().await.unwrap();

        self.inner
            .cache
            .insert((text.to_string(), speaker_id), bytes.clone())
            .await;

        Ok(bytes)
    }
}
