//! Remote skill market contracts for the unchanged Console.

use std::collections::BTreeMap;
use std::time::Duration;

use axum::Json;
use axum::Router;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::routing::{get, post};
use futures_util::StreamExt as _;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::AppServer;
use super::desktop_models::ApiError;

#[path = "desktop_market_catalog.rs"]
mod catalog;
#[path = "desktop_official_plugins.rs"]
mod official;
#[path = "desktop_plugin_market.rs"]
mod plugins;
#[path = "desktop_market_signing.rs"]
mod signing;
#[cfg(test)]
#[path = "desktop_market_tests.rs"]
mod tests;

const PROVIDERS: [(&str, &str); 4] = [
    ("qwenpaw", "QwenPaw"),
    ("clawhub", "ClawHub"),
    ("modelscope", "ModelScope"),
    ("aliyun", "Aliyun"),
];
const MAX_BYTES: usize = 8 * 1024 * 1024;
const TIMEOUT: Duration = Duration::from_secs(15);

pub(super) struct Sources {
    pub qwenpaw: String,
    pub clawhub: String,
    pub modelscope: String,
    pub aliyun: String,
    pub download: String,
}

impl Default for Sources {
    fn default() -> Self {
        Self {
            qwenpaw: String::from("https://platform.agentscope.io"),
            clawhub: String::from("https://clawhub.ai"),
            modelscope: String::from("https://www.modelscope.cn"),
            aliyun: String::from("https://agentexplorer.aliyuncs.com"),
            download: String::from("https://download.qwenpaw.agentscope.io"),
        }
    }
}

#[derive(Default)]
struct ProviderPages(Vec<(String, i64)>);

impl<'de> Deserialize<'de> for ProviderPages {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Ordered;
        impl<'de> serde::de::Visitor<'de> for Ordered {
            type Value = ProviderPages;
            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("an object mapping provider names to page numbers")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut map: A,
            ) -> Result<Self::Value, A::Error> {
                let mut values: Vec<(String, i64)> = Vec::new();
                while let Some((key, value)) = map.next_entry::<String, i64>()? {
                    if let Some(previous) = values.iter_mut().find(|(name, _)| name == &key) {
                        previous.1 = value;
                    } else {
                        if values.len() >= 32 {
                            return Err(serde::de::Error::custom("too many providers"));
                        }
                        values.push((key, value));
                    }
                }
                Ok(ProviderPages(values))
            }
        }
        deserializer.deserialize_map(Ordered)
    }
}

#[derive(Deserialize)]
struct Search {
    #[serde(default)]
    query: String,
    #[serde(default)]
    provider_pages: ProviderPages,
    #[serde(default = "default_limit")]
    limit: u64,
    #[serde(default = "default_lang")]
    lang: String,
    category: Option<String>,
}
const fn default_limit() -> u64 {
    10
}
fn default_lang() -> String {
    String::from("en")
}

#[derive(Serialize)]
struct ResultItem {
    source: String,
    slug: String,
    name: String,
    description: Option<String>,
    source_url: String,
    version: Option<String>,
    author: Option<String>,
    icon_url: Option<String>,
    stats: Option<BTreeMap<String, Value>>,
}

pub(super) fn router() -> Router<AppServer> {
    Router::new()
        .route("/api/market/providers", get(providers))
        .route("/api/market/categories", get(categories))
        .route("/api/market/search", post(search))
        .route("/api/plugins/market/search", get(plugins::search))
        .route("/api/plugins/catalog", get(official::list))
}

fn invalid(message: &str) -> ApiError {
    (StatusCode::BAD_REQUEST, Json(json!({"detail": message})))
}

fn environment(server: &AppServer) -> Result<BTreeMap<String, String>, ApiError> {
    let mut values = server
        .inner
        .core
        .runtime_environment()
        .map_err(|_| invalid("Application environment is unavailable"))?;
    for key in [
        "ALIBABA_CLOUD_ACCESS_KEY_ID",
        "ALIBABA_CLOUD_ACCESS_KEY_SECRET",
        "ALIBABA_CLOUD_SECURITY_TOKEN",
        "ALIYUN_AGENTEXPLORER_ENDPOINT",
        "QWENPAW_SKILLS_HUB_BASE_URL",
        "QWENPAW_SKILLS_HUB_SEARCH_PATH",
    ] {
        if !values.contains_key(key)
            && let Ok(value) = std::env::var(key)
        {
            values.insert(key.to_owned(), value);
        }
    }
    Ok(values)
}

fn aliyun_unavailable(values: &BTreeMap<String, String>) -> Option<String> {
    let missing = [
        "ALIBABA_CLOUD_ACCESS_KEY_ID",
        "ALIBABA_CLOUD_ACCESS_KEY_SECRET",
    ]
    .into_iter()
    .filter(|key| values.get(*key).is_none_or(String::is_empty))
    .collect::<Vec<_>>();
    (!missing.is_empty()).then(|| {
        format!(
            "missing env vars: {} (set Aliyun AK/SK so requests can be signed)",
            missing.join(", ")
        )
    })
}

async fn providers(State(server): State<AppServer>) -> Result<Json<Value>, ApiError> {
    let environment = environment(&server)?;
    Ok(Json(Value::Array(PROVIDERS.into_iter().map(|(key, label)| {
        let reason = (key == "aliyun").then(|| aliyun_unavailable(&environment)).flatten();
        json!({"key": key, "label": label, "available": reason.is_none(), "reason": reason, "supports_browse": true})
    }).collect())))
}

async fn categories(Query(query): Query<BTreeMap<String, String>>) -> Json<Value> {
    Json(catalog::categories(
        query.get("lang").map_or("en", String::as_str),
    ))
}

async fn search(
    State(server): State<AppServer>,
    Json(body): Json<Search>,
) -> Result<Json<Value>, ApiError> {
    if !(1..=50).contains(&body.limit)
        || body.query.len() > 4096
        || body.lang.len() > 64
        || body
            .category
            .as_ref()
            .is_some_and(|category| category.len() > 256)
    {
        return Err(invalid("Market search parameters exceed supported limits"));
    }
    let mut unknown = body
        .provider_pages
        .0
        .iter()
        .map(|(key, _)| key)
        .filter(|key| !PROVIDERS.iter().any(|(name, _)| name == key))
        .cloned()
        .collect::<Vec<_>>();
    if !unknown.is_empty() {
        unknown.sort();
        return Err(invalid(&format!("unknown providers: {unknown:?}")));
    }
    let environment = environment(&server)?;
    let environment = &environment;
    let sources = &server.inner.desktop_market;
    let request = &body;
    let outcomes = futures_util::future::join_all(body.provider_pages.0.iter().map(
        |(key, page)| async move {
            let result = tokio::time::timeout(
                TIMEOUT,
                search_one(
                    sources,
                    environment,
                    key,
                    request.query.as_str(),
                    u64::try_from((*page).max(1)).unwrap_or(1),
                    request.limit,
                    &request.lang,
                    request.category.as_deref(),
                ),
            )
            .await
            .unwrap_or_else(|_| Err(String::from("Market provider request timed out")));
            (key, result)
        },
    ))
    .await;
    let mut results = Vec::new();
    let mut errors = Vec::new();
    let mut by_provider = BTreeMap::new();
    for (key, outcome) in outcomes {
        match outcome {
            Ok((items, more, total)) => {
                results.extend(items);
                by_provider.insert(key, json!({"has_more": more, "total": total}));
            }
            Err(message) => errors.push(json!({"provider": key, "message": message})),
        }
    }
    Ok(Json(
        json!({"results": results, "errors": errors, "by_provider": by_provider}),
    ))
}

// Keep provider-specific pagination branches together with their common contract.
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
async fn search_one(
    sources: &Sources,
    environment: &BTreeMap<String, String>,
    key: &str,
    query: &str,
    page: u64,
    limit: u64,
    lang: &str,
    category: Option<&str>,
) -> Result<(Vec<ResultItem>, bool, u64), String> {
    let (native, fallback) = catalog::routing(category, key, lang);
    let query = if query.is_empty() {
        fallback.unwrap_or(query)
    } else {
        query
    };
    match key {
        "qwenpaw" | "modelscope" => {
            let base = if key == "qwenpaw" {
                &sources.qwenpaw
            } else {
                &sources.modelscope
            };
            let mut url = endpoint(base, "/openapi/v1/skills")?;
            url.query_pairs_mut()
                .append_pair("page_size", &limit.to_string())
                .append_pair("page_number", &page.to_string());
            if !query.trim().is_empty() {
                url.query_pairs_mut().append_pair("search", query.trim());
            }
            if let Some(native) = native {
                url.query_pairs_mut().append_pair(
                    if key == "qwenpaw" {
                        "category"
                    } else {
                        "filter.category"
                    },
                    native,
                );
            }
            let body = fetch(url, HeaderMap::new()).await?;
            if body.get("success") == Some(&Value::Bool(false)) {
                return Err(String::from("Market provider reported failure"));
            }
            let items = body
                .pointer("/data/skills")
                .and_then(Value::as_array)
                .ok_or("Market provider returned an invalid catalog")?;
            let results = items
                .iter()
                .filter_map(|item| catalog::platform(item, key, lang))
                .collect::<Vec<_>>();
            let total = body
                .pointer("/data/total")
                .and_then(Value::as_u64)
                .unwrap_or(results.len() as u64);
            Ok((results, page.saturating_mul(limit) < total, total))
        }
        "clawhub" if !query.trim().is_empty() => {
            let base = environment
                .get("QWENPAW_SKILLS_HUB_BASE_URL")
                .filter(|value| !value.is_empty())
                .map_or(sources.clawhub.as_str(), String::as_str);
            let path = environment
                .get("QWENPAW_SKILLS_HUB_SEARCH_PATH")
                .filter(|value| !value.is_empty())
                .map_or("/api/v1/search", String::as_str);
            let mut url = endpoint(base, path)?;
            url.query_pairs_mut()
                .append_pair("q", query.trim())
                .append_pair("limit", "500");
            let body = fetch(url, HeaderMap::new()).await?;
            let items = body
                .as_array()
                .map(Vec::as_slice)
                .or_else(|| {
                    ["items", "skills", "results", "data"]
                        .into_iter()
                        .find_map(|key| body.get(key).and_then(Value::as_array))
                        .map(Vec::as_slice)
                })
                .or_else(|| {
                    (body.get("name").is_some() && body.get("slug").is_some())
                        .then(|| std::slice::from_ref(&body))
                })
                .ok_or("ClawHub returned an invalid catalog")?;
            let all = items
                .iter()
                .filter_map(catalog::claw_search)
                .collect::<Vec<_>>();
            let total = all.len() as u64;
            let start = page.saturating_sub(1).saturating_mul(limit);
            Ok((
                all.into_iter()
                    .skip(usize::try_from(start).unwrap_or(usize::MAX))
                    .take(usize::try_from(limit).unwrap_or(usize::MAX))
                    .collect(),
                start.saturating_add(limit) < total,
                total,
            ))
        }
        "clawhub" | "aliyun" => {
            if key == "aliyun"
                && let Some(reason) = aliyun_unavailable(environment)
            {
                return Err(reason);
            }
            if page > 50 {
                return Ok((Vec::new(), false, 0));
            }
            let mut cursor: Option<String> = None;
            for current in 1..=page {
                let (url, headers) = if key == "clawhub" {
                    let mut url = endpoint(&sources.clawhub, "/api/v1/skills")?;
                    url.query_pairs_mut()
                        .append_pair("limit", &limit.to_string())
                        .append_pair("sort", "recommended");
                    if let Some(cursor) = &cursor {
                        url.query_pairs_mut().append_pair("cursor", cursor);
                    }
                    (url, HeaderMap::new())
                } else {
                    let mut query_params =
                        BTreeMap::from([(String::from("maxResults"), limit.to_string())]);
                    if !query.is_empty() {
                        query_params.insert(String::from("keyword"), query.to_owned());
                    }
                    if let Some(cursor) = &cursor {
                        query_params.insert(String::from("nextToken"), cursor.clone());
                    }
                    signing::request(
                        sources,
                        environment,
                        "SearchSkills",
                        "/openapi/skills",
                        &query_params,
                    )?
                };
                let body = fetch(url, headers).await?;
                let items = body
                    .get(if key == "clawhub" { "items" } else { "data" })
                    .and_then(Value::as_array)
                    .ok_or("Market provider returned an invalid catalog")?;
                cursor = body
                    .get(if key == "clawhub" {
                        "nextCursor"
                    } else {
                        "nextToken"
                    })
                    .and_then(Value::as_str)
                    .filter(|value| !value.is_empty())
                    .map(str::to_owned);
                if current == page {
                    let results = items
                        .iter()
                        .filter_map(|item| {
                            if key == "clawhub" {
                                catalog::claw_browse(item)
                            } else {
                                catalog::aliyun(item)
                            }
                        })
                        .collect();
                    let total = if key == "aliyun" {
                        body.get("totalCount")
                            .and_then(catalog::integer)
                            .and_then(|value| u64::try_from(value).ok())
                            .unwrap_or(0)
                    } else {
                        0
                    };
                    return Ok((results, cursor.is_some(), total));
                }
                if cursor.is_none() {
                    return Ok((Vec::new(), false, 0));
                }
            }
            Ok((Vec::new(), false, 0))
        }
        _ => Err(String::from("Unknown market provider")),
    }
}

pub(super) fn endpoint(base: &str, path: &str) -> Result<url::Url, String> {
    let url = url::Url::parse(&format!("{}{path}", base.trim_end_matches('/')))
        .map_err(|_| String::from("Market endpoint is invalid"))?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
        || url.query().is_some()
    {
        return Err(String::from("Market endpoint is invalid"));
    }
    Ok(url)
}

pub(super) fn aliyun_skill_request(
    server: &AppServer,
    skill: &str,
) -> Result<(url::Url, HeaderMap), ApiError> {
    let environment = environment(server)?;
    if let Some(reason) = aliyun_unavailable(&environment) {
        return Err(invalid(&reason));
    }
    signing::request(
        &server.inner.desktop_market,
        &environment,
        "GetSkillContent",
        &format!("/openapi/skills/{}", signing::encode(skill)),
        &BTreeMap::new(),
    )
    .map_err(|message| invalid(&message))
}

async fn fetch(url: url::Url, headers: HeaderMap) -> Result<Value, String> {
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(TIMEOUT)
        .build()
        .map_err(|_| String::from("Market HTTP client could not start"))?;
    let response = client
        .get(url)
        .headers(headers)
        .send()
        .await
        .map_err(|_| String::from("Market provider request failed"))?;
    if !response.status().is_success() {
        return Err(format!(
            "Market provider returned HTTP {}",
            response.status().as_u16()
        ));
    }
    let mut bytes = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk =
            chunk.map_err(|_| String::from("Market provider response could not be read"))?;
        if bytes.len().saturating_add(chunk.len()) > MAX_BYTES {
            return Err(String::from("Market provider response exceeds size limit"));
        }
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes)
        .map_err(|_| String::from("Market provider returned invalid JSON"))
}
