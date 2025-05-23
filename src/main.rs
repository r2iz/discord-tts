#![warn(clippy::pedantic)]

mod commands;
mod config;
mod db;
mod filter;
mod songbird_handler;
mod sozai;
mod voicevox;
mod wavsource;

use std::io::Cursor;

use reqwest::Url;
use serenity::{
    async_trait,
    client::{Client, Context, EventHandler},
    model::{
        application::{Command, Interaction},
        channel::Message,
        gateway::Ready,
        prelude::GatewayIntents,
    },
};
use songbird::{input::HttpRequest, SerenityInit};
use tap::Tap;

use crate::config::CONFIG;
use crate::db::INMEMORY_DB;
use crate::db::PERSISTENT_DB;

struct Bot {
    voicevox: voicevox::Client,
    prefix: String,
}

#[async_trait]
impl EventHandler for Bot {
    async fn ready(&self, ctx: Context, ready: Ready) {
        Command::set_global_commands(
            &ctx.http,
            vec![
                commands::join::register(&self.prefix),
                commands::leave::register(&self.prefix),
                commands::skip::register(&self.prefix),
                commands::speaker::register(&self.prefix),
                commands::dict::register(&self.prefix),
            ],
        )
        .await
        .unwrap();

        println!("{} is connected!", ready.user.name);
    }

    async fn message(&self, ctx: Context, msg: Message) {
        let Some(content) = filter::filter(&ctx, &msg.clone()).await else {
            return;
        };
        if let Some(url) = INMEMORY_DB.get_sozai_url(&msg.content) {
            let client = HttpRequest::new(reqwest::Client::new(), url.clone());

            let manager = songbird::get(&ctx)
                .await
                .expect("Songbird is not initialized");

            let handler = manager.get(msg.guild_id.unwrap()).unwrap();

            let track = handler.lock().await.enqueue_input(client.into()).await;
            track.set_volume(0.3).unwrap();
        } else {
            let speaker = PERSISTENT_DB.get_speaker_id(msg.author.id);

            let manager = songbird::get(&ctx)
                .await
                .expect("Songbird is not initialized");

            let handler = manager.get(msg.guild_id.unwrap()).unwrap();

            let Ok(wav) = self.voicevox.tts(&content, speaker).await else {
                msg.reply(&ctx.http, "Error: Failed to synthesise a message")
                    .await
                    .unwrap();
                return;
            };

            handler
                .lock()
                .await
                .enqueue_input(
                    songbird::input::RawAdapter::new(
                        wavsource::WavSource::new(&mut Cursor::new(wav)),
                        48000,
                        1,
                    )
                    .into(),
                )
                .await;
        };
    }

    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        let prefix = &self.prefix;
        match interaction {
            Interaction::Command(command) => match command.data.name.as_str() {
                s if s == format!("{prefix}speaker") => {
                    commands::speaker::run(&ctx, command, &self.voicevox).await;
                }
                s if s == format!("{prefix}join") => commands::join::run(&ctx, command).await,
                s if s == format!("{prefix}leave") => commands::leave::run(&ctx, command).await,
                s if s == format!("{prefix}skip") => commands::skip::run(&ctx, command).await,
                s if s == format!("{prefix}dict") => commands::dict::run(&ctx, command).await,
                _ => unreachable!("Unknown command: {}", command.data.name),
            },
            Interaction::Component(interaction) => {
                commands::speaker::update(&ctx, interaction, &self.voicevox).await;
            }
            _ => {}
        }
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let intents = GatewayIntents::GUILDS
        | GatewayIntents::GUILD_VOICE_STATES
        | GatewayIntents::GUILD_MESSAGES
        | GatewayIntents::MESSAGE_CONTENT;

    let default_header = reqwest::header::HeaderMap::new().tap_mut(|h| {
        let Some(s) = &CONFIG.additional_headers else {
            return;
        };

        for s in s.split(',') {
            let mut split = s.split(':');

            let key = split.next().unwrap().trim();
            let value = split.next().unwrap().trim();

            h.insert(key, reqwest::header::HeaderValue::from_str(value).unwrap());
        }
    });

    let mut client = Client::builder(&CONFIG.discord_token, intents)
        .event_handler(Bot {
            voicevox: voicevox::Client::new(
                Url::parse(&CONFIG.voicevox_host).unwrap(),
                reqwest::Client::builder()
                    .default_headers(default_header)
                    .build()
                    .unwrap(),
            )
            .await,
            prefix: CONFIG.command_prefix.clone().unwrap_or_default(),
        })
        .register_songbird()
        .await
        .expect("Failed to create client");

    INMEMORY_DB
        .init_sozai_map(&CONFIG.sozai_index_url)
        .await
        .unwrap();

    tokio::spawn(async move {
        let _: Result<_, _> = client
            .start()
            .await
            .map_err(|why| println!("Client ended: {why:?}"));
    });

    tokio::signal::ctrl_c()
        .await
        .expect("Failed to wait Ctrl+C");

    println!("Received Ctrl+C, shutting down.");
}
