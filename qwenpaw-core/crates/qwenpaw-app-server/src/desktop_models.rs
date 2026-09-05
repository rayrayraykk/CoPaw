//! Persistent model-provider configuration for the unchanged Console.

use std::collections::BTreeMap;
use std::fs;
use std::io::Write as _;
use std::path::PathBuf;

use axum::Json;
use axum::Router;
use axum::body::Bytes;
use axum::extract::Path;
use axum::extract::Query;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::http::StatusCode;
use axum::routing::delete;
use axum::routing::get;
use axum::routing::post;
use axum::routing::put;
use chrono::SecondsFormat;
use chrono::Utc;
use futures_util::StreamExt as _;
use qwenpaw_core::Core;
use qwenpaw_protocol::ConfigWriteParams;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Map;
use serde_json::Value;
use serde_json::json;

use super::AppServer;
use super::DesktopCredentialStore;
use super::DesktopWorkspace;
use super::desktop_model_remote;

const REGISTRY_SCHEMA_VERSION: u32 = 1;
const DEFAULT_PROVIDER_ID: &str = "openai-compatible";
const REGISTRY_MAX_BYTES: u64 = 2 * 1024 * 1024;
const MAX_PROVIDERS: usize = 128;
const MAX_MODELS_PER_PROVIDER: usize = 4_096;
const MAX_PROVIDER_NAME_BYTES: usize = 256;
const MAX_MODEL_TEXT_BYTES: usize = 1_024;
const MAX_URL_BYTES: usize = 4_096;
const MAX_API_KEY_BYTES: usize = 8_192;
const MAX_HEADERS: usize = 256;
const MAX_JSON_DEPTH: usize = 16;
const MAX_JSON_ITEMS: usize = 4_096;
const MAX_JSON_STRING_BYTES: usize = 16 * 1024;
const MODEL_SECRET_PREFIX: &str = "model-provider-api-key:";
const MODEL_TEST_TIMEOUT_SECONDS: u64 = 5;
const MODEL_TEST_ERROR_BODY_BYTES: usize = 16 * 1024;
const BUILTIN_PROVIDERS_JSON: &str = include_str!("../assets/builtin_providers.json");
const BUILTIN_PROVIDER_ORDER: &[&str] = &[
    "qwenpaw-local",
    "ollama",
    "lmstudio",
    "openrouter",
    "github-models",
    "modelscope",
    "dashscope",
    "aliyun-codingplan",
    "aliyun-codingplan-intl",
    "aliyun-tokenplan",
    "aliyun-tokenplan-intl",
    "opencode",
    "kilo",
    "openai",
    "openai-response",
    "azure-openai",
    "anthropic",
    "gemini",
    "deepseek",
    "kimi-cn",
    "kimi-intl",
    "kimi-codingplan",
    "minimax-cn",
    "minimax",
    "zhipu-cn",
    "zhipu-cn-codingplan",
    "zhipu-intl",
    "zhipu-intl-codingplan",
    "siliconflow-cn",
    "siliconflow-intl",
    "volcengine-cn",
    "volcengine-cn-codingplan",
    "volcengine-cn-agentplan",
    "mimo-tokenplan",
    "mimo",
];

type ApiError = (StatusCode, Json<Value>);

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ProviderRegistry {
    schema_version: u32,
    revision: u64,
    active_provider_id: String,
    providers: BTreeMap<String, ProviderRecord>,
    local_model: LocalModelConfig,
    local_generate_kwargs: Map<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
#[allow(clippy::struct_excessive_bools)]
struct ProviderRecord {
    id: String,
    name: String,
    base_url: String,
    api_key_configured: bool,
    api_key_prefix: String,
    api_key_prefixes: Vec<String>,
    chat_model: String,
    models: Vec<ModelRecord>,
    extra_models: Vec<ModelRecord>,
    discovered_models: Vec<ModelRecord>,
    models_last_synced_at: Option<String>,
    models_last_sync_error: Option<String>,
    models_syncing: bool,
    hidden_model_ids: Vec<String>,
    is_custom: bool,
    is_local: bool,
    support_model_discovery: bool,
    support_connection_check: bool,
    freeze_url: bool,
    require_api_key: bool,
    generate_kwargs: Map<String, Value>,
    custom_headers: BTreeMap<String, String>,
    auth_mode: String,
    supports_oauth: bool,
    oauth_connected: bool,
    is_free_tier: bool,
    provider_group: String,
    provider_group_name: String,
    provider_variant: String,
    thinking_param_style: Option<String>,
    reasoning_effort_options: Vec<String>,
    thinking_budget_range: Vec<u64>,
    meta: Map<String, Value>,
}

impl Default for ProviderRecord {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            base_url: String::new(),
            api_key_configured: false,
            api_key_prefix: String::new(),
            api_key_prefixes: Vec::new(),
            chat_model: String::from("OpenAIChatModel"),
            models: Vec::new(),
            extra_models: Vec::new(),
            discovered_models: Vec::new(),
            models_last_synced_at: None,
            models_last_sync_error: None,
            models_syncing: false,
            hidden_model_ids: Vec::new(),
            is_custom: false,
            is_local: false,
            support_model_discovery: false,
            support_connection_check: false,
            freeze_url: false,
            require_api_key: true,
            generate_kwargs: Map::new(),
            custom_headers: BTreeMap::new(),
            auth_mode: String::from("api_key"),
            supports_oauth: false,
            oauth_connected: false,
            is_free_tier: false,
            provider_group: String::new(),
            provider_group_name: String::new(),
            provider_variant: String::new(),
            thinking_param_style: None,
            reasoning_effort_options: vec![
                String::from("none"),
                String::from("minimal"),
                String::from("low"),
                String::from("medium"),
                String::from("high"),
                String::from("xhigh"),
            ],
            thinking_budget_range: vec![1, 81_920],
            meta: Map::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
#[allow(clippy::struct_excessive_bools)]
struct ModelRecord {
    id: String,
    name: String,
    supports_multimodal: Option<bool>,
    supports_image: Option<bool>,
    supports_video: Option<bool>,
    probe_source: Option<String>,
    is_free: bool,
    is_recommended: bool,
    source: String,
    discovery_origin: Option<String>,
    availability_status: String,
    max_output_length: Option<u64>,
    max_output_length_source: String,
    max_output_length_updated_at: Option<String>,
    max_input_length: u64,
    max_input_length_configured: bool,
    max_input_length_auto_detected: Option<u64>,
    generate_kwargs: Map<String, Value>,
    relay_reasoning: bool,
    thinking_enabled: Option<bool>,
    thinking_budget: Option<u64>,
    reasoning_effort: Option<String>,
    thinking_param_style: Option<String>,
    reasoning_effort_options: Option<Vec<String>>,
    thinking_budget_range: Option<Vec<u64>>,
    supports_agent_thinking: Option<bool>,
}

impl Default for ModelRecord {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            supports_multimodal: None,
            supports_image: None,
            supports_video: None,
            probe_source: None,
            is_free: false,
            is_recommended: false,
            source: String::from("user"),
            discovery_origin: None,
            availability_status: String::from("unverified"),
            max_output_length: None,
            max_output_length_source: String::from("unknown"),
            max_output_length_updated_at: None,
            max_input_length: 128_000,
            max_input_length_configured: false,
            max_input_length_auto_detected: None,
            generate_kwargs: Map::new(),
            relay_reasoning: true,
            thinking_enabled: None,
            thinking_budget: None,
            reasoning_effort: None,
            thinking_param_style: None,
            reasoning_effort_options: None,
            thinking_budget_range: None,
            supports_agent_thinking: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
struct LocalModelConfig {
    max_context_length: u64,
    port: Option<u16>,
}

impl Default for LocalModelConfig {
    fn default() -> Self {
        Self {
            max_context_length: 65_536,
            port: None,
        }
    }
}

#[derive(Debug, Deserialize)]
struct CreateProviderRequest {
    id: String,
    name: String,
    #[serde(default)]
    default_base_url: String,
    #[serde(default)]
    api_key_prefix: String,
    #[serde(default = "default_chat_model")]
    chat_model: String,
    #[serde(default)]
    models: Vec<ModelRecord>,
}

#[derive(Debug, Deserialize)]
struct AddModelRequest {
    id: String,
    name: String,
    #[serde(default)]
    is_free: bool,
    #[serde(default)]
    supports_multimodal: Option<bool>,
    #[serde(default)]
    supports_image: Option<bool>,
    #[serde(default)]
    supports_video: Option<bool>,
    #[serde(default)]
    probe_source: Option<String>,
}

#[derive(Debug, Deserialize)]
struct VisibilityRequest {
    hidden: bool,
}

#[derive(Debug, Deserialize)]
struct TestModelRequest {
    model_id: String,
}

#[derive(Debug, Default, Deserialize)]
struct TestProviderRequest {
    #[serde(default)]
    api_key: Option<String>,
    #[serde(default)]
    base_url: Option<String>,
    #[serde(default)]
    chat_model: Option<String>,
    #[serde(default)]
    generate_kwargs: Option<Value>,
    #[serde(default)]
    custom_headers: Option<BTreeMap<String, String>>,
    #[serde(default)]
    auth_mode: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct DiscoverModelsRequest {
    #[serde(default)]
    api_key: Option<String>,
    #[serde(default)]
    base_url: Option<String>,
    #[serde(default)]
    chat_model: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DiscoverModelsQuery {
    #[serde(default = "default_true")]
    save: bool,
}

const fn default_true() -> bool {
    true
}

#[derive(Debug)]
struct ModelConnectionResult {
    success: bool,
    message: String,
    status: &'static str,
    http_status: Option<u16>,
    retryable: bool,
    checked_at: String,
}

#[derive(Debug, Default, Deserialize)]
struct ActiveModelQuery {
    #[serde(default = "default_active_scope")]
    scope: String,
    #[serde(default)]
    agent_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ActiveModelRequest {
    provider_id: String,
    model: String,
    scope: String,
    #[serde(default)]
    agent_id: Option<String>,
}

pub(super) fn router() -> Router<AppServer> {
    Router::new()
        .route("/api/models", get(list_providers))
        .route(
            "/api/models/active",
            get(active_models).put(set_active_model),
        )
        .route("/api/models/custom-providers", post(create_provider))
        .route(
            "/api/models/custom-providers/{provider_id}",
            delete(delete_provider),
        )
        .route("/api/models/{provider_id}/config", put(configure_provider))
        .route(
            "/api/models/{provider_id}/test",
            post(test_provider_connection),
        )
        .route("/api/models/{provider_id}/discover", post(discover_models))
        .route("/api/models/{provider_id}/models", post(add_model))
        .route(
            "/api/models/{provider_id}/models/test",
            post(test_model_connection),
        )
        .route(
            "/api/models/{provider_id}/models/{model_id}",
            delete(remove_model),
        )
        .route(
            "/api/models/{provider_id}/models/{model_id}/visibility",
            put(set_model_visibility),
        )
        .route(
            "/api/models/{provider_id}/models/{model_id}/config",
            put(configure_model),
        )
        .route(
            "/api/models/{provider_id}/models/{model_id}/probe-multimodal",
            post(probe_model_multimodal),
        )
        .route(
            "/api/local-models/config",
            get(get_local_model_config).put(put_local_model_config),
        )
}

pub(super) fn initialize(
    core: &Core,
    credentials: &dyn DesktopCredentialStore,
    workspace: &DesktopWorkspace,
) -> anyhow::Result<()> {
    let path = registry_path(workspace);
    if path.exists() {
        let mut registry =
            read_registry_from(workspace).map_err(|error| api_error_message(&error))?;
        if normalize_remote_capabilities(&mut registry) {
            bump_registry(&mut registry);
            write_registry_to(workspace, &registry).map_err(|error| api_error_message(&error))?;
        }
        validate_registry(&registry).map_err(|error| api_error_message(&error))?;
        let provider = registry
            .providers
            .get(&registry.active_provider_id)
            .ok_or_else(|| anyhow::anyhow!("Active model provider is invalid"))?;
        let model = core.read_config().config.default_model;
        let model = if all_models(provider).any(|candidate| candidate.id == model) {
            model
        } else {
            all_models(provider)
                .next()
                .map(|candidate| candidate.id.clone())
                .ok_or_else(|| anyhow::anyhow!("Active model provider has no models"))?
        };
        let secret = load_secret_from_ref(credentials, &provider.id).unwrap_or_else(|error| {
            tracing::warn!(
                provider_id = %provider.id,
                error = %error,
                "Desktop model credential could not be loaded"
            );
            None
        });
        core.write_config(ConfigWriteParams {
            base_url: Some(provider.base_url.clone()),
            default_model: Some(model),
        })?;
        core.set_runtime_api_key(secret)?;
        return Ok(());
    }
    let config = core.read_config().config;
    let registry = default_registry(
        &config.default_model,
        &config.base_url,
        config.api_key_configured,
    );
    write_registry_to(workspace, &registry).map_err(|error| api_error_message(&error))
}

async fn list_providers(State(server): State<AppServer>) -> Result<Json<Value>, ApiError> {
    let _guard = server.inner.desktop_models_lock.lock().await;
    let registry = read_registry(&server)?;
    Ok(Json(json!(provider_responses(&registry)?)))
}

async fn create_provider(
    State(server): State<AppServer>,
    Json(body): Json<CreateProviderRequest>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    validate_provider_id(&body.id)?;
    validate_provider_name(&body.name)?;
    validate_base_url(&body.default_base_url)?;
    validate_custom_chat_model(&body.chat_model)?;
    validate_optional_short_text(&body.api_key_prefix, "API key prefix")?;
    if body.models.len() > MAX_MODELS_PER_PROVIDER {
        return Err(payload_too_large("Too many provider models"));
    }
    let mut models = body.models;
    for model in &mut models {
        model.source = String::from("user");
        validate_model(model)?;
    }
    ensure_unique_models(&models)?;
    let _guard = server.inner.desktop_models_lock.lock().await;
    let mut registry = read_registry(&server)?;
    if registry.providers.len() >= MAX_PROVIDERS {
        return Err(payload_too_large("Too many model providers"));
    }
    if registry.providers.contains_key(&body.id) {
        return Err(bad_request(&format!(
            "Provider '{}' already exists",
            body.id
        )));
    }
    let record = ProviderRecord {
        id: body.id.clone(),
        name: body.name.trim().to_owned(),
        base_url: body.default_base_url.trim().to_owned(),
        api_key_prefix: body.api_key_prefix.trim().to_owned(),
        chat_model: body.chat_model,
        extra_models: models,
        is_custom: true,
        support_model_discovery: true,
        support_connection_check: true,
        ..ProviderRecord::default()
    };
    registry.providers.insert(body.id, record.clone());
    bump_registry(&mut registry);
    write_registry(&server, &registry)?;
    Ok((StatusCode::CREATED, Json(provider_response(&record)?)))
}

async fn configure_provider(
    State(server): State<AppServer>,
    Path(provider_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let submitted = object(&body, "Provider config must be an object")?;
    validate_provider_update(submitted)?;
    let _guard = server.inner.desktop_models_lock.lock().await;
    let mut registry = read_registry(&server)?;
    let previous_registry = registry.clone();
    let provider = registry
        .providers
        .get_mut(&provider_id)
        .ok_or_else(|| not_found(&format!("Provider '{provider_id}' not found")))?;
    let previous_secret = if submitted.contains_key("api_key") {
        load_provider_secret(&server, &provider_id).await?
    } else {
        None
    };
    apply_provider_update(provider, submitted);
    let next_secret = submitted
        .get("api_key")
        .and_then(Value::as_str)
        .map(str::trim);
    if let Some(secret) = next_secret {
        save_provider_secret(
            &server,
            &provider_id,
            (!secret.is_empty()).then_some(secret),
        )
        .await?;
        provider.api_key_configured = !secret.is_empty();
    }
    let response = provider_response(provider)?;
    bump_registry(&mut registry);
    if let Err(error) = write_registry(&server, &registry) {
        if submitted.contains_key("api_key") {
            let _ = save_provider_secret(&server, &provider_id, previous_secret.as_deref()).await;
        }
        return Err(error);
    }
    if registry.active_provider_id == provider_id
        && let Err(error) = apply_active_provider(&server, &registry).await
    {
        let _ = write_registry(&server, &previous_registry);
        if submitted.contains_key("api_key") {
            let _ = save_provider_secret(&server, &provider_id, previous_secret.as_deref()).await;
        }
        return Err(error);
    }
    Ok(Json(response))
}

async fn delete_provider(
    State(server): State<AppServer>,
    Path(provider_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let _guard = server.inner.desktop_models_lock.lock().await;
    let mut registry = read_registry(&server)?;
    let previous_registry = registry.clone();
    let provider = registry
        .providers
        .get(&provider_id)
        .ok_or_else(|| bad_request(&format!("Custom Provider '{provider_id}' not found")))?;
    if !provider.is_custom {
        return Err(bad_request("Built-in providers cannot be deleted"));
    }
    let previous_secret = load_provider_secret(&server, &provider_id).await?;
    let was_active = registry.active_provider_id == provider_id;
    registry.providers.remove(&provider_id);
    if was_active {
        registry.active_provider_id = String::from(DEFAULT_PROVIDER_ID);
    }
    bump_registry(&mut registry);
    write_registry(&server, &registry)?;
    if let Err(error) = save_provider_secret(&server, &provider_id, None).await {
        let _ = write_registry(&server, &previous_registry);
        return Err(error);
    }
    if was_active && let Err(error) = apply_active_provider(&server, &registry).await {
        let _ = write_registry(&server, &previous_registry);
        let _ = save_provider_secret(&server, &provider_id, previous_secret.as_deref()).await;
        return Err(error);
    }
    Ok(Json(json!(provider_responses(&registry)?)))
}

async fn add_model(
    State(server): State<AppServer>,
    Path(provider_id): Path<String>,
    Json(body): Json<AddModelRequest>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let model = ModelRecord {
        id: body.id.trim().to_owned(),
        name: body.name.trim().to_owned(),
        is_free: body.is_free,
        supports_multimodal: body.supports_multimodal,
        supports_image: body.supports_image,
        supports_video: body.supports_video,
        probe_source: body.probe_source,
        ..ModelRecord::default()
    };
    validate_model(&model)?;
    let _guard = server.inner.desktop_models_lock.lock().await;
    let mut registry = read_registry(&server)?;
    let provider = provider_mut(&mut registry, &provider_id)?;
    if provider
        .models
        .iter()
        .chain(provider.extra_models.iter())
        .any(|candidate| candidate.id == model.id)
    {
        return Err(bad_request(&format!(
            "Model '{}' already exists in provider '{provider_id}'",
            model.id
        )));
    }
    if model_count(provider) >= MAX_MODELS_PER_PROVIDER {
        return Err(payload_too_large("Too many provider models"));
    }
    provider.extra_models.push(model);
    let response = provider_response(provider)?;
    bump_registry(&mut registry);
    write_registry(&server, &registry)?;
    Ok((StatusCode::CREATED, Json(response)))
}

async fn test_provider_connection(
    State(server): State<AppServer>,
    Path(provider_id): Path<String>,
    body: Bytes,
) -> Result<Json<Value>, ApiError> {
    let body = optional_json_body::<TestProviderRequest>(&body)?;
    validate_test_provider_request(&body)?;
    let provider = {
        let _guard = server.inner.desktop_models_lock.lock().await;
        read_registry(&server)?
            .providers
            .get(&provider_id)
            .cloned()
            .ok_or_else(|| not_found(&format!("Provider '{provider_id}' not found")))?
    };
    if !provider.support_connection_check {
        return Err(bad_request("Provider connection testing is not supported"));
    }
    let stored_secret = load_provider_secret(&server, &provider_id).await?;
    let secret = body
        .api_key
        .as_deref()
        .map(str::trim)
        .map(str::to_owned)
        .or(stored_secret)
        .filter(|secret| !secret.is_empty());
    let remote = remote_provider(&provider, &body, secret);
    let fallback_model = provider
        .models
        .first()
        .or_else(|| provider.extra_models.first())
        .map_or("", |model| model.id.as_str());
    let result = desktop_model_remote::test_provider(&remote, fallback_model).await;
    Ok(Json(json!({
        "success": result.success,
        "message": result.message,
        "status": result.status,
        "http_status": result.http_status,
        "retryable": result.retryable,
        "checked_at": Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
        "verification": "provider_only"
    })))
}

async fn discover_models(
    State(server): State<AppServer>,
    Path(provider_id): Path<String>,
    Query(query): Query<DiscoverModelsQuery>,
    body: Bytes,
) -> Result<Json<Value>, ApiError> {
    let body = optional_json_body::<DiscoverModelsRequest>(&body)?;
    validate_discovery_request(&body)?;
    let (provider, revision) = {
        let _guard = server.inner.desktop_models_lock.lock().await;
        let registry = read_registry(&server)?;
        let provider = registry
            .providers
            .get(&provider_id)
            .cloned()
            .ok_or_else(|| not_found(&format!("Provider '{provider_id}' not found")))?;
        (provider, registry.revision)
    };
    if !provider.support_model_discovery {
        return Err(bad_request("Provider model discovery is not supported"));
    }
    let previous_secret = load_provider_secret(&server, &provider_id).await?;
    let secret = body
        .api_key
        .as_deref()
        .map(str::trim)
        .map(str::to_owned)
        .or_else(|| previous_secret.clone())
        .filter(|secret| !secret.is_empty());
    let mut request_provider = provider.clone();
    if let Some(base_url) = &body.base_url {
        request_provider.base_url = base_url.trim().to_owned();
    }
    if let Some(chat_model) = &body.chat_model {
        request_provider.chat_model.clone_from(chat_model);
    }
    let remote = remote_provider(
        &request_provider,
        &TestProviderRequest::default(),
        secret.clone(),
    );
    let discovered = match desktop_model_remote::discover_models(&remote).await {
        Ok(models) if !models.is_empty() => models,
        Ok(_) => {
            let failure = desktop_model_remote::RemoteFailure {
                message: String::from("Provider returned no models"),
                error_kind: "incompatible_api",
                http_status: Some(200),
                retryable: false,
            };
            if query.save {
                record_discovery_failure(&server, &provider_id, revision, &failure.message).await?;
            }
            return Ok(Json(discovery_failure_response(&provider, &failure)?));
        }
        Err(failure) => {
            if query.save {
                record_discovery_failure(&server, &provider_id, revision, &failure.message).await?;
            }
            return Ok(Json(discovery_failure_response(&provider, &failure)?));
        }
    };
    let synced_at = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);
    let previous_ids = provider
        .discovered_models
        .iter()
        .map(|model| model.id.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    let discovered_count = discovered
        .iter()
        .filter(|model| !previous_ids.contains(model.id.as_str()))
        .count();
    let mut records = discovered
        .into_iter()
        .map(|model| discovered_model_record(model, &synced_at))
        .collect::<Vec<_>>();
    preserve_discovered_state(&provider.discovered_models, &mut records);
    if query.save {
        persist_discovery(
            &server,
            &provider_id,
            revision,
            &body,
            previous_secret.as_deref(),
            secret.as_deref(),
            &records,
            &synced_at,
        )
        .await?;
    }
    Ok(Json(json!({
        "success": true,
        "models": records,
        "discovered_count": discovered_count,
        "last_synced_at": synced_at,
        "used_static_fallback": false,
        "message": "",
        "error_kind": null
    })))
}

async fn probe_model_multimodal(
    State(server): State<AppServer>,
    Path((provider_id, model_id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    validate_model_id(&model_id)?;
    let (provider, revision) = {
        let _guard = server.inner.desktop_models_lock.lock().await;
        let registry = read_registry(&server)?;
        let provider = registry
            .providers
            .get(&provider_id)
            .cloned()
            .ok_or_else(|| not_found(&format!("Provider '{provider_id}' not found")))?;
        if !all_models(&provider).any(|model| model.id == model_id) {
            return Err(not_found(&format!(
                "Model '{model_id}' not found in provider '{provider_id}'"
            )));
        }
        (provider, registry.revision)
    };
    let secret = load_provider_secret(&server, &provider_id).await?;
    let remote = remote_provider(&provider, &TestProviderRequest::default(), secret);
    let result = desktop_model_remote::probe_multimodal(&remote, &model_id).await;
    let _guard = server.inner.desktop_models_lock.lock().await;
    let mut registry = read_registry(&server)?;
    if registry.revision != revision {
        return Err(conflict("Model configuration changed while probing"));
    }
    let provider = provider_mut(&mut registry, &provider_id)?;
    for model in all_models_mut(provider).filter(|model| model.id == model_id) {
        model.supports_image = Some(result.supports_image);
        model.supports_video = Some(result.supports_video);
        model.supports_multimodal = Some(result.supports_image || result.supports_video);
        model.probe_source = Some(String::from("probed"));
    }
    bump_registry(&mut registry);
    write_registry(&server, &registry)?;
    Ok(Json(json!({
        "supports_image": result.supports_image,
        "supports_video": result.supports_video,
        "supports_multimodal": result.supports_image || result.supports_video,
        "image_message": result.image_message,
        "video_message": result.video_message
    })))
}

fn validate_test_provider_request(body: &TestProviderRequest) -> Result<(), ApiError> {
    if let Some(api_key) = &body.api_key {
        validate_api_key(api_key)?;
    }
    if let Some(base_url) = &body.base_url {
        validate_base_url(base_url)?;
    }
    if let Some(chat_model) = &body.chat_model {
        validate_chat_model(chat_model)?;
    }
    if let Some(generate_kwargs) = &body.generate_kwargs {
        validate_json_object(generate_kwargs, "generate_kwargs")?;
    }
    if let Some(headers) = &body.custom_headers {
        validate_headers(
            &serde_json::to_value(headers)
                .map_err(|_| bad_request("custom_headers must be an object"))?,
        )?;
    }
    if let Some(auth_mode) = &body.auth_mode
        && !matches!(auth_mode.as_str(), "api_key" | "auth_token")
    {
        return Err(bad_request("auth_mode must be api_key or auth_token"));
    }
    Ok(())
}

fn optional_json_body<T>(body: &[u8]) -> Result<T, ApiError>
where
    T: Default + serde::de::DeserializeOwned,
{
    if body.iter().all(u8::is_ascii_whitespace) {
        return Ok(T::default());
    }
    serde_json::from_slice(body).map_err(|_| bad_request("Request body must be valid JSON"))
}

fn validate_discovery_request(body: &DiscoverModelsRequest) -> Result<(), ApiError> {
    if let Some(api_key) = &body.api_key {
        validate_api_key(api_key)?;
    }
    if let Some(base_url) = &body.base_url {
        validate_base_url(base_url)?;
    }
    if let Some(chat_model) = &body.chat_model {
        validate_chat_model(chat_model)?;
    }
    Ok(())
}

fn remote_provider(
    provider: &ProviderRecord,
    body: &TestProviderRequest,
    secret: Option<String>,
) -> desktop_model_remote::RemoteProvider {
    desktop_model_remote::RemoteProvider {
        base_url: body
            .base_url
            .as_deref()
            .unwrap_or(&provider.base_url)
            .trim()
            .to_owned(),
        chat_model: body
            .chat_model
            .clone()
            .unwrap_or_else(|| provider.chat_model.clone()),
        custom_headers: body
            .custom_headers
            .as_ref()
            .unwrap_or(&provider.custom_headers)
            .iter()
            .map(|(name, value)| (name.clone(), value.clone()))
            .collect(),
        auth_mode: body
            .auth_mode
            .clone()
            .unwrap_or_else(|| provider.auth_mode.clone()),
        secret,
    }
}

fn discovered_model_record(
    model: desktop_model_remote::DiscoveredModel,
    synced_at: &str,
) -> ModelRecord {
    ModelRecord {
        id: model.id,
        name: model.name,
        source: String::from("discovered"),
        discovery_origin: Some(String::from("api")),
        max_input_length: model.max_input_length.unwrap_or(128_000),
        max_input_length_auto_detected: model.max_input_length,
        max_output_length: model.max_output_length,
        max_output_length_source: if model.max_output_length.is_some() {
            String::from("api")
        } else {
            String::from("unknown")
        },
        max_output_length_updated_at: model
            .max_output_length
            .is_some()
            .then(|| synced_at.to_owned()),
        ..ModelRecord::default()
    }
}

fn preserve_discovered_state(previous: &[ModelRecord], current: &mut [ModelRecord]) {
    for model in current {
        let Some(old) = previous.iter().find(|candidate| candidate.id == model.id) else {
            continue;
        };
        model.supports_multimodal = old.supports_multimodal;
        model.supports_image = old.supports_image;
        model.supports_video = old.supports_video;
        model.probe_source.clone_from(&old.probe_source);
        model
            .availability_status
            .clone_from(&old.availability_status);
    }
}

#[allow(clippy::too_many_arguments)]
async fn persist_discovery(
    server: &AppServer,
    provider_id: &str,
    expected_revision: u64,
    body: &DiscoverModelsRequest,
    previous_secret: Option<&str>,
    request_secret: Option<&str>,
    records: &[ModelRecord],
    synced_at: &str,
) -> Result<(), ApiError> {
    let _guard = server.inner.desktop_models_lock.lock().await;
    let mut registry = read_registry(server)?;
    if registry.revision != expected_revision {
        return Err(conflict("Model discovery was superseded by a newer update"));
    }
    let previous_registry = registry.clone();
    let provider = provider_mut(&mut registry, provider_id)?;
    if let Some(base_url) = &body.base_url {
        base_url.trim().clone_into(&mut provider.base_url);
    }
    if let Some(chat_model) = &body.chat_model {
        provider.chat_model.clone_from(chat_model);
    }
    if body.api_key.is_some() {
        provider.api_key_configured = request_secret.is_some();
    }
    provider.discovered_models = records.to_vec();
    provider.models_last_synced_at = Some(synced_at.to_owned());
    provider.models_last_sync_error = None;
    provider.models_syncing = false;
    apply_discovered_metadata(provider, records);
    bump_registry(&mut registry);

    if body.api_key.is_some() {
        save_provider_secret(server, provider_id, request_secret).await?;
    }
    if let Err(error) = write_registry(server, &registry) {
        if body.api_key.is_some() {
            let _ = save_provider_secret(server, provider_id, previous_secret).await;
        }
        return Err(error);
    }
    if registry.active_provider_id == provider_id
        && let Err(error) = apply_active_provider(server, &registry).await
    {
        let _ = write_registry(server, &previous_registry);
        if body.api_key.is_some() {
            let _ = save_provider_secret(server, provider_id, previous_secret).await;
        }
        return Err(error);
    }
    Ok(())
}

fn apply_discovered_metadata(provider: &mut ProviderRecord, records: &[ModelRecord]) {
    for configured in provider
        .models
        .iter_mut()
        .chain(provider.extra_models.iter_mut())
    {
        let Some(discovered) = records.iter().find(|model| model.id == configured.id) else {
            continue;
        };
        configured.max_input_length_auto_detected = discovered.max_input_length_auto_detected;
        if !configured.max_input_length_configured
            && let Some(max_input_length) = discovered.max_input_length_auto_detected
        {
            configured.max_input_length = max_input_length;
        }
        if discovered.max_output_length.is_some() && configured.max_output_length_source != "user" {
            configured.max_output_length = discovered.max_output_length;
            configured
                .max_output_length_source
                .clone_from(&discovered.max_output_length_source);
            configured
                .max_output_length_updated_at
                .clone_from(&discovered.max_output_length_updated_at);
        }
    }
}

async fn record_discovery_failure(
    server: &AppServer,
    provider_id: &str,
    expected_revision: u64,
    message: &str,
) -> Result<(), ApiError> {
    let _guard = server.inner.desktop_models_lock.lock().await;
    let mut registry = read_registry(server)?;
    if registry.revision != expected_revision {
        return Err(conflict("Model discovery was superseded by a newer update"));
    }
    let provider = provider_mut(&mut registry, provider_id)?;
    provider.models_last_sync_error = Some(message.to_owned());
    provider.models_syncing = false;
    bump_registry(&mut registry);
    write_registry(server, &registry)
}

fn discovery_failure_response(
    provider: &ProviderRecord,
    failure: &desktop_model_remote::RemoteFailure,
) -> Result<Value, ApiError> {
    Ok(json!({
        "success": false,
        "models": serde_json::to_value(&provider.discovered_models)
            .map_err(|_| internal("Discovered models could not be encoded"))?,
        "discovered_count": 0,
        "last_synced_at": provider.models_last_synced_at,
        "used_static_fallback": true,
        "message": failure.message,
        "error_kind": discovery_error_kind(failure)
    }))
}

fn discovery_error_kind(failure: &desktop_model_remote::RemoteFailure) -> &'static str {
    match failure.error_kind {
        "permission_denied" if failure.http_status == Some(401) => "authentication",
        "permission_denied" => "authorization",
        "incompatible_api" => "invalid_response",
        "transient_error" if failure.http_status.is_none() => "network",
        "model_not_found" => "unsupported",
        _ => "provider_unavailable",
    }
}

fn normalize_remote_capabilities(registry: &mut ProviderRegistry) -> bool {
    let mut changed = false;
    for provider in registry
        .providers
        .values_mut()
        .filter(|provider| provider.is_custom || provider.id == DEFAULT_PROVIDER_ID)
    {
        if !provider.support_model_discovery {
            provider.support_model_discovery = true;
            changed = true;
        }
        if !provider.support_connection_check {
            provider.support_connection_check = true;
            changed = true;
        }
    }
    changed
}

async fn test_model_connection(
    State(server): State<AppServer>,
    Path(provider_id): Path<String>,
    Json(body): Json<TestModelRequest>,
) -> Result<Json<Value>, ApiError> {
    validate_model_id(&body.model_id)?;
    let provider = {
        let _guard = server.inner.desktop_models_lock.lock().await;
        read_registry(&server)?
            .providers
            .get(&provider_id)
            .cloned()
            .ok_or_else(|| not_found(&format!("Provider '{provider_id}' not found")))?
    };
    let secret = load_provider_secret(&server, &provider_id).await?;
    let result = check_model_connection(&provider, secret.as_deref(), &body.model_id).await;

    {
        let _guard = server.inner.desktop_models_lock.lock().await;
        let mut registry = read_registry(&server)?;
        let provider = provider_mut(&mut registry, &provider_id)?;
        let mut updated = false;
        for model in all_models_mut(provider).filter(|model| model.id == body.model_id) {
            model.availability_status = String::from(result.status);
            updated = true;
        }
        if updated {
            bump_registry(&mut registry);
            write_registry(&server, &registry)?;
        }
    }

    Ok(Json(json!({
        "success": result.success,
        "message": if result.success {
            String::from("Model connection successful")
        } else {
            format!("Model connection failed: {}", result.message)
        },
        "status": result.status,
        "http_status": result.http_status,
        "retryable": result.retryable,
        "checked_at": result.checked_at,
        "verification": "live"
    })))
}

async fn check_model_connection(
    provider: &ProviderRecord,
    secret: Option<&str>,
    model_id: &str,
) -> ModelConnectionResult {
    let checked_at = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);
    let endpoint = match provider.chat_model.as_str() {
        "OpenAIChatModel" => "chat/completions",
        "OpenAIResponseModel" => "responses",
        "AnthropicChatModel" => "messages",
        _ => {
            return failed_model_connection(
                "incompatible_api",
                None,
                false,
                "Unsupported provider protocol",
                checked_at,
            );
        }
    };
    let url = match model_endpoint(&provider.base_url, &provider.chat_model, endpoint) {
        Ok(url) => url,
        Err(message) => {
            return failed_model_connection("transient_error", None, true, &message, checked_at);
        }
    };
    let client = match reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(MODEL_TEST_TIMEOUT_SECONDS))
        .build()
    {
        Ok(client) => client,
        Err(error) => {
            return failed_model_connection(
                "transient_error",
                None,
                true,
                &error.to_string(),
                checked_at,
            );
        }
    };
    let body = model_test_body(&provider.chat_model, model_id);
    let mut request = client.post(url).json(&body);
    for (name, value) in &provider.custom_headers {
        request = request.header(name, value);
    }
    if let Some(secret) = secret {
        request = if provider.chat_model == "AnthropicChatModel" && provider.auth_mode == "api_key"
        {
            request
                .header("x-api-key", secret)
                .header("anthropic-version", "2023-06-01")
        } else {
            request.bearer_auth(secret)
        };
    }
    let response = match request.send().await {
        Ok(response) => response,
        Err(error) => {
            return failed_model_connection(
                "transient_error",
                None,
                true,
                &error.to_string(),
                checked_at,
            );
        }
    };
    let http_status = response.status().as_u16();
    if response.status().is_success() {
        return ModelConnectionResult {
            success: true,
            message: String::new(),
            status: "available",
            http_status: Some(http_status),
            retryable: false,
            checked_at,
        };
    }
    let detail = redact_model_secret(&limited_response_text(response).await, secret);
    let (status, retryable) = classify_model_failure(http_status, &detail);
    failed_model_connection(
        status,
        Some(http_status),
        retryable,
        &format!("HTTP {http_status}: {detail}"),
        checked_at,
    )
}

fn model_endpoint(base_url: &str, chat_model: &str, endpoint: &str) -> Result<url::Url, String> {
    let base_url = base_url.trim();
    if base_url.is_empty() {
        return Err(String::from("Provider Base URL is empty"));
    }
    let has_v1_suffix = url::Url::parse(base_url)
        .is_ok_and(|url| url.path().trim_end_matches('/').ends_with("/v1"));
    let endpoint = if chat_model == "AnthropicChatModel" && !has_v1_suffix {
        format!("v1/{endpoint}")
    } else {
        endpoint.to_owned()
    };
    url::Url::parse(&format!("{}/{endpoint}", base_url.trim_end_matches('/')))
        .map_err(|error| format!("Provider Base URL is invalid: {error}"))
}

fn model_test_body(chat_model: &str, model_id: &str) -> Value {
    match chat_model {
        "OpenAIResponseModel" => json!({
            "model": model_id,
            "input": "ping",
            "max_output_tokens": 20
        }),
        "AnthropicChatModel" => json!({
            "model": model_id,
            "max_tokens": 20,
            "messages": [{"role": "user", "content": "ping"}]
        }),
        _ => json!({
            "model": model_id,
            "max_tokens": 20,
            "stream": false,
            "messages": [{"role": "user", "content": [{
                "type": "text",
                "text": "ping"
            }]}]
        }),
    }
}

async fn limited_response_text(response: reqwest::Response) -> String {
    let mut bytes = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let Ok(chunk) = chunk else {
            break;
        };
        let remaining = MODEL_TEST_ERROR_BODY_BYTES.saturating_sub(bytes.len());
        if remaining == 0 {
            break;
        }
        bytes.extend_from_slice(&chunk[..chunk.len().min(remaining)]);
    }
    let text = String::from_utf8_lossy(&bytes).trim().to_owned();
    if text.is_empty() {
        String::from("Provider returned an empty error response")
    } else {
        text
    }
}

fn classify_model_failure(http_status: u16, detail: &str) -> (&'static str, bool) {
    let detail = detail.to_ascii_lowercase();
    if matches!(http_status, 401 | 403)
        || [
            "unauthorized",
            "forbidden",
            "permission denied",
            "invalid api key",
            "incorrect api key",
            "authentication",
            "not activated",
            "not enabled",
        ]
        .iter()
        .any(|marker| detail.contains(marker))
    {
        ("permission_denied", false)
    } else if http_status == 404
        || [
            "model not found",
            "unknown model",
            "does not exist",
            "no such model",
        ]
        .iter()
        .any(|marker| detail.contains(marker))
    {
        ("model_not_found", false)
    } else if [
        "unsupported model",
        "does not support chat",
        "not support chat",
        "chat completions is not supported",
        "incompatible api",
        "incompatible endpoint",
        "unsupported endpoint",
    ]
    .iter()
    .any(|marker| detail.contains(marker))
    {
        ("incompatible_api", false)
    } else if http_status == 429 || detail.contains("rate limit") {
        ("rate_limited", true)
    } else {
        ("transient_error", true)
    }
}

fn redact_model_secret(message: &str, secret: Option<&str>) -> String {
    match secret.filter(|secret| !secret.is_empty()) {
        Some(secret) => message.replace(secret, "[REDACTED]"),
        None => message.to_owned(),
    }
}

fn failed_model_connection(
    status: &'static str,
    http_status: Option<u16>,
    retryable: bool,
    message: &str,
    checked_at: String,
) -> ModelConnectionResult {
    ModelConnectionResult {
        success: false,
        message: message.to_owned(),
        status,
        http_status,
        retryable,
        checked_at,
    }
}

async fn remove_model(
    State(server): State<AppServer>,
    Path((provider_id, model_id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    validate_model_id(&model_id)?;
    let _guard = server.inner.desktop_models_lock.lock().await;
    let mut registry = read_registry(&server)?;
    let active = registry.active_provider_id == provider_id
        && active_model_id(&server, &registry) == model_id;
    let provider = provider_mut(&mut registry, &provider_id)?;
    if active {
        return Err(bad_request("The active model cannot be removed"));
    }
    let before = model_count(provider);
    provider.extra_models.retain(|model| model.id != model_id);
    if provider.is_custom {
        provider.models.retain(|model| model.id != model_id);
        provider
            .discovered_models
            .retain(|model| model.id != model_id);
    }
    provider.hidden_model_ids.retain(|id| id != &model_id);
    if before == model_count(provider) {
        return Err(bad_request(&format!("Model '{model_id}' not found")));
    }
    let response = provider_response(provider)?;
    bump_registry(&mut registry);
    write_registry(&server, &registry)?;
    Ok(Json(response))
}

async fn set_model_visibility(
    State(server): State<AppServer>,
    Path((provider_id, model_id)): Path<(String, String)>,
    Json(body): Json<VisibilityRequest>,
) -> Result<Json<Value>, ApiError> {
    validate_model_id(&model_id)?;
    let _guard = server.inner.desktop_models_lock.lock().await;
    let mut registry = read_registry(&server)?;
    let provider = provider_mut(&mut registry, &provider_id)?;
    if !all_models(provider).any(|model| model.id == model_id) {
        return Err(not_found(&format!("Model '{model_id}' not found")));
    }
    if body.hidden {
        if !provider.hidden_model_ids.contains(&model_id) {
            provider.hidden_model_ids.push(model_id);
            provider.hidden_model_ids.sort();
        }
    } else {
        provider.hidden_model_ids.retain(|id| id != &model_id);
    }
    let response = provider_response(provider)?;
    bump_registry(&mut registry);
    write_registry(&server, &registry)?;
    Ok(Json(response))
}

async fn configure_model(
    State(server): State<AppServer>,
    Path((provider_id, model_id)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let submitted = object(&body, "Model config must be an object")?;
    validate_model_update(submitted)?;
    let _guard = server.inner.desktop_models_lock.lock().await;
    let mut registry = read_registry(&server)?;
    let provider = provider_mut(&mut registry, &provider_id)?;
    let model = all_models_mut(provider)
        .find(|model| model.id == model_id)
        .ok_or_else(|| not_found(&format!("Model '{model_id}' not found")))?;
    apply_model_update(model, submitted)?;
    validate_model(model)?;
    let response = provider_response(provider)?;
    bump_registry(&mut registry);
    write_registry(&server, &registry)?;
    Ok(Json(response))
}

async fn get_local_model_config(
    State(server): State<AppServer>,
) -> Result<Json<LocalModelConfig>, ApiError> {
    let _guard = server.inner.desktop_models_lock.lock().await;
    Ok(Json(read_registry(&server)?.local_model))
}

async fn put_local_model_config(
    State(server): State<AppServer>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let submitted = object(&body, "Local model config must be an object")?;
    validate_local_model_update(submitted)?;
    let _guard = server.inner.desktop_models_lock.lock().await;
    let mut registry = read_registry(&server)?;
    if let Some(value) = submitted.get("max_context_length") {
        registry.local_model.max_context_length = value
            .as_u64()
            .ok_or_else(|| bad_request("max_context_length must be an integer"))?;
    }
    if let Some(value) = submitted.get("port") {
        registry.local_model.port = if value.is_null() {
            None
        } else {
            Some(
                u16::try_from(
                    value
                        .as_u64()
                        .ok_or_else(|| bad_request("port must be an integer or null"))?,
                )
                .map_err(|_| bad_request("port must be between 1 and 65535"))?,
            )
        };
    }
    if let Some(value) = submitted.get("generate_kwargs") {
        registry.local_generate_kwargs = value
            .as_object()
            .cloned()
            .ok_or_else(|| bad_request("generate_kwargs must be an object"))?;
    }
    bump_registry(&mut registry);
    write_registry(&server, &registry)?;
    Ok(Json(json!({
        "status": "ok",
        "message": "Local model settings updated"
    })))
}

async fn active_models(
    State(server): State<AppServer>,
    headers: HeaderMap,
    Query(query): Query<ActiveModelQuery>,
) -> Result<Json<Value>, ApiError> {
    if !matches!(query.scope.as_str(), "effective" | "global" | "agent") {
        return Err(bad_request("model scope is invalid"));
    }
    let _guard = server.inner.desktop_models_lock.lock().await;
    let registry = read_registry(&server)?;
    if query.scope == "global" {
        let model = active_model_id(&server, &registry);
        return Ok(Json(active_model_response(
            &registry,
            &registry.active_provider_id,
            &model,
        )?));
    }
    let agent_id = match query.agent_id {
        Some(agent_id) => agent_id,
        None if query.scope == "agent" => {
            return Err(bad_request("agent_id is required when scope is 'agent'"));
        }
        None => super::desktop_agents::requested_agent_id(&headers)?,
    };
    let config = super::desktop_agents::config_for_agent(&server, &agent_id).await?;
    if let Some(active) = config.get("active_model").and_then(Value::as_object) {
        let provider_id = active.get("provider_id").and_then(Value::as_str);
        let model_id = active.get("model").and_then(Value::as_str);
        if let (Some(provider_id), Some(model_id)) = (provider_id, model_id)
            && provider_has_model(&registry, provider_id, model_id)
        {
            return Ok(Json(active_model_response(
                &registry,
                provider_id,
                model_id,
            )?));
        }
    }
    if query.scope == "agent" {
        return Ok(Json(json!({
            "active_llm": null,
            "effective_max_input_length": null
        })));
    }
    let model = active_model_id(&server, &registry);
    Ok(Json(active_model_response(
        &registry,
        &registry.active_provider_id,
        &model,
    )?))
}

async fn set_active_model(
    State(server): State<AppServer>,
    Json(body): Json<ActiveModelRequest>,
) -> Result<Json<Value>, ApiError> {
    if !matches!(body.scope.as_str(), "global" | "agent") {
        return Err(bad_request("model scope is invalid"));
    }
    validate_provider_id(&body.provider_id)?;
    validate_model_id(&body.model)?;
    let _guard = server.inner.desktop_models_lock.lock().await;
    let mut registry = read_registry(&server)?;
    if !provider_has_model(&registry, &body.provider_id, &body.model) {
        return Err(bad_request(&format!(
            "Model '{}' not found in provider '{}'",
            body.model, body.provider_id
        )));
    }
    let response = active_model_response(&registry, &body.provider_id, &body.model)?;
    if body.scope == "agent" {
        let agent_id = body.agent_id.as_deref().unwrap_or("default");
        super::desktop_agents::replace_config_field(
            &server,
            agent_id,
            "active_model",
            json!({"provider_id": body.provider_id, "model": body.model}),
        )
        .await?;
        return Ok(Json(response));
    }
    let previous = registry.clone();
    registry.active_provider_id.clone_from(&body.provider_id);
    bump_registry(&mut registry);
    write_registry(&server, &registry)?;
    if let Err(error) = apply_active_provider_with_model(&server, &registry, &body.model).await {
        let _ = write_registry(&server, &previous);
        return Err(error);
    }
    Ok(Json(response))
}

fn default_registry(model: &str, base_url: &str, api_key_configured: bool) -> ProviderRegistry {
    let default_provider = ProviderRecord {
        id: String::from(DEFAULT_PROVIDER_ID),
        name: String::from("OpenAI Compatible"),
        base_url: base_url.to_owned(),
        api_key_configured,
        support_model_discovery: true,
        support_connection_check: true,
        models: vec![ModelRecord {
            id: model.to_owned(),
            name: model.to_owned(),
            source: String::from("builtin"),
            ..ModelRecord::default()
        }],
        ..ProviderRecord::default()
    };
    let mut providers = embedded_builtin_providers();
    providers.insert(String::from(DEFAULT_PROVIDER_ID), default_provider);
    ProviderRegistry {
        schema_version: REGISTRY_SCHEMA_VERSION,
        revision: 0,
        active_provider_id: String::from(DEFAULT_PROVIDER_ID),
        providers,
        local_model: LocalModelConfig::default(),
        local_generate_kwargs: Map::new(),
    }
}

fn embedded_builtin_providers() -> BTreeMap<String, ProviderRecord> {
    serde_json::from_str::<Vec<ProviderRecord>>(BUILTIN_PROVIDERS_JSON)
        .expect("embedded provider catalog should be valid")
        .into_iter()
        .map(|provider| (provider.id.clone(), provider))
        .collect()
}

fn provider_responses(registry: &ProviderRegistry) -> Result<Vec<Value>, ApiError> {
    let mut responses = Vec::with_capacity(registry.providers.len());
    for provider_id in BUILTIN_PROVIDER_ORDER {
        if let Some(provider) = registry.providers.get(*provider_id) {
            responses.push(provider_response(provider)?);
        }
    }
    for provider in registry
        .providers
        .values()
        .filter(|provider| !BUILTIN_PROVIDER_ORDER.contains(&provider.id.as_str()))
    {
        responses.push(provider_response(provider)?);
    }
    Ok(responses)
}

fn provider_response(provider: &ProviderRecord) -> Result<Value, ApiError> {
    let mut value = serde_json::to_value(provider)
        .map_err(|_| internal("Model provider could not be encoded"))?;
    let object = value
        .as_object_mut()
        .ok_or_else(|| internal("Model provider could not be encoded"))?;
    object.remove("api_key_configured");
    object.insert(
        String::from("api_key"),
        Value::String(if provider.api_key_configured {
            String::from("********")
        } else {
            String::new()
        }),
    );
    Ok(value)
}

fn active_model_response(
    registry: &ProviderRegistry,
    provider_id: &str,
    model_id: &str,
) -> Result<Value, ApiError> {
    let provider = registry
        .providers
        .get(provider_id)
        .ok_or_else(|| internal("Active model provider is invalid"))?;
    let model = all_models(provider)
        .find(|model| model.id == model_id)
        .ok_or_else(|| internal("Active model is invalid"))?;
    Ok(json!({
        "active_llm": {"provider_id": provider_id, "model": model_id},
        "effective_max_input_length": model.max_input_length
    }))
}

fn active_model_id(server: &AppServer, registry: &ProviderRegistry) -> String {
    let core_model = server.inner.core.read_config().config.default_model;
    if provider_has_model(registry, &registry.active_provider_id, &core_model) {
        return core_model;
    }
    registry
        .providers
        .get(&registry.active_provider_id)
        .and_then(|provider| all_models(provider).next())
        .map(|model| model.id.clone())
        .unwrap_or_default()
}

async fn apply_active_provider(
    server: &AppServer,
    registry: &ProviderRegistry,
) -> Result<(), ApiError> {
    let model = active_model_id(server, registry);
    apply_active_provider_with_model(server, registry, &model).await
}

async fn apply_active_provider_with_model(
    server: &AppServer,
    registry: &ProviderRegistry,
    model: &str,
) -> Result<(), ApiError> {
    let provider = registry
        .providers
        .get(&registry.active_provider_id)
        .ok_or_else(|| internal("Active model provider is invalid"))?;
    let secret = load_provider_secret(server, &provider.id).await?;
    server
        .inner
        .core
        .write_config(ConfigWriteParams {
            base_url: Some(provider.base_url.clone()),
            default_model: Some(model.to_owned()),
        })
        .map_err(|error| internal(&error.to_string()))?;
    server
        .inner
        .core
        .set_runtime_api_key(secret)
        .map_err(|error| internal(&error.to_string()))
}

fn apply_provider_update(provider: &mut ProviderRecord, submitted: &Map<String, Value>) {
    if provider.is_custom
        && let Some(name) = submitted.get("name").and_then(Value::as_str)
        && !name.trim().is_empty()
    {
        name.trim().clone_into(&mut provider.name);
    }
    if let Some(base_url) = submitted.get("base_url").and_then(Value::as_str) {
        base_url.trim().clone_into(&mut provider.base_url);
    }
    if let Some(chat_model) = submitted.get("chat_model").and_then(Value::as_str) {
        chat_model.clone_into(&mut provider.chat_model);
    }
    if let Some(kwargs) = submitted.get("generate_kwargs") {
        provider.generate_kwargs = kwargs.as_object().cloned().unwrap_or_default();
    }
    if let Some(headers) = submitted.get("custom_headers") {
        provider.custom_headers = headers
            .as_object()
            .map(|headers| {
                headers
                    .iter()
                    .filter_map(|(key, value)| {
                        value.as_str().map(|value| (key.clone(), value.to_owned()))
                    })
                    .collect()
            })
            .unwrap_or_default();
    }
    if let Some(auth_mode) = submitted.get("auth_mode").and_then(Value::as_str) {
        auth_mode.clone_into(&mut provider.auth_mode);
    }
}

fn apply_model_update(
    model: &mut ModelRecord,
    submitted: &Map<String, Value>,
) -> Result<(), ApiError> {
    if let Some(value) = submitted.get("max_input_length") {
        model.max_input_length = value
            .as_u64()
            .ok_or_else(|| bad_request("max_input_length must be an integer"))?;
        model.max_input_length_configured = true;
    }
    if let Some(value) = submitted.get("generate_kwargs") {
        model.generate_kwargs = value.as_object().cloned().unwrap_or_default();
    }
    if let Some(value) = submitted.get("relay_reasoning") {
        model.relay_reasoning = value
            .as_bool()
            .ok_or_else(|| bad_request("relay_reasoning must be a boolean"))?;
    }
    if let Some(value) = submitted.get("thinking_enabled") {
        model.thinking_enabled = optional_bool(value, "thinking_enabled")?;
    }
    if let Some(value) = submitted.get("thinking_budget") {
        model.thinking_budget = optional_u64(value, "thinking_budget")?;
    }
    if let Some(value) = submitted.get("reasoning_effort") {
        model.reasoning_effort = optional_string(value, "reasoning_effort")?;
    }
    Ok(())
}

fn validate_provider_update(submitted: &Map<String, Value>) -> Result<(), ApiError> {
    const ALLOWED: [&str; 7] = [
        "api_key",
        "base_url",
        "name",
        "chat_model",
        "generate_kwargs",
        "custom_headers",
        "auth_mode",
    ];
    reject_unknown_fields(submitted, &ALLOWED, "provider")?;
    if let Some(value) = submitted.get("api_key") {
        let secret = value
            .as_str()
            .ok_or_else(|| bad_request("api_key must be a string"))?;
        validate_api_key(secret)?;
    }
    if let Some(value) = submitted.get("base_url") {
        validate_base_url(
            value
                .as_str()
                .ok_or_else(|| bad_request("base_url must be a string"))?,
        )?;
    }
    if let Some(value) = submitted.get("name") {
        validate_provider_name(
            value
                .as_str()
                .ok_or_else(|| bad_request("name must be a string"))?,
        )?;
    }
    if let Some(value) = submitted.get("chat_model") {
        validate_chat_model(
            value
                .as_str()
                .ok_or_else(|| bad_request("chat_model must be a string"))?,
        )?;
    }
    if let Some(value) = submitted.get("generate_kwargs") {
        validate_json_object(value, "generate_kwargs")?;
    }
    if let Some(value) = submitted.get("custom_headers") {
        validate_headers(value)?;
    }
    if let Some(value) = submitted.get("auth_mode")
        && !matches!(value.as_str(), Some("api_key" | "auth_token"))
    {
        return Err(bad_request("auth_mode must be api_key or auth_token"));
    }
    Ok(())
}

fn validate_model_update(submitted: &Map<String, Value>) -> Result<(), ApiError> {
    const ALLOWED: [&str; 6] = [
        "max_input_length",
        "generate_kwargs",
        "relay_reasoning",
        "thinking_enabled",
        "thinking_budget",
        "reasoning_effort",
    ];
    reject_unknown_fields(submitted, &ALLOWED, "model")?;
    if let Some(value) = submitted.get("max_input_length")
        && value.as_u64().is_none_or(|value| value < 1_000)
    {
        return Err(bad_request("max_input_length must be at least 1000"));
    }
    if let Some(value) = submitted.get("generate_kwargs") {
        validate_json_object(value, "generate_kwargs")?;
        if let Some(max_tokens) = value.get("max_tokens")
            && max_tokens.as_u64().is_none_or(|value| value < 1)
        {
            return Err(bad_request(
                "generate_kwargs.max_tokens must be an integer >= 1",
            ));
        }
    }
    if let Some(value) = submitted.get("relay_reasoning")
        && !value.is_boolean()
    {
        return Err(bad_request("relay_reasoning must be a boolean"));
    }
    if let Some(value) = submitted.get("thinking_enabled") {
        optional_bool(value, "thinking_enabled")?;
    }
    if let Some(value) = submitted.get("thinking_budget")
        && optional_u64(value, "thinking_budget")? == Some(0)
    {
        return Err(bad_request("thinking_budget must be at least 1"));
    }
    if let Some(value) = submitted.get("reasoning_effort") {
        optional_string(value, "reasoning_effort")?;
    }
    Ok(())
}

fn validate_local_model_update(submitted: &Map<String, Value>) -> Result<(), ApiError> {
    const ALLOWED: [&str; 3] = ["max_context_length", "port", "generate_kwargs"];
    reject_unknown_fields(submitted, &ALLOWED, "local model")?;
    if let Some(value) = submitted.get("max_context_length")
        && value.as_u64().is_none_or(|value| value < 32_768)
    {
        return Err(bad_request("max_context_length must be at least 32768"));
    }
    if let Some(value) = submitted.get("port")
        && !value.is_null()
        && !value
            .as_u64()
            .is_some_and(|port| (1..=u64::from(u16::MAX)).contains(&port))
    {
        return Err(bad_request("port must be between 1 and 65535 or null"));
    }
    if let Some(value) = submitted.get("generate_kwargs") {
        validate_json_object(value, "generate_kwargs")?;
    }
    Ok(())
}

fn validate_registry(registry: &ProviderRegistry) -> Result<(), ApiError> {
    if registry.schema_version != REGISTRY_SCHEMA_VERSION
        || registry.providers.is_empty()
        || registry.providers.len() > MAX_PROVIDERS
        || !registry.providers.contains_key(DEFAULT_PROVIDER_ID)
        || !registry
            .providers
            .contains_key(&registry.active_provider_id)
    {
        return Err(internal("Model provider registry is invalid"));
    }
    if registry.local_model.max_context_length < 32_768 {
        return Err(internal("Local model config is invalid"));
    }
    validate_json_value(&Value::Object(registry.local_generate_kwargs.clone()), 0)
        .map_err(|_| internal("Local model config is invalid"))?;
    for (id, provider) in &registry.providers {
        if id != &provider.id {
            return Err(internal("Model provider registry is invalid"));
        }
        validate_provider_id(id).map_err(|_| internal("Model provider registry is invalid"))?;
        validate_provider(provider).map_err(|_| internal("Model provider registry is invalid"))?;
    }
    Ok(())
}

fn validate_provider(provider: &ProviderRecord) -> Result<(), ApiError> {
    validate_provider_name(&provider.name)?;
    validate_base_url(&provider.base_url)?;
    if provider.is_custom {
        validate_custom_chat_model(&provider.chat_model)?;
    } else {
        validate_chat_model(&provider.chat_model)?;
    }
    validate_headers(
        &serde_json::to_value(&provider.custom_headers)
            .map_err(|_| internal("Model provider headers could not be encoded"))?,
    )?;
    validate_json_value(&Value::Object(provider.generate_kwargs.clone()), 0)?;
    if model_count(provider) > MAX_MODELS_PER_PROVIDER {
        return Err(payload_too_large("Too many provider models"));
    }
    for model in all_models(provider) {
        validate_model(model)?;
    }
    ensure_unique_models(&provider.models)?;
    ensure_unique_models(&provider.extra_models)?;
    ensure_unique_models(&provider.discovered_models)?;
    ensure_unique_model_ids(provider.models.iter().chain(provider.extra_models.iter()))?;
    Ok(())
}

fn validate_model(model: &ModelRecord) -> Result<(), ApiError> {
    validate_model_id(&model.id)?;
    validate_short_text(&model.name, "Model name")?;
    if model.max_input_length < 1_000 || model.thinking_budget == Some(0) {
        return Err(bad_request("Model token limits are invalid"));
    }
    if let Some(value) = &model.reasoning_effort {
        validate_short_text(value, "reasoning_effort")?;
    }
    validate_json_value(&Value::Object(model.generate_kwargs.clone()), 0)
}

fn validate_provider_id(value: &str) -> Result<(), ApiError> {
    let mut characters = value.chars();
    let first = characters.next();
    if value.len() > 64
        || !first.is_some_and(|character| character.is_ascii_lowercase())
        || !characters.all(|character| {
            character.is_ascii_lowercase()
                || character.is_ascii_digit()
                || matches!(character, '-' | '_')
        })
    {
        return Err(bad_request(
            "Provider ID must match ^[a-z][a-z0-9_-]{0,63}$",
        ));
    }
    Ok(())
}

fn validate_provider_name(value: &str) -> Result<(), ApiError> {
    let value = value.trim();
    if value.is_empty()
        || value.len() > MAX_PROVIDER_NAME_BYTES
        || value.chars().any(char::is_control)
    {
        return Err(bad_request("Provider name is invalid"));
    }
    Ok(())
}

fn validate_model_id(value: &str) -> Result<(), ApiError> {
    let value = value.trim();
    if value.is_empty() || value.len() > MAX_MODEL_TEXT_BYTES || value.chars().any(char::is_control)
    {
        return Err(bad_request("Model ID is invalid"));
    }
    Ok(())
}

fn validate_short_text(value: &str, field: &str) -> Result<(), ApiError> {
    if value.trim().is_empty()
        || value.len() > MAX_MODEL_TEXT_BYTES
        || value.chars().any(char::is_control)
    {
        return Err(bad_request(&format!("{field} is invalid")));
    }
    Ok(())
}

fn validate_optional_short_text(value: &str, field: &str) -> Result<(), ApiError> {
    if value.is_empty() {
        Ok(())
    } else {
        validate_short_text(value, field)
    }
}

fn validate_chat_model(value: &str) -> Result<(), ApiError> {
    if matches!(
        value,
        "OpenAIChatModel"
            | "OpenAIResponseModel"
            | "AnthropicChatModel"
            | "DashScopeChatModel"
            | "GeminiChatModel"
    ) {
        Ok(())
    } else {
        Err(bad_request(&format!(
            "Unsupported custom protocol: {value}"
        )))
    }
}

fn validate_custom_chat_model(value: &str) -> Result<(), ApiError> {
    if matches!(
        value,
        "OpenAIChatModel" | "OpenAIResponseModel" | "AnthropicChatModel"
    ) {
        Ok(())
    } else {
        Err(bad_request(&format!(
            "Unsupported custom protocol: {value}"
        )))
    }
}

fn validate_base_url(value: &str) -> Result<(), ApiError> {
    let value = value.trim();
    if value.is_empty() {
        return Ok(());
    }
    if value.len() > MAX_URL_BYTES || value.chars().any(char::is_control) {
        return Err(bad_request("Provider base URL is invalid"));
    }
    let url = url::Url::parse(value).map_err(|_| bad_request("Provider base URL is invalid"))?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return Err(bad_request("Provider base URL must be an HTTP(S) URL"));
    }
    Ok(())
}

fn validate_api_key(value: &str) -> Result<(), ApiError> {
    if value.len() > MAX_API_KEY_BYTES || value.chars().any(char::is_control) {
        Err(bad_request(
            "API key must contain at most 8192 bytes without control characters",
        ))
    } else {
        Ok(())
    }
}

fn validate_headers(value: &Value) -> Result<(), ApiError> {
    let headers = value
        .as_object()
        .ok_or_else(|| bad_request("custom_headers must be an object"))?;
    if headers.len() > MAX_HEADERS {
        return Err(payload_too_large("Too many custom headers"));
    }
    for (name, value) in headers {
        let valid_header = name.parse::<reqwest::header::HeaderName>().is_ok()
            && value
                .as_str()
                .is_some_and(|value| value.parse::<reqwest::header::HeaderValue>().is_ok());
        if !valid_header
            || name.trim().is_empty()
            || name.len() > MAX_MODEL_TEXT_BYTES
            || name.chars().any(char::is_control)
            || !value.as_str().is_some_and(|value| {
                value.len() <= MAX_JSON_STRING_BYTES && !value.chars().any(char::is_control)
            })
        {
            return Err(bad_request("Custom provider header is invalid"));
        }
    }
    Ok(())
}

fn validate_json_object(value: &Value, field: &str) -> Result<(), ApiError> {
    if !value.is_object() {
        return Err(bad_request(&format!("{field} must be an object")));
    }
    validate_json_value(value, 0)
}

fn validate_json_value(value: &Value, depth: usize) -> Result<(), ApiError> {
    if depth > MAX_JSON_DEPTH {
        return Err(payload_too_large(
            "Model configuration is too deeply nested",
        ));
    }
    match value {
        Value::String(value) if value.len() > MAX_JSON_STRING_BYTES => {
            Err(payload_too_large("Model configuration string is too large"))
        }
        Value::Array(values) => {
            if values.len() > MAX_JSON_ITEMS {
                return Err(payload_too_large("Model configuration has too many items"));
            }
            for value in values {
                validate_json_value(value, depth + 1)?;
            }
            Ok(())
        }
        Value::Object(values) => {
            if values.len() > MAX_JSON_ITEMS {
                return Err(payload_too_large("Model configuration has too many items"));
            }
            for (key, value) in values {
                if key.len() > MAX_JSON_STRING_BYTES {
                    return Err(payload_too_large("Model configuration key is too large"));
                }
                validate_json_value(value, depth + 1)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

fn reject_unknown_fields(
    values: &Map<String, Value>,
    allowed: &[&str],
    kind: &str,
) -> Result<(), ApiError> {
    if values.keys().any(|key| !allowed.contains(&key.as_str())) {
        Err(bad_request(&format!("Unknown {kind} config field")))
    } else {
        Ok(())
    }
}

fn optional_bool(value: &Value, field: &str) -> Result<Option<bool>, ApiError> {
    if value.is_null() {
        Ok(None)
    } else {
        value
            .as_bool()
            .map(Some)
            .ok_or_else(|| bad_request(&format!("{field} must be a boolean or null")))
    }
}

fn optional_u64(value: &Value, field: &str) -> Result<Option<u64>, ApiError> {
    if value.is_null() {
        Ok(None)
    } else {
        value
            .as_u64()
            .map(Some)
            .ok_or_else(|| bad_request(&format!("{field} must be an integer or null")))
    }
}

fn optional_string(value: &Value, field: &str) -> Result<Option<String>, ApiError> {
    if value.is_null() {
        return Ok(None);
    }
    let value = value
        .as_str()
        .ok_or_else(|| bad_request(&format!("{field} must be a string or null")))?;
    validate_short_text(value, field)?;
    Ok(Some(value.to_owned()))
}

fn ensure_unique_models(models: &[ModelRecord]) -> Result<(), ApiError> {
    ensure_unique_model_ids(models.iter())
}

fn ensure_unique_model_ids<'a>(
    models: impl Iterator<Item = &'a ModelRecord>,
) -> Result<(), ApiError> {
    let mut ids = models.map(|model| model.id.as_str()).collect::<Vec<_>>();
    let count = ids.len();
    ids.sort_unstable();
    ids.dedup();
    if ids.len() == count {
        Ok(())
    } else {
        Err(bad_request("Provider model IDs must be unique"))
    }
}

fn all_models(provider: &ProviderRecord) -> impl Iterator<Item = &ModelRecord> {
    provider
        .models
        .iter()
        .chain(provider.extra_models.iter())
        .chain(provider.discovered_models.iter())
}

fn all_models_mut(provider: &mut ProviderRecord) -> impl Iterator<Item = &mut ModelRecord> {
    provider
        .models
        .iter_mut()
        .chain(provider.extra_models.iter_mut())
        .chain(provider.discovered_models.iter_mut())
}

fn model_count(provider: &ProviderRecord) -> usize {
    provider.models.len() + provider.extra_models.len() + provider.discovered_models.len()
}

fn provider_has_model(registry: &ProviderRegistry, provider_id: &str, model_id: &str) -> bool {
    registry
        .providers
        .get(provider_id)
        .is_some_and(|provider| all_models(provider).any(|model| model.id == model_id))
}

fn provider_mut<'a>(
    registry: &'a mut ProviderRegistry,
    provider_id: &str,
) -> Result<&'a mut ProviderRecord, ApiError> {
    registry
        .providers
        .get_mut(provider_id)
        .ok_or_else(|| not_found(&format!("Provider '{provider_id}' not found")))
}

fn object<'a>(value: &'a Value, detail: &str) -> Result<&'a Map<String, Value>, ApiError> {
    value.as_object().ok_or_else(|| bad_request(detail))
}

fn default_chat_model() -> String {
    String::from("OpenAIChatModel")
}

fn default_active_scope() -> String {
    String::from("effective")
}

fn bump_registry(registry: &mut ProviderRegistry) {
    registry.revision = registry.revision.saturating_add(1);
}

fn read_registry(server: &AppServer) -> Result<ProviderRegistry, ApiError> {
    read_registry_from(desktop_workspace(server)?)
}

fn read_registry_from(workspace: &DesktopWorkspace) -> Result<ProviderRegistry, ApiError> {
    let path = registry_path(workspace);
    let metadata = fs::metadata(&path)
        .map_err(|_| internal("Model provider registry could not be inspected"))?;
    if !metadata.is_file() || metadata.len() > REGISTRY_MAX_BYTES {
        return Err(internal("Model provider registry is invalid"));
    }
    let bytes =
        fs::read(path).map_err(|_| internal("Model provider registry could not be read"))?;
    let registry = serde_json::from_slice::<ProviderRegistry>(&bytes)
        .map_err(|_| internal("Model provider registry is invalid"))?;
    validate_registry(&registry)?;
    Ok(registry)
}

fn write_registry(server: &AppServer, registry: &ProviderRegistry) -> Result<(), ApiError> {
    write_registry_to(desktop_workspace(server)?, registry)
}

fn write_registry_to(
    workspace: &DesktopWorkspace,
    registry: &ProviderRegistry,
) -> Result<(), ApiError> {
    validate_registry(registry)?;
    let bytes = serde_json::to_vec_pretty(registry)
        .map_err(|_| internal("Model provider registry could not be encoded"))?;
    if bytes.len() as u64 > REGISTRY_MAX_BYTES {
        return Err(payload_too_large("Model provider registry is too large"));
    }
    let path = registry_path(workspace);
    let directory = path
        .parent()
        .ok_or_else(|| internal("Model provider registry path is invalid"))?;
    fs::create_dir_all(directory)
        .map_err(|_| internal("Model provider registry directory could not be created"))?;
    let mut temporary = tempfile::NamedTempFile::new_in(directory)
        .map_err(|_| internal("Model provider registry could not be staged"))?;
    temporary
        .write_all(&bytes)
        .and_then(|()| temporary.flush())
        .map_err(|_| internal("Model provider registry could not be staged"))?;
    temporary
        .persist(path)
        .map_err(|_| internal("Model provider registry could not be persisted"))?;
    Ok(())
}

fn registry_path(workspace: &DesktopWorkspace) -> PathBuf {
    workspace.data_dir.join("models").join("registry.json")
}

fn desktop_workspace(server: &AppServer) -> Result<&DesktopWorkspace, ApiError> {
    server
        .inner
        .desktop_workspace
        .as_ref()
        .ok_or_else(|| internal("Desktop Workspace is unavailable"))
}

async fn load_provider_secret(
    server: &AppServer,
    provider_id: &str,
) -> Result<Option<String>, ApiError> {
    let credentials = server
        .inner
        .desktop_credentials
        .clone()
        .ok_or_else(credential_store_error)?;
    let provider_id = provider_id.to_owned();
    tokio::task::spawn_blocking(move || load_secret_from_ref(credentials.as_ref(), &provider_id))
        .await
        .map_err(|_| credential_store_error())?
        .map_err(|_| credential_store_error())
}

async fn save_provider_secret(
    server: &AppServer,
    provider_id: &str,
    value: Option<&str>,
) -> Result<(), ApiError> {
    let credentials = server
        .inner
        .desktop_credentials
        .clone()
        .ok_or_else(credential_store_error)?;
    let provider_id = provider_id.to_owned();
    let value = value.map(str::to_owned);
    tokio::task::spawn_blocking(move || {
        save_secret_from_ref(credentials.as_ref(), &provider_id, value.as_deref())
    })
    .await
    .map_err(|_| credential_store_error())?
    .map_err(|_| credential_store_error())
}

fn load_secret_from_ref(
    credentials: &dyn DesktopCredentialStore,
    provider_id: &str,
) -> anyhow::Result<Option<String>> {
    if provider_id == DEFAULT_PROVIDER_ID {
        credentials.load_api_key()
    } else {
        credentials.load_agent_setting_secret(&format!("{MODEL_SECRET_PREFIX}{provider_id}"))
    }
}

fn save_secret_from_ref(
    credentials: &dyn DesktopCredentialStore,
    provider_id: &str,
    value: Option<&str>,
) -> anyhow::Result<()> {
    if provider_id == DEFAULT_PROVIDER_ID {
        credentials.save_api_key(value)
    } else {
        credentials.save_agent_setting_secret(&format!("{MODEL_SECRET_PREFIX}{provider_id}"), value)
    }
}

fn bad_request(detail: &str) -> ApiError {
    (StatusCode::BAD_REQUEST, Json(json!({"detail": detail})))
}

fn conflict(detail: &str) -> ApiError {
    (StatusCode::CONFLICT, Json(json!({"detail": detail})))
}

fn not_found(detail: &str) -> ApiError {
    (StatusCode::NOT_FOUND, Json(json!({"detail": detail})))
}

fn payload_too_large(detail: &str) -> ApiError {
    (
        StatusCode::PAYLOAD_TOO_LARGE,
        Json(json!({"detail": detail})),
    )
}

fn internal(detail: &str) -> ApiError {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({"detail": detail})),
    )
}

fn credential_store_error() -> ApiError {
    internal("System credential storage is unavailable")
}

fn api_error_message(error: &ApiError) -> anyhow::Error {
    anyhow::anyhow!(
        error
            .1
            .0
            .get("detail")
            .and_then(Value::as_str)
            .unwrap_or("Model provider registry is invalid")
            .to_owned()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_registry_contains_real_core_model_without_a_secret() {
        let registry = default_registry("model-a", "https://example.test/v1", true);
        assert_eq!(registry.providers.len(), 36);
        validate_registry(&registry).expect("embedded provider registry should be valid");
        let provider = &registry.providers[DEFAULT_PROVIDER_ID];
        assert_eq!(provider.id, DEFAULT_PROVIDER_ID);
        assert_eq!(provider.models[0].id, "model-a");
        let response = provider_response(provider).expect("provider should encode");
        assert_eq!(response["api_key"], "********");
        assert!(response.get("api_key_configured").is_none());
        let responses = provider_responses(&registry).expect("providers should encode");
        assert_eq!(responses[0]["id"], "qwenpaw-local");
        assert_eq!(responses[13]["id"], "openai");
        assert_eq!(responses[13]["models"].as_array().map(Vec::len), Some(11));
        assert_eq!(responses[35]["id"], DEFAULT_PROVIDER_ID);
    }

    #[test]
    fn validates_provider_and_model_boundaries() {
        assert!(validate_provider_id("custom-1").is_ok());
        assert!(validate_provider_id("../custom").is_err());
        assert!(validate_base_url("http://127.0.0.1:9000/v1").is_ok());
        assert!(validate_base_url("file:///tmp/model").is_err());
        assert!(validate_chat_model("AnthropicChatModel").is_ok());
        assert!(validate_chat_model("UnknownModel").is_err());
        assert_eq!(
            classify_model_failure(400, "unsupported endpoint"),
            ("incompatible_api", false)
        );
        assert_eq!(
            classify_model_failure(503, "temporarily unavailable"),
            ("transient_error", true)
        );
        assert_eq!(
            redact_model_secret("credential secret-value failed", Some("secret-value")),
            "credential [REDACTED] failed"
        );
        assert!(
            validate_model_update(
                json!({"generate_kwargs": {"max_tokens": 0}})
                    .as_object()
                    .expect("object")
            )
            .is_err()
        );
    }
}
