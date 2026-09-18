//! Read-only official plugin catalog for the unchanged Console.

use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::time::Duration;

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use futures_util::StreamExt as _;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use super::{ApiError, AppServer, MAX_BYTES};
use crate::desktop_pawapps::plugins_directory;

#[path = "desktop_official_plugin_metadata.rs"]
mod metadata;

#[cfg(test)]
#[path = "desktop_official_plugin_transport_tests.rs"]
mod transport_tests;

#[derive(Default, Deserialize)]
struct PluginIndex {
    #[serde(default)]
    updated_at: Value,
    #[serde(default)]
    files: FileEntries,
}

// Preserve stable sort ties without changing serde_json ordering workspace-wide.
#[derive(Default)]
struct FileEntries(Vec<(String, Value)>);

impl<'de> Deserialize<'de> for FileEntries {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Ordered;
        impl<'de> serde::de::Visitor<'de> for Ordered {
            type Value = FileEntries;
            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("plugin files object or null")
            }
            fn visit_unit<E: serde::de::Error>(self) -> Result<Self::Value, E> {
                Ok(FileEntries::default())
            }
            fn visit_bool<E: serde::de::Error>(self, value: bool) -> Result<Self::Value, E> {
                if value {
                    Err(E::custom("invalid files"))
                } else {
                    self.visit_unit()
                }
            }
            fn visit_u64<E: serde::de::Error>(self, value: u64) -> Result<Self::Value, E> {
                if value == 0 {
                    self.visit_unit()
                } else {
                    Err(E::custom("invalid files"))
                }
            }
            fn visit_i64<E: serde::de::Error>(self, value: i64) -> Result<Self::Value, E> {
                if value == 0 {
                    self.visit_unit()
                } else {
                    Err(E::custom("invalid files"))
                }
            }
            fn visit_f64<E: serde::de::Error>(self, value: f64) -> Result<Self::Value, E> {
                if value == 0.0 {
                    self.visit_unit()
                } else {
                    Err(E::custom("invalid files"))
                }
            }
            fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Self::Value, E> {
                if value.is_empty() {
                    self.visit_unit()
                } else {
                    Err(E::custom("invalid files"))
                }
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut sequence: A,
            ) -> Result<Self::Value, A::Error> {
                if sequence.next_element::<Value>()?.is_none() {
                    self.visit_unit()
                } else {
                    Err(serde::de::Error::custom("invalid files"))
                }
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut map: A,
            ) -> Result<Self::Value, A::Error> {
                let mut entries = Vec::<(String, Value)>::new();
                let mut positions = BTreeMap::<String, usize>::new();
                while let Some((key, value)) = map.next_entry::<String, Value>()? {
                    if let Some(&position) = positions.get(&key) {
                        entries[position].1 = value;
                    } else {
                        positions.insert(key.clone(), entries.len());
                        entries.push((key, value));
                    }
                }
                Ok(FileEntries(entries))
            }
        }
        deserializer.deserialize_any(Ordered)
    }
}

fn catalog_error(message: &str) -> Json<Value> {
    Json(json!({"updated_at":null,"plugins":[],"error":message}))
}

fn internal() -> ApiError {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({"detail":"Plugin catalog could not be read"})),
    )
}

pub(super) async fn list(State(server): State<AppServer>) -> Result<Json<Value>, ApiError> {
    let base = server.inner.desktop_market.download.trim_end_matches('/');
    let Ok(main) = fetch::<Value>(base, "/metadata/index.json").await else {
        return Ok(catalog_error("Failed to fetch plugin catalog index"));
    };
    let product = &main["products"]["plugins"];
    if !metadata::truthy(product) {
        return Ok(Json(json!({"updated_at":null,"plugins":[],"error":null})));
    }
    let path = metadata::field(product, "index_url", "");
    if source_url(base, &path).is_none() {
        return Ok(catalog_error("Invalid plugins index_url in main metadata"));
    }
    let Ok(index) = fetch::<PluginIndex>(base, &path).await else {
        return Ok(catalog_error("Failed to fetch plugins metadata"));
    };
    let owned = server.clone();
    let installed = tokio::task::spawn_blocking(move || installed_plugins(&owned))
        .await
        .map_err(|_| internal())??;
    let mut plugins = Vec::new();
    for (file_id, entry) in index.files.0 {
        if !entry.is_object() || !metadata::compatible(&entry) {
            continue;
        }
        let path = metadata::field(&entry, "url", "");
        if source_url(base, &path).is_none() {
            continue;
        }
        plugins.push(metadata::row(&entry, &file_id, base, &path, &installed));
    }
    plugins.sort_by(|left, right| {
        (left["kind"].as_str(), left["name"].as_str())
            .cmp(&(right["kind"].as_str(), right["name"].as_str()))
    });
    Ok(Json(
        json!({"updated_at":index.updated_at,"plugins":plugins,"error":null}),
    ))
}

fn source_url(base: &str, path: &str) -> Option<url::Url> {
    if !path.starts_with('/') {
        return None;
    }
    // Concatenation, not join: a network-path reference must not replace the host.
    let source = url::Url::parse(base).ok()?;
    let target = url::Url::parse(&format!("{base}{path}")).ok()?;
    (source.origin() == target.origin()
        && target.username().is_empty()
        && target.password().is_none())
    .then_some(target)
}

async fn fetch<T: DeserializeOwned + Send + 'static>(base: &str, path: &str) -> Result<T, ()> {
    let url = source_url(base, path).ok_or(())?;
    let response = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|_| ())?
        .get(url)
        .header("accept", "application/json")
        .header("accept-encoding", "gzip")
        .send()
        .await
        .map_err(|_| ())?;
    if !response.status().is_success()
        || response
            .content_length()
            .is_some_and(|length| length > MAX_BYTES as u64)
    {
        return Err(());
    }
    let gzip = response
        .headers()
        .get("content-encoding")
        .is_some_and(|value| value == "gzip");
    let mut stream = response.bytes_stream();
    let mut bytes = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| ())?;
        if chunk.len() > MAX_BYTES - bytes.len() {
            return Err(());
        }
        bytes.extend_from_slice(&chunk);
    }
    tokio::task::spawn_blocking(move || {
        if gzip || bytes.starts_with(&[0x1f, 0x8b]) {
            let mut decoded = Vec::new();
            flate2::read::MultiGzDecoder::new(bytes.as_slice())
                .take(MAX_BYTES as u64 + 1)
                .read_to_end(&mut decoded)
                .map_err(|_| ())?;
            if decoded.len() > MAX_BYTES {
                return Err(());
            }
            bytes = decoded;
        }
        serde_json::from_slice(&bytes).map_err(|_| ())
    })
    .await
    .map_err(|_| ())?
}

fn installed_plugins(server: &AppServer) -> Result<BTreeMap<String, String>, ApiError> {
    let Some(root) = plugins_directory(server)? else {
        return Ok(BTreeMap::new());
    };
    let mut installed = BTreeMap::new();
    for item in fs::read_dir(root).map_err(|_| internal())? {
        let item = item.map_err(|_| internal())?;
        if !item.file_type().is_ok_and(|kind| kind.is_dir()) {
            continue;
        }
        let Some(manifest) = read_manifest(&item.path()) else {
            continue;
        };
        let id = metadata::field(&manifest, "id", &item.file_name().to_string_lossy());
        installed.insert(id, metadata::field(&manifest, "version", "0.0.0"));
    }
    Ok(installed)
}

fn read_manifest(directory: &std::path::Path) -> Option<Value> {
    let directory = directory.canonicalize().ok()?;
    let path = directory.join("plugin.json").canonicalize().ok()?;
    if !path.starts_with(directory) || !path.is_file() {
        return None;
    }
    let mut bytes = Vec::new();
    fs::File::open(path)
        .ok()?
        .take(1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() > 1024 * 1024 {
        return None;
    }
    let manifest: Value = serde_json::from_slice(&bytes).ok()?;
    manifest.is_object().then_some(manifest)
}
