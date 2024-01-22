use std::collections::HashMap;

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Asset {
    hash: String,
    id: String,
    names: Vec<String>,
    namespaces: Vec<String>,
    path: String,
    url: String,
}

pub async fn init(
    hash_map: &mut HashMap<String, String>,
    index_url: &str,
) -> anyhow::Result<()> {
    let res: Vec<Asset> = reqwest::get(index_url).await?.json().await?;
    for asset in res {
        let url = asset.url.as_str();
        asset.names.into_iter().for_each(|name| {
            hash_map.insert(name, String::from(url));
        });
    }
    Ok(())
}

// pub async fn get_bytes(key: &str, hash_map: &HashMap<String, String>) -> anyhow::Result<Bytes> {
//     let Some(url) = hash_map.get(key) else {
//         return Err(anyhow::Error::msg("key not found"));
//     };
//     let res = reqwest::get(url).await?.bytes().await?;
//
//     Ok(())
// }
