//! `OpenRouter` model catalog compatibility for the unchanged model manager.

use std::collections::BTreeSet;

use super::*;

#[derive(Debug, Serialize, PartialEq)]
#[allow(clippy::struct_excessive_bools)] // Original extended-model wire contract.
struct ExtendedModel {
    id: String,
    name: String,
    supports_multimodal: bool,
    supports_image: bool,
    supports_video: bool,
    probe_source: &'static str,
    is_free: bool,
    provider: String,
    input_modalities: Vec<String>,
    output_modalities: Vec<String>,
    pricing: BTreeMap<String, String>,
}

#[derive(Default, Deserialize)]
struct FilterRequest {
    #[serde(default)]
    providers: Vec<String>,
    #[serde(default)]
    input_modalities: Vec<String>,
    #[serde(default)]
    output_modalities: Vec<String>,
    max_prompt_price: Option<f64>,
    is_free: Option<bool>,
}

pub(super) fn router() -> Router<AppServer> {
    Router::new()
        .route("/api/models/openrouter/series", get(series))
        .route("/api/models/openrouter/discover-extended", post(discover))
        .route("/api/models/openrouter/models/filter", post(filter))
}

async fn catalog(server: &AppServer) -> Result<Vec<ExtendedModel>, ApiError> {
    let remote = {
        let _guard = server.inner.desktop_models_lock.lock().await;
        let registry = read_registry(server)?;
        let provider = registry
            .providers
            .get("openrouter")
            .ok_or_else(|| not_found("OpenRouter provider not found"))?;
        let secret = load_provider_secret(server, "openrouter").await?;
        let mut remote = remote_provider(provider, &TestProviderRequest::default(), secret);
        for (name, value) in [
            ("HTTP-Referer", "https://qwenpaw.agentscope.io/"),
            ("X-OpenRouter-Title", "QwenPaw"),
            ("X-OpenRouter-Categories", "personal-agent,cli-agent"),
            ("User-Agent", "QwenPaw/1.1"),
        ] {
            if !remote
                .custom_headers
                .iter()
                .any(|(key, _)| key.eq_ignore_ascii_case(name))
            {
                remote
                    .custom_headers
                    .push((name.to_owned(), value.to_owned()));
            }
        }
        remote
    };
    desktop_model_remote::discover_catalog(&remote, normalize)
        .await
        .map_err(|_| internal("OpenRouter model catalog could not be fetched"))
}

fn providers(models: &[ExtendedModel]) -> BTreeSet<&str> {
    models
        .iter()
        .map(|model| model.provider.as_str())
        .filter(|provider| !provider.is_empty())
        .collect()
}

pub(super) async fn probe(
    server: &AppServer,
    model_id: &str,
) -> Result<desktop_model_remote::RemoteProbe, ApiError> {
    let model = catalog(server)
        .await?
        .into_iter()
        .find(|model| model.id == model_id)
        .ok_or_else(|| bad_request("Model was not found in the OpenRouter catalog"))?;
    Ok(desktop_model_remote::RemoteProbe {
        supports_image: model.supports_image,
        supports_video: model.supports_video,
        image_message: format!(
            "Image capability reported by OpenRouter model metadata: {}",
            model.supports_image
        ),
        video_message: format!(
            "Video capability reported by OpenRouter model metadata: {}",
            model.supports_video
        ),
    })
}

async fn series(State(server): State<AppServer>) -> Result<Json<Value>, ApiError> {
    let models = catalog(&server).await?;
    Ok(Json(json!({"series": providers(&models)})))
}

async fn discover(State(server): State<AppServer>, body: Bytes) -> Result<Json<Value>, ApiError> {
    let body = optional_json_body::<Option<DiscoverModelsRequest>>(&body)?.unwrap_or_default();
    if let Some(key) = body.api_key.filter(|key| !key.is_empty()) {
        let _ = configure_provider(
            State(server.clone()),
            Path(String::from("openrouter")),
            Json(json!({"api_key": key})),
        )
        .await?;
    }
    match catalog(&server).await {
        Ok(models) => Ok(Json(json!({"success": true, "models": models,
            "providers": providers(&models), "total_count": models.len()}))),
        Err(_) => Ok(Json(json!({"success": false, "models": [],
            "providers": [], "total_count": 0}))),
    }
}

async fn filter(
    State(server): State<AppServer>,
    Json(body): Json<FilterRequest>,
) -> Result<Json<Value>, ApiError> {
    for values in [
        &body.providers,
        &body.input_modalities,
        &body.output_modalities,
    ] {
        if values.len() > 256 || values.iter().any(|value| value.len() > 256) {
            return Err(bad_request("OpenRouter filter exceeds supported limits"));
        }
    }
    let models = catalog(&server)
        .await?
        .into_iter()
        .filter_map(|model| match matches_filter(&model, &body) {
            Ok(true) => Some(Ok(model)),
            Ok(false) => None,
            Err(error) => Some(Err(error)),
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Json(
        json!({"success": true, "total_count": models.len(), "models": models}),
    ))
}

fn matches_filter(model: &ExtendedModel, filter: &FilterRequest) -> Result<bool, ApiError> {
    if (!filter.providers.is_empty()
        && !filter
            .providers
            .iter()
            .any(|value| value.to_lowercase() == model.provider.to_lowercase()))
        || (!filter.input_modalities.is_empty()
            && !filter
                .input_modalities
                .iter()
                .any(|value| model.input_modalities.contains(value)))
        || (!filter.output_modalities.is_empty()
            && !filter
                .output_modalities
                .iter()
                .any(|value| model.output_modalities.contains(value)))
    {
        return Ok(false);
    }
    if let Some(maximum) = filter.max_prompt_price {
        let Some(price) = model
            .pricing
            .get("prompt")
            .filter(|price| !price.is_empty())
        else {
            return Ok(false);
        };
        let price = price
            .trim()
            .parse::<f64>()
            .map_err(|_| internal("OpenRouter model catalog contains an invalid prompt price"))?;
        if price
            .partial_cmp(&maximum)
            .is_none_or(std::cmp::Ordering::is_gt)
        {
            return Ok(false);
        }
    }
    Ok(filter.is_free != Some(true) || model.is_free)
}

fn normalize(row: &Value) -> Option<(String, ExtendedModel)> {
    let id = row.get("id")?.as_str()?.trim();
    validate_model_id(id).ok()?;
    let provider = id.split_once('/').map_or("", |(provider, _)| provider);
    let name = if id.contains('/') {
        id.rsplit('/').next().unwrap_or(id)
    } else {
        row.get("name")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .unwrap_or(id)
    };
    let modalities = |key| {
        row.get("architecture")
            .and_then(|value| value.get(key))
            .and_then(Value::as_array)
            .map(|values| {
                values
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    };
    let input_modalities = modalities("input_modalities");
    let output_modalities = modalities("output_modalities");
    let pricing = row
        .get("pricing")
        .and_then(Value::as_object)
        .map(|values| {
            values
                .iter()
                .filter(|(_, value)| !value.is_null())
                .map(|(key, value)| {
                    (
                        key.clone(),
                        value
                            .as_str()
                            .map_or_else(|| value.to_string(), str::to_owned),
                    )
                })
                .collect::<BTreeMap<_, _>>()
        })
        .unwrap_or_default();
    let numeric = pricing
        .values()
        .filter_map(|value| {
            let value = value.trim();
            value.parse::<f64>().ok().map(|number| {
                // A tiny nonzero decimal must not become free through f64 underflow.
                number == 0.0
                    && !value
                        .split(['e', 'E'])
                        .next()
                        .unwrap_or(value)
                        .chars()
                        .any(|value| matches!(value, '1'..='9'))
            })
        })
        .collect::<Vec<_>>();
    Some((
        id.to_owned(),
        ExtendedModel {
            id: id.to_owned(),
            name: name.to_owned(),
            provider: provider.to_owned(),
            supports_multimodal: input_modalities.iter().any(|value| value != "text"),
            supports_image: input_modalities.iter().any(|value| value == "image"),
            supports_video: input_modalities.iter().any(|value| value == "video"),
            probe_source: "documentation",
            is_free: !numeric.is_empty() && numeric.iter().all(|value| *value),
            input_modalities,
            output_modalities,
            pricing,
        },
    ))
}

#[cfg(test)]
#[path = "desktop_openrouter_tests.rs"]
mod tests;
