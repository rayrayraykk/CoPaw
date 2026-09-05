//! Persistent Agent runtime and voice settings for the unchanged Console.

use std::collections::BTreeMap;
use std::io::Write as _;
use std::path::Path;
use std::path::PathBuf;
use std::time::Duration;
use std::time::Instant;

use anyhow::Context as _;
use axum::Json;
use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::extract::Multipart;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::http::StatusCode;
use axum::routing::get;
use axum::routing::post;
use futures_util::StreamExt as _;
use qwenpaw_core::AgentRuntimeConfig;
use qwenpaw_core::Core;
use qwenpaw_core::ToolApprovalLevel;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
use serde_json::json;
use tracing::warn;

use super::AppServer;
use super::DesktopCredentialStore;

const SETTINGS_VERSION: u32 = 1;
const MAX_SETTINGS_BYTES: usize = 256 * 1024;
const MAX_VALUE_DEPTH: usize = 16;
const MAX_COLLECTION_ITEMS: usize = 1_024;
const MAX_STRING_BYTES: usize = 16 * 1024;
const MAX_AUDIO_UPLOAD_BYTES: usize = 32 * 1024 * 1024;
const MAX_SYSTEM_PROMPT_FILES_BODY_BYTES: usize = 64 * 1024;
const MAX_TRANSCRIPTION_RESPONSE_BYTES: usize = 1024 * 1024;
const SUPPORTED_AGENT_LANGUAGES: [&str; 4] = ["en", "id", "ru", "zh"];
const SUPPORTED_AUDIO_MODES: [&str; 2] = ["auto", "native"];
const SUPPORTED_PROVIDER_TYPES: [&str; 3] = ["disabled", "local_whisper", "whisper_api"];
const EMBEDDING_API_KEY: &str = "running-config.embedding-api-key";
const RERANKER_API_KEY: &str = "running-config.reranker-api-key";
const ADBPG_REST_API_KEY: &str = "running-config.adbpg-rest-api-key";
const AGENT_TEMPLATE_FILENAMES: [&str; 8] = [
    "AGENTS.md",
    "BOOTSTRAP.md",
    "CONTACTS.md",
    "HEARTBEAT.md",
    "MAIL_TRIAGE.md",
    "MEMORY.md",
    "PROFILE.md",
    "SOUL.md",
];

const SECRET_PATHS: [(&str, &str); 3] = [
    (
        EMBEDDING_API_KEY,
        "/reme_light_memory_config/embedding_model_config/api_key",
    ),
    (
        RERANKER_API_KEY,
        "/reme_light_memory_config/reranker_config/api_key",
    ),
    (ADBPG_REST_API_KEY, "/adbpg_memory_config/rest_api_key"),
];

type ApiError = (StatusCode, Json<Value>);

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DesktopAgentSettings {
    version: u32,
    running_config: Value,
    language: String,
    user_timezone: String,
    audio_mode: String,
    transcription_provider_type: String,
    transcription_provider_id: String,
}

impl Default for DesktopAgentSettings {
    fn default() -> Self {
        Self {
            version: SETTINGS_VERSION,
            running_config: default_running_config(),
            language: String::from("en"),
            user_timezone: String::from("UTC"),
            audio_mode: String::from("auto"),
            transcription_provider_type: String::from("disabled"),
            transcription_provider_id: String::new(),
        }
    }
}

#[derive(Debug, Deserialize)]
struct LanguageRequest {
    language: String,
}

#[derive(Debug, Deserialize)]
struct TimezoneRequest {
    timezone: String,
}

#[derive(Debug, Deserialize)]
struct AudioModeRequest {
    audio_mode: String,
}

#[derive(Debug, Deserialize)]
struct ProviderTypeRequest {
    transcription_provider_type: String,
}

#[derive(Debug, Deserialize)]
struct ProviderRequest {
    #[serde(default)]
    provider_id: String,
}

#[derive(Debug, Deserialize)]
struct EmbeddingTestRequest {
    backend: String,
    #[serde(default)]
    api_key: String,
    base_url: String,
    model_name: String,
    dimensions: usize,
    #[serde(default)]
    use_dimensions: bool,
    health_check_timeout: u64,
}

pub(super) fn router() -> Router<AppServer> {
    Router::new()
        .route(
            "/api/workspace/system-prompt-files",
            get(get_system_prompt_files)
                .put(put_system_prompt_files)
                .layer(DefaultBodyLimit::max(MAX_SYSTEM_PROMPT_FILES_BODY_BYTES)),
        )
        .route(
            "/api/workspace/running-config",
            get(get_running_config).put(put_running_config),
        )
        .route("/api/workspace/embedding/test", post(test_embedding))
        .route(
            "/api/workspace/language",
            get(get_language).put(put_language),
        )
        .route(
            "/api/config/user-timezone",
            get(get_timezone).put(put_timezone),
        )
        .route(
            "/api/workspace/audio-mode",
            get(get_audio_mode).put(put_audio_mode),
        )
        .route(
            "/api/workspace/transcription-provider-type",
            get(get_provider_type).put(put_provider_type),
        )
        .route(
            "/api/workspace/transcription-providers",
            get(get_transcription_providers),
        )
        .route(
            "/api/workspace/transcription-provider",
            axum::routing::put(put_transcription_provider),
        )
        .route(
            "/api/workspace/local-whisper-status",
            get(get_local_whisper_status),
        )
        .route("/api/workspace/transcribe", post(transcribe_audio))
        .layer(DefaultBodyLimit::max(MAX_AUDIO_UPLOAD_BYTES))
}

async fn get_system_prompt_files(
    State(server): State<AppServer>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    let agent_id = super::desktop_agents::requested_agent_id(&headers)?;
    let config = super::desktop_agents::config_for_agent(&server, &agent_id).await?;
    Ok(Json(
        config
            .get("system_prompt_files")
            .cloned()
            .unwrap_or_else(|| json!(["AGENTS.md", "SOUL.md", "PROFILE.md"])),
    ))
}

async fn put_system_prompt_files(
    State(server): State<AppServer>,
    headers: HeaderMap,
    Json(files): Json<Vec<String>>,
) -> Result<Json<Value>, ApiError> {
    let agent_id = super::desktop_agents::requested_agent_id(&headers)?;
    let _guard = server.inner.desktop_agent_settings_lock.lock().await;
    let previous = server
        .inner
        .core
        .system_prompt_files()
        .map_err(|error| internal_error(&error.to_string()))?;
    server
        .inner
        .core
        .replace_system_prompt_files(files.clone())
        .map_err(|error| bad_request(&error.to_string()))?;
    if let Err(error) = super::desktop_agents::replace_config_field(
        &server,
        &agent_id,
        "system_prompt_files",
        json!(files),
    )
    .await
    {
        let _ = server.inner.core.replace_system_prompt_files(previous);
        return Err(error);
    }
    Ok(Json(json!(files)))
}

pub(super) fn initialize(
    core: &Core,
    _credentials: &dyn DesktopCredentialStore,
    workspace_root: &Path,
) -> anyhow::Result<()> {
    let settings = load_settings(core).map_err(|(_, body)| {
        anyhow::anyhow!(body.0["detail"].as_str().unwrap_or("invalid").to_owned())
    })?;
    core.replace_agent_runtime_config(runtime_config(&settings.running_config).map_err(
        |(_, body)| anyhow::anyhow!(body.0["detail"].as_str().unwrap_or("invalid").to_owned()),
    )?)
    .map_err(anyhow::Error::msg)
    .context("failed to apply persisted Desktop Agent settings")?;
    copy_agent_templates_to(workspace_root, &settings.language, false)
        .map_err(|(_, body)| {
            anyhow::anyhow!(
                body.0["detail"]
                    .as_str()
                    .unwrap_or("Agent templates could not be initialized")
                    .to_owned()
            )
        })
        .context("failed to initialize Desktop Agent templates")?;
    Ok(())
}

pub(super) fn memory_directories(core: &Core) -> Result<(PathBuf, PathBuf), ApiError> {
    let settings = load_settings(core)?;
    memory_directory_paths(&settings.running_config)
}

pub(super) fn user_timezone(core: &Core) -> Result<String, ApiError> {
    Ok(load_settings(core)?.user_timezone)
}

async fn get_running_config(
    State(server): State<AppServer>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    let agent_id = super::desktop_agents::requested_agent_id(&headers)?;
    let _guard = server.inner.desktop_agent_settings_lock.lock().await;
    let agent_config = super::desktop_agents::config_for_agent(&server, &agent_id).await?;
    let mut config = agent_config
        .get("running")
        .cloned()
        .unwrap_or_else(default_running_config);
    hydrate_secrets(&server, &agent_id, &mut config)?;
    Ok(Json(config))
}

async fn put_running_config(
    State(server): State<AppServer>,
    headers: HeaderMap,
    Json(submitted): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let agent_id = super::desktop_agents::requested_agent_id(&headers)?;
    let _guard = server.inner.desktop_agent_settings_lock.lock().await;
    validate_value_bounds(&submitted, 0)?;
    let mut next = default_running_config();
    merge_json(&mut next, &submitted);
    validate_running_config(&next)?;
    let next_runtime = runtime_config(&next)?;
    let credentials = credentials(&server)?;
    let previous_secrets = load_secrets(credentials, &agent_id)?;
    let next_secrets = submitted_secrets(&submitted, &previous_secrets)?;
    scrub_secrets(&mut next);
    replace_secrets(credentials, &agent_id, &previous_secrets, &next_secrets)?;
    let previous_runtime = server
        .inner
        .core
        .agent_runtime_config()
        .map_err(|error| internal_error(&error.to_string()))?;
    if let Err(error) = server.inner.core.replace_agent_runtime_config(next_runtime) {
        let _ = replace_secrets(credentials, &agent_id, &next_secrets, &previous_secrets);
        return Err(bad_request(&error.to_string()));
    }
    if let Err(error) =
        super::desktop_agents::replace_config_field(&server, &agent_id, "running", next.clone())
            .await
    {
        let _ = server
            .inner
            .core
            .replace_agent_runtime_config(previous_runtime);
        let _ = replace_secrets(credentials, &agent_id, &next_secrets, &previous_secrets);
        return Err(error);
    }
    let mut response = next;
    hydrate_values(&mut response, &next_secrets);
    Ok(Json(response))
}

async fn test_embedding(
    State(server): State<AppServer>,
    headers: HeaderMap,
    Json(request): Json<EmbeddingTestRequest>,
) -> Result<Json<Value>, ApiError> {
    let agent_id = super::desktop_agents::requested_agent_id(&headers)?;
    super::desktop_agents::workspace_for_agent(&server, &agent_id).await?;
    if request.dimensions == 0 || request.dimensions > 1_000_000 {
        return Err(bad_request("Embedding dimensions are invalid"));
    }
    if request.model_name.trim().is_empty() || request.model_name.len() > 1_024 {
        return Err(bad_request("Embedding model name is invalid"));
    }
    let timeout = request.health_check_timeout.clamp(1, 120);
    let started = Instant::now();
    let tested = perform_embedding_test(&request, timeout).await;
    let latency_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    let (actual_dimensions, error) = match tested {
        Ok(dimensions) if dimensions == request.dimensions => (Some(dimensions), None),
        Ok(dimensions) => (
            Some(dimensions),
            Some(format!(
                "Embedding dimension mismatch: expected {}, got {dimensions}",
                request.dimensions
            )),
        ),
        Err(message) => {
            let message = if request.api_key.is_empty() {
                message
            } else {
                message.replace(&request.api_key, "***")
            };
            (None, Some(message))
        }
    };
    Ok(Json(json!({
        "success": error.is_none(),
        "configured_dimensions": request.dimensions,
        "actual_dimensions": actual_dimensions,
        "latency_ms": latency_ms,
        "message": error.unwrap_or_else(|| String::from("Embedding service is available"))
    })))
}

async fn perform_embedding_test(
    request: &EmbeddingTestRequest,
    timeout_seconds: u64,
) -> Result<usize, String> {
    if !matches!(request.backend.as_str(), "openai" | "dashscope") {
        return Err(format!(
            "Embedding backend '{}' is not available in Rust Core",
            request.backend
        ));
    }
    let base_url = url::Url::parse(request.base_url.trim())
        .map_err(|_| String::from("Embedding base URL is invalid"))?;
    if !matches!(base_url.scheme(), "http" | "https")
        || !base_url.username().is_empty()
        || base_url.password().is_some()
        || base_url.host_str().is_none()
    {
        return Err(String::from("Embedding base URL is invalid"));
    }
    let endpoint = format!(
        "{}/embeddings",
        request.base_url.trim().trim_end_matches('/')
    );
    let mut body = json!({
        "model": request.model_name,
        "input": ["QwenPaw embedding health check"]
    });
    if request.use_dimensions {
        body["dimensions"] = json!(request.dimensions);
    }
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(timeout_seconds))
        .build()
        .map_err(|error| error.to_string())?;
    let mut builder = client.post(endpoint).json(&body);
    if !request.api_key.trim().is_empty() {
        builder = builder.bearer_auth(request.api_key.trim());
    }
    let response = builder.send().await.map_err(|error| error.to_string())?;
    let status = response.status();
    let mut bytes = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| error.to_string())?;
        if bytes.len().saturating_add(chunk.len()) > MAX_TRANSCRIPTION_RESPONSE_BYTES {
            return Err(String::from("Embedding response is too large"));
        }
        bytes.extend_from_slice(&chunk);
    }
    if !status.is_success() {
        return Err(format!("Embedding service returned HTTP {status}"));
    }
    let response = serde_json::from_slice::<Value>(&bytes)
        .map_err(|_| String::from("Embedding service returned invalid JSON"))?;
    let embedding = response
        .pointer("/data/0/embedding")
        .and_then(Value::as_array)
        .ok_or_else(|| String::from("Embedding service returned an empty vector"))?;
    if embedding.is_empty()
        || embedding
            .iter()
            .any(|value| value.as_f64().is_none_or(|number| !number.is_finite()))
    {
        return Err(String::from("Embedding service returned invalid numbers"));
    }
    Ok(embedding.len())
}

async fn get_language(
    State(server): State<AppServer>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    let agent_id = super::desktop_agents::requested_agent_id(&headers)?;
    let _guard = server.inner.desktop_agent_settings_lock.lock().await;
    let config = super::desktop_agents::config_for_agent(&server, &agent_id).await?;
    let language = config
        .get("language")
        .and_then(Value::as_str)
        .unwrap_or("en");
    Ok(Json(json!({
        "language": language,
        "agent_id": agent_id
    })))
}

async fn put_language(
    State(server): State<AppServer>,
    headers: HeaderMap,
    Json(request): Json<LanguageRequest>,
) -> Result<Json<Value>, ApiError> {
    let agent_id = super::desktop_agents::requested_agent_id(&headers)?;
    let language = request.language.trim().to_ascii_lowercase();
    if !SUPPORTED_AGENT_LANGUAGES.contains(&language.as_str()) {
        return Err(bad_request(&format!(
            "Invalid language '{language}'. Must be one of: {}",
            SUPPORTED_AGENT_LANGUAGES.join(", ")
        )));
    }
    let _guard = server.inner.desktop_agent_settings_lock.lock().await;
    let config = super::desktop_agents::config_for_agent(&server, &agent_id).await?;
    let current = config
        .get("language")
        .and_then(Value::as_str)
        .unwrap_or("en");
    let copied_files = if current == language {
        Vec::new()
    } else {
        let workspace = super::desktop_agents::workspace_for_agent(&server, &agent_id).await?;
        copy_agent_templates_to(&workspace, &language, true)?
    };
    super::desktop_agents::replace_config_field(
        &server,
        &agent_id,
        "language",
        Value::String(language.clone()),
    )
    .await?;
    Ok(Json(json!({
        "language": language,
        "copied_files": copied_files,
        "agent_id": agent_id
    })))
}

pub(super) fn copy_agent_templates_to(
    workspace_root: &Path,
    language: &str,
    overwrite: bool,
) -> Result<Vec<&'static str>, ApiError> {
    let canonical_root = workspace_root
        .canonicalize()
        .map_err(|_| internal_error("Desktop workspace could not be resolved"))?;
    if !canonical_root.is_dir() {
        return Err(internal_error("Desktop workspace is not a directory"));
    }
    let templates = agent_templates(language)
        .ok_or_else(|| bad_request("Agent language templates are unavailable"))?;
    for filename in AGENT_TEMPLATE_FILENAMES {
        let target = canonical_root.join(filename);
        if let Ok(metadata) = target.symlink_metadata()
            && (!metadata.is_file() || metadata.file_type().is_symlink())
        {
            return Err(bad_request(&format!(
                "Invalid Agent template target: {filename}"
            )));
        }
    }
    let mut copied = Vec::new();
    for (filename, contents) in AGENT_TEMPLATE_FILENAMES.iter().zip(templates) {
        let target = canonical_root.join(filename);
        if !overwrite && target.is_file() {
            continue;
        }
        let mut temporary = tempfile::NamedTempFile::new_in(&canonical_root)
            .map_err(|_| internal_error("Agent template could not be staged"))?;
        temporary
            .write_all(contents.as_bytes())
            .map_err(|_| internal_error("Agent template could not be staged"))?;
        temporary
            .persist(&target)
            .map_err(|_| internal_error("Agent template could not be installed"))?;
        copied.push(*filename);
    }
    Ok(copied)
}

fn agent_templates(language: &str) -> Option<[&'static str; 8]> {
    match language {
        "en" => Some([
            include_str!("../../../../src/qwenpaw/agents/md_files/en/AGENTS.md"),
            include_str!("../../../../src/qwenpaw/agents/md_files/en/BOOTSTRAP.md"),
            include_str!("../../../../src/qwenpaw/agents/md_files/en/CONTACTS.md"),
            include_str!("../../../../src/qwenpaw/agents/md_files/en/HEARTBEAT.md"),
            include_str!("../../../../src/qwenpaw/agents/md_files/en/MAIL_TRIAGE.md"),
            include_str!("../../../../src/qwenpaw/agents/md_files/en/MEMORY.md"),
            include_str!("../../../../src/qwenpaw/agents/md_files/en/PROFILE.md"),
            include_str!("../../../../src/qwenpaw/agents/md_files/en/SOUL.md"),
        ]),
        "id" => Some([
            include_str!("../../../../src/qwenpaw/agents/md_files/id/AGENTS.md"),
            include_str!("../../../../src/qwenpaw/agents/md_files/id/BOOTSTRAP.md"),
            include_str!("../../../../src/qwenpaw/agents/md_files/id/CONTACTS.md"),
            include_str!("../../../../src/qwenpaw/agents/md_files/id/HEARTBEAT.md"),
            include_str!("../../../../src/qwenpaw/agents/md_files/id/MAIL_TRIAGE.md"),
            include_str!("../../../../src/qwenpaw/agents/md_files/id/MEMORY.md"),
            include_str!("../../../../src/qwenpaw/agents/md_files/id/PROFILE.md"),
            include_str!("../../../../src/qwenpaw/agents/md_files/id/SOUL.md"),
        ]),
        "ru" => Some([
            include_str!("../../../../src/qwenpaw/agents/md_files/ru/AGENTS.md"),
            include_str!("../../../../src/qwenpaw/agents/md_files/ru/BOOTSTRAP.md"),
            include_str!("../../../../src/qwenpaw/agents/md_files/ru/CONTACTS.md"),
            include_str!("../../../../src/qwenpaw/agents/md_files/ru/HEARTBEAT.md"),
            include_str!("../../../../src/qwenpaw/agents/md_files/ru/MAIL_TRIAGE.md"),
            include_str!("../../../../src/qwenpaw/agents/md_files/ru/MEMORY.md"),
            include_str!("../../../../src/qwenpaw/agents/md_files/ru/PROFILE.md"),
            include_str!("../../../../src/qwenpaw/agents/md_files/ru/SOUL.md"),
        ]),
        "zh" => Some([
            include_str!("../../../../src/qwenpaw/agents/md_files/zh/AGENTS.md"),
            include_str!("../../../../src/qwenpaw/agents/md_files/zh/BOOTSTRAP.md"),
            include_str!("../../../../src/qwenpaw/agents/md_files/zh/CONTACTS.md"),
            include_str!("../../../../src/qwenpaw/agents/md_files/zh/HEARTBEAT.md"),
            include_str!("../../../../src/qwenpaw/agents/md_files/zh/MAIL_TRIAGE.md"),
            include_str!("../../../../src/qwenpaw/agents/md_files/zh/MEMORY.md"),
            include_str!("../../../../src/qwenpaw/agents/md_files/zh/PROFILE.md"),
            include_str!("../../../../src/qwenpaw/agents/md_files/zh/SOUL.md"),
        ]),
        _ => None,
    }
}

async fn get_timezone(State(server): State<AppServer>) -> Result<Json<Value>, ApiError> {
    let _guard = server.inner.desktop_agent_settings_lock.lock().await;
    let settings = load_settings(&server.inner.core)?;
    Ok(Json(json!({"timezone": settings.user_timezone})))
}

async fn put_timezone(
    State(server): State<AppServer>,
    Json(request): Json<TimezoneRequest>,
) -> Result<Json<Value>, ApiError> {
    let timezone = normalize_timezone(&request.timezone)?;
    let _guard = server.inner.desktop_agent_settings_lock.lock().await;
    let mut settings = load_settings(&server.inner.core)?;
    settings.user_timezone.clone_from(&timezone);
    persist_settings(&server.inner.core, &settings)?;
    Ok(Json(json!({"timezone": timezone})))
}

async fn get_audio_mode(
    State(server): State<AppServer>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    let agent_id = super::desktop_agents::requested_agent_id(&headers)?;
    let _guard = server.inner.desktop_agent_settings_lock.lock().await;
    let config = super::desktop_agents::config_for_agent(&server, &agent_id).await?;
    let audio_mode = config
        .get("audio_mode")
        .and_then(Value::as_str)
        .unwrap_or("auto");
    Ok(Json(json!({"audio_mode": audio_mode})))
}

async fn put_audio_mode(
    State(server): State<AppServer>,
    headers: HeaderMap,
    Json(request): Json<AudioModeRequest>,
) -> Result<Json<Value>, ApiError> {
    let agent_id = super::desktop_agents::requested_agent_id(&headers)?;
    let audio_mode = request.audio_mode.trim().to_ascii_lowercase();
    if !SUPPORTED_AUDIO_MODES.contains(&audio_mode.as_str()) {
        return Err(bad_request(&format!(
            "Invalid audio_mode '{audio_mode}'. Must be one of: {}",
            SUPPORTED_AUDIO_MODES.join(", ")
        )));
    }
    let _guard = server.inner.desktop_agent_settings_lock.lock().await;
    super::desktop_agents::replace_config_field(
        &server,
        &agent_id,
        "audio_mode",
        Value::String(audio_mode.clone()),
    )
    .await?;
    Ok(Json(json!({"audio_mode": audio_mode})))
}

async fn get_provider_type(
    State(server): State<AppServer>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    let agent_id = super::desktop_agents::requested_agent_id(&headers)?;
    let _guard = server.inner.desktop_agent_settings_lock.lock().await;
    let config = super::desktop_agents::config_for_agent(&server, &agent_id).await?;
    let provider_type = config
        .get("transcription_provider_type")
        .and_then(Value::as_str)
        .unwrap_or("disabled");
    Ok(Json(json!({
        "transcription_provider_type": provider_type
    })))
}

async fn put_provider_type(
    State(server): State<AppServer>,
    headers: HeaderMap,
    Json(request): Json<ProviderTypeRequest>,
) -> Result<Json<Value>, ApiError> {
    let agent_id = super::desktop_agents::requested_agent_id(&headers)?;
    let provider_type = request
        .transcription_provider_type
        .trim()
        .to_ascii_lowercase();
    if !SUPPORTED_PROVIDER_TYPES.contains(&provider_type.as_str()) {
        return Err(bad_request(&format!(
            "Invalid transcription_provider_type '{provider_type}'. Must be one of: {}",
            SUPPORTED_PROVIDER_TYPES.join(", ")
        )));
    }
    let _guard = server.inner.desktop_agent_settings_lock.lock().await;
    super::desktop_agents::replace_config_field(
        &server,
        &agent_id,
        "transcription_provider_type",
        Value::String(provider_type.clone()),
    )
    .await?;
    Ok(Json(json!({
        "transcription_provider_type": provider_type
    })))
}

async fn get_transcription_providers(
    State(server): State<AppServer>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    let agent_id = super::desktop_agents::requested_agent_id(&headers)?;
    let _guard = server.inner.desktop_agent_settings_lock.lock().await;
    let config = super::desktop_agents::config_for_agent(&server, &agent_id).await?;
    let provider_id = config
        .get("transcription_provider_id")
        .and_then(Value::as_str)
        .unwrap_or("");
    let model = server.inner.core.read_config().config;
    Ok(Json(json!({
        "providers": [{
            "id": "openai-compatible",
            "name": "OpenAI Compatible",
            "available": model.api_key_configured
        }],
        "configured_provider_id": provider_id
    })))
}

async fn put_transcription_provider(
    State(server): State<AppServer>,
    headers: HeaderMap,
    Json(request): Json<ProviderRequest>,
) -> Result<Json<Value>, ApiError> {
    let agent_id = super::desktop_agents::requested_agent_id(&headers)?;
    let provider_id = request.provider_id.trim();
    if provider_id.len() > 256 || provider_id.chars().any(char::is_control) {
        return Err(bad_request("Invalid transcription provider ID"));
    }
    let _guard = server.inner.desktop_agent_settings_lock.lock().await;
    super::desktop_agents::replace_config_field(
        &server,
        &agent_id,
        "transcription_provider_id",
        Value::String(provider_id.to_owned()),
    )
    .await?;
    Ok(Json(json!({"provider_id": provider_id})))
}

async fn get_local_whisper_status() -> Json<Value> {
    let ffmpeg_installed = executable_on_path("ffmpeg");
    let whisper_installed = executable_on_path("whisper");
    Json(json!({
        "available": ffmpeg_installed && whisper_installed,
        "ffmpeg_installed": ffmpeg_installed,
        "whisper_installed": whisper_installed
    }))
}

async fn transcribe_audio(
    State(server): State<AppServer>,
    headers: HeaderMap,
    multipart: Multipart,
) -> Result<Json<Value>, ApiError> {
    let agent_id = super::desktop_agents::requested_agent_id(&headers)?;
    let (provider_type, provider_id) = {
        let _guard = server.inner.desktop_agent_settings_lock.lock().await;
        let config = super::desktop_agents::config_for_agent(&server, &agent_id).await?;
        (
            config
                .get("transcription_provider_type")
                .and_then(Value::as_str)
                .unwrap_or("disabled")
                .to_owned(),
            config
                .get("transcription_provider_id")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned(),
        )
    };
    if provider_type == "disabled" {
        return Err(transcription_error(
            StatusCode::BAD_REQUEST,
            "TRANSCRIPTION_DISABLED",
            "Transcription is disabled. Configure a transcription provider in Settings.",
        ));
    }
    let (filename, content_type, data) = read_audio_upload(multipart).await?;
    if provider_type == "local_whisper" {
        return Err(transcription_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "TRANSCRIPTION_PROVIDER_UNAVAILABLE",
            "The Rust Core local Whisper runtime is not installed.",
        ));
    }
    if provider_id != "openai-compatible" {
        return Err(transcription_error(
            StatusCode::BAD_REQUEST,
            "TRANSCRIPTION_PROVIDER_UNAVAILABLE",
            "Select an available Whisper API provider in Settings.",
        ));
    }
    call_whisper_api(&server, filename, content_type, data).await
}

async fn call_whisper_api(
    server: &AppServer,
    filename: String,
    content_type: String,
    data: Vec<u8>,
) -> Result<Json<Value>, ApiError> {
    let credential_store = server
        .inner
        .desktop_credentials
        .clone()
        .ok_or_else(|| internal_error("Desktop credential storage is unavailable"))?;
    let api_key = tokio::task::spawn_blocking(move || credential_store.load_api_key())
        .await
        .map_err(|_| internal_error("Transcription credential task failed"))?
        .map_err(|error| {
            warn!(%error, "failed to read transcription provider credential");
            internal_error("Transcription provider credential could not be loaded")
        })?
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            transcription_error(
                StatusCode::BAD_REQUEST,
                "TRANSCRIPTION_PROVIDER_UNAVAILABLE",
                "The selected Whisper API provider has no API key.",
            )
        })?;
    let base_url = server.inner.core.read_config().config.base_url;
    let endpoint = format!("{}/audio/transcriptions", base_url.trim_end_matches('/'));
    let part = reqwest::multipart::Part::bytes(data)
        .file_name(filename)
        .mime_str(&content_type)
        .map_err(|_| bad_request("Uploaded audio content type is invalid"))?;
    let form = reqwest::multipart::Form::new()
        .part("file", part)
        .text("model", "whisper-1");
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(|_| internal_error("Transcription HTTP client could not be created"))?;
    let response = client
        .post(endpoint)
        .bearer_auth(&api_key)
        .multipart(form)
        .send()
        .await
        .map_err(|_| {
            transcription_error(
                StatusCode::BAD_GATEWAY,
                "TRANSCRIPTION_PROVIDER_ERROR",
                "The Whisper API request failed.",
            )
        })?;
    let status = response.status();
    let payload = read_bounded_response(response).await?;
    if !status.is_success() {
        return Err(transcription_error(
            StatusCode::BAD_GATEWAY,
            "TRANSCRIPTION_PROVIDER_ERROR",
            &format!("The Whisper API returned HTTP {}.", status.as_u16()),
        ));
    }
    let payload = serde_json::from_slice::<Value>(&payload).map_err(|_| {
        transcription_error(
            StatusCode::BAD_GATEWAY,
            "TRANSCRIPTION_PROVIDER_ERROR",
            "The Whisper API returned invalid JSON.",
        )
    })?;
    let text = payload["text"].as_str().ok_or_else(|| {
        transcription_error(
            StatusCode::BAD_GATEWAY,
            "TRANSCRIPTION_PROVIDER_ERROR",
            "The Whisper API response did not contain text.",
        )
    })?;
    Ok(Json(json!({"text": text})))
}

async fn read_audio_upload(
    mut multipart: Multipart,
) -> Result<(String, String, Vec<u8>), ApiError> {
    let mut upload = None;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|_| bad_request("Audio upload is invalid"))?
    {
        if field.name() != Some("file") {
            continue;
        }
        if upload.is_some() {
            return Err(bad_request("Audio upload requires exactly one file"));
        }
        let filename = field.file_name().unwrap_or("audio.webm").to_owned();
        let suffix = Path::new(&filename)
            .extension()
            .and_then(|value| value.to_str())
            .map_or_else(
                || String::from(".webm"),
                |value| format!(".{}", value.to_ascii_lowercase()),
            );
        if !matches!(
            suffix.as_str(),
            ".webm" | ".mp4" | ".m4a" | ".wav" | ".mp3" | ".ogg" | ".flac"
        ) {
            return Err(transcription_error(
                StatusCode::BAD_REQUEST,
                "UNSUPPORTED_FILE_TYPE",
                &format!("Unsupported file type: {suffix}."),
            ));
        }
        let content_type = field.content_type().map_or_else(
            || {
                mime_guess::from_path(&filename)
                    .first_or_octet_stream()
                    .to_string()
            },
            str::to_owned,
        );
        let bytes = field
            .bytes()
            .await
            .map_err(|_| bad_request("Audio upload could not be read"))?;
        if bytes.len() > MAX_AUDIO_UPLOAD_BYTES {
            return Err(transcription_error(
                StatusCode::PAYLOAD_TOO_LARGE,
                "FILE_TOO_LARGE",
                "Audio file exceeds the 32 MiB upload limit.",
            ));
        }
        upload = Some((filename, content_type, bytes.to_vec()));
    }
    upload.ok_or_else(|| bad_request("Audio upload requires a file"))
}

async fn read_bounded_response(response: reqwest::Response) -> Result<Vec<u8>, ApiError> {
    let mut stream = response.bytes_stream();
    let mut payload = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| {
            transcription_error(
                StatusCode::BAD_GATEWAY,
                "TRANSCRIPTION_PROVIDER_ERROR",
                "The Whisper API response could not be read.",
            )
        })?;
        if payload.len().saturating_add(chunk.len()) > MAX_TRANSCRIPTION_RESPONSE_BYTES {
            return Err(transcription_error(
                StatusCode::BAD_GATEWAY,
                "TRANSCRIPTION_PROVIDER_ERROR",
                "The Whisper API response exceeded the 1 MiB limit.",
            ));
        }
        payload.extend_from_slice(&chunk);
    }
    Ok(payload)
}

fn load_settings(core: &Core) -> Result<DesktopAgentSettings, ApiError> {
    let Some(serialized) = core
        .read_agent_settings_data()
        .map_err(|error| internal_error(&error.to_string()))?
    else {
        return Ok(DesktopAgentSettings::default());
    };
    if serialized.len() > MAX_SETTINGS_BYTES {
        return Err(internal_error(
            "Stored Agent settings exceed the size limit",
        ));
    }
    let settings = serde_json::from_str::<DesktopAgentSettings>(&serialized)
        .map_err(|_| internal_error("Stored Agent settings are invalid"))?;
    validate_settings(&settings)
        .map_err(|_| internal_error("Stored Agent settings are invalid"))?;
    Ok(settings)
}

fn persist_settings(core: &Core, settings: &DesktopAgentSettings) -> Result<(), ApiError> {
    validate_settings(settings)?;
    let serialized = serde_json::to_string(settings)
        .map_err(|_| internal_error("Agent settings could not be serialized"))?;
    if serialized.len() > MAX_SETTINGS_BYTES {
        return Err(payload_too_large("Agent settings exceed the 256 KiB limit"));
    }
    core.write_agent_settings_data(&serialized)
        .map_err(|error| internal_error(&error.to_string()))
}

fn validate_settings(settings: &DesktopAgentSettings) -> Result<(), ApiError> {
    if settings.version != SETTINGS_VERSION {
        return Err(bad_request("Unsupported Agent settings version"));
    }
    validate_running_config(&settings.running_config)?;
    if !SUPPORTED_AGENT_LANGUAGES.contains(&settings.language.as_str())
        || !SUPPORTED_AUDIO_MODES.contains(&settings.audio_mode.as_str())
        || !SUPPORTED_PROVIDER_TYPES.contains(&settings.transcription_provider_type.as_str())
        || settings.transcription_provider_id.len() > 256
        || settings
            .transcription_provider_id
            .chars()
            .any(char::is_control)
    {
        return Err(bad_request("Stored Agent setting is invalid"));
    }
    normalize_timezone(&settings.user_timezone)?;
    Ok(())
}

fn validate_running_config(config: &Value) -> Result<(), ApiError> {
    validate_value_bounds(config, 0)?;
    validate_shape(config, &default_running_config(), "running_config")?;
    integer_range(config, "/max_iters", 1, 500)?;
    integer_range(config, "/llm_max_retries", 1, i64::MAX)?;
    integer_range(config, "/llm_max_concurrent", 1, i64::MAX)?;
    integer_range(config, "/llm_max_qpm", 0, i64::MAX)?;
    integer_range(config, "/max_input_length", 1_000, i64::MAX)?;
    integer_range(config, "/history_max_length", 1_000, i64::MAX)?;
    float_range(config, "/llm_backoff_base", 0.1, f64::MAX)?;
    float_range(config, "/llm_backoff_cap", 0.5, f64::MAX)?;
    float_range(config, "/llm_rate_limit_pause", 1.0, f64::MAX)?;
    float_range(config, "/llm_rate_limit_jitter", 0.0, f64::MAX)?;
    float_range(config, "/llm_acquire_timeout", 10.0, f64::MAX)?;
    float_range(config, "/shell_command_timeout", 1.0, 600.0)?;
    let base = number(config, "/llm_backoff_base")?;
    let cap = number(config, "/llm_backoff_cap")?;
    if cap < base {
        return Err(bad_request(
            "llm_backoff_cap must be greater than or equal to llm_backoff_base",
        ));
    }
    let executable = string(config, "/shell_command_executable")?;
    if executable.len() > 4_096 || executable.chars().any(char::is_control) {
        return Err(bad_request("shell_command_executable is invalid"));
    }
    let approval = string(config, "/approval_level")?.to_ascii_uppercase();
    if !matches!(approval.as_str(), "STRICT" | "SMART" | "AUTO" | "OFF") {
        return Err(bad_request("approval_level is invalid"));
    }
    if let Some(max_iterations) = config.pointer("/loop/iteration/max_iterations")
        && !max_iterations.is_null()
    {
        integer_range(config, "/loop/iteration/max_iterations", 1, 500)?;
    }
    integer_range(config, "/loop/doom_loop/window_size", 2, i64::MAX)?;
    float_range(config, "/loop/doom_loop/similarity_threshold", 0.0, 1.0)?;
    integer_range(config, "/loop/rubric/max_interventions", 1, 10)?;
    integer_range(config, "/loop/goal/max_iterations", 1, 500)?;
    integer_range(config, "/loop/goal/max_tokens", 1, i64::MAX)?;
    integer_range(config, "/loop/mission/max_iterations", 1, 100)?;
    integer_range(config, "/loop/mission/max_retries_per_story", 0, 10)?;
    validate_embedding(config)?;
    memory_directory_paths(config)?;
    Ok(())
}

fn memory_directory_paths(config: &Value) -> Result<(PathBuf, PathBuf), ApiError> {
    let daily = portable_memory_directory(string(config, "/reme_light_memory_config/daily_dir")?)?;
    let digest =
        portable_memory_directory(string(config, "/reme_light_memory_config/digest_dir")?)?;
    if daily == digest {
        return Err(bad_request(
            "daily_dir and digest_dir must identify different directories",
        ));
    }
    Ok((daily, digest))
}

fn portable_memory_directory(value: &str) -> Result<PathBuf, ApiError> {
    let normalized = value.replace('\\', "/");
    let invalid = normalized.is_empty()
        || normalized.len() > 4_096
        || normalized.starts_with('/')
        || normalized.ends_with('/')
        || normalized.chars().any(char::is_control)
        || normalized
            .split('/')
            .any(|part| part.is_empty() || matches!(part, "." | "..") || part.contains(':'));
    if invalid {
        return Err(bad_request("Memory directory must be a safe relative path"));
    }
    Ok(normalized.split('/').collect())
}

fn validate_embedding(config: &Value) -> Result<(), ApiError> {
    let base = "/reme_light_memory_config/embedding_model_config";
    let backend = string(config, &format!("{base}/backend"))?;
    if !matches!(
        backend,
        "openai" | "dashscope" | "dashscope_multimodal" | "gemini" | "ollama"
    ) {
        return Err(bad_request("Embedding backend is invalid"));
    }
    for name in [
        "dimensions",
        "max_cache_size",
        "max_input_length",
        "max_batch_size",
    ] {
        integer_range(config, &format!("{base}/{name}"), 1, i64::MAX)?;
    }
    float_range(
        config,
        &format!("{base}/health_check_timeout"),
        f64::EPSILON,
        300.0,
    )?;
    Ok(())
}

pub(super) fn runtime_config(config: &Value) -> Result<AgentRuntimeConfig, ApiError> {
    validate_running_config(config)?;
    let iteration_enabled = boolean(config, "/loop/iteration/enabled")?;
    let max_agent_steps = if iteration_enabled {
        usize::try_from(
            config
                .pointer("/loop/iteration/max_iterations")
                .and_then(Value::as_u64)
                .or_else(|| config.pointer("/max_iters").and_then(Value::as_u64))
                .unwrap_or(100),
        )
        .map_err(|_| bad_request("Agent max steps are invalid"))?
    } else {
        500
    };
    let timeout = Duration::from_secs_f64(number(config, "/shell_command_timeout")?);
    let timeout_ms = u64::try_from(timeout.as_millis())
        .map_err(|_| bad_request("Agent shell timeout is invalid"))?;
    let approval_level = match string(config, "/approval_level")?
        .to_ascii_uppercase()
        .as_str()
    {
        "STRICT" => ToolApprovalLevel::Strict,
        "SMART" => ToolApprovalLevel::Smart,
        "OFF" => ToolApprovalLevel::Off,
        _ => ToolApprovalLevel::Auto,
    };
    Ok(AgentRuntimeConfig {
        max_agent_steps,
        shell_timeout_ms: timeout_ms,
        shell_executable: string(config, "/shell_command_executable")?.to_owned(),
        approval_level,
    })
}

fn credentials(server: &AppServer) -> Result<&dyn DesktopCredentialStore, ApiError> {
    server
        .inner
        .desktop_credentials
        .as_deref()
        .ok_or_else(|| internal_error("Desktop Agent credential storage is unavailable"))
}

fn load_secrets(
    credentials: &dyn DesktopCredentialStore,
    agent_id: &str,
) -> Result<BTreeMap<String, Option<String>>, ApiError> {
    SECRET_PATHS
        .iter()
        .map(|(key, _)| {
            let scoped_key = scoped_secret_key(agent_id, key);
            credentials
                .load_agent_setting_secret(&scoped_key)
                .map(|value| ((*key).to_owned(), value))
                .map_err(|error| {
                    warn!(%error, "failed to load a Desktop Agent credential");
                    internal_error("Desktop Agent credentials could not be loaded")
                })
        })
        .collect()
}

fn submitted_secrets(
    submitted: &Value,
    previous: &BTreeMap<String, Option<String>>,
) -> Result<BTreeMap<String, Option<String>>, ApiError> {
    let mut next = previous.clone();
    for (key, path) in SECRET_PATHS {
        let explicit = submitted.pointer(path).map(|value| {
            value
                .as_str()
                .ok_or_else(|| bad_request("Agent setting secret must be a string"))
                .and_then(validate_secret)
        });
        if let Some(value) = explicit {
            next.insert(key.to_owned(), value?);
        } else if key == ADBPG_REST_API_KEY
            && submitted
                .pointer("/adbpg_memory_config")
                .is_some_and(Value::is_null)
        {
            next.insert(key.to_owned(), None);
        }
    }
    Ok(next)
}

fn validate_secret(value: &str) -> Result<Option<String>, ApiError> {
    let value = value.trim();
    if value.is_empty() {
        return Ok(None);
    }
    if value.len() > MAX_STRING_BYTES || value.chars().any(char::is_control) {
        return Err(bad_request("Agent setting secret is invalid"));
    }
    Ok(Some(value.to_owned()))
}

fn replace_secrets(
    credentials: &dyn DesktopCredentialStore,
    agent_id: &str,
    previous: &BTreeMap<String, Option<String>>,
    next: &BTreeMap<String, Option<String>>,
) -> Result<(), ApiError> {
    let mut changed = Vec::new();
    for (key, _) in SECRET_PATHS {
        if previous.get(key) == next.get(key) {
            continue;
        }
        let value = next.get(key).and_then(Option::as_deref);
        let scoped_key = scoped_secret_key(agent_id, key);
        if let Err(error) = credentials.save_agent_setting_secret(&scoped_key, value) {
            for changed_key in changed.into_iter().rev() {
                let scoped_changed_key = scoped_secret_key(agent_id, changed_key);
                let _ = credentials.save_agent_setting_secret(
                    &scoped_changed_key,
                    previous.get(changed_key).and_then(Option::as_deref),
                );
            }
            warn!(%error, "failed to replace Desktop Agent credentials");
            return Err(internal_error(
                "Desktop Agent credentials could not be saved",
            ));
        }
        changed.push(key);
    }
    Ok(())
}

fn hydrate_secrets(server: &AppServer, agent_id: &str, config: &mut Value) -> Result<(), ApiError> {
    let values = load_secrets(credentials(server)?, agent_id)?;
    hydrate_values(config, &values);
    Ok(())
}

fn scoped_secret_key(agent_id: &str, key: &str) -> String {
    if agent_id == "default" {
        key.to_owned()
    } else {
        format!("agent.{agent_id}.{key}")
    }
}

fn hydrate_values(config: &mut Value, values: &BTreeMap<String, Option<String>>) {
    for (key, path) in SECRET_PATHS {
        if let Some(target) = config.pointer_mut(path) {
            *target = Value::String(values.get(key).and_then(Clone::clone).unwrap_or_default());
        }
    }
}

fn scrub_secrets(config: &mut Value) {
    for (_, path) in SECRET_PATHS {
        if let Some(target) = config.pointer_mut(path) {
            *target = Value::String(String::new());
        }
    }
}

fn merge_json(base: &mut Value, submitted: &Value) {
    if let (Some(base), Some(submitted)) = (base.as_object_mut(), submitted.as_object()) {
        for (key, value) in submitted {
            match base.get_mut(key) {
                Some(existing) if existing.is_object() && value.is_object() => {
                    merge_json(existing, value);
                }
                _ => {
                    base.insert(key.clone(), value.clone());
                }
            }
        }
    } else {
        *base = submitted.clone();
    }
}

fn validate_shape(value: &Value, expected: &Value, path: &str) -> Result<(), ApiError> {
    match expected {
        Value::Null => Ok(()),
        Value::Bool(_) if value.is_boolean() => Ok(()),
        Value::Number(_) if value.is_number() => Ok(()),
        Value::String(_) if value.is_string() => Ok(()),
        Value::Array(expected_items) => {
            let items = value
                .as_array()
                .ok_or_else(|| bad_request(&format!("{path} must be an array")))?;
            if let Some(expected_item) = expected_items.first() {
                for (index, item) in items.iter().enumerate() {
                    validate_shape(item, expected_item, &format!("{path}[{index}]"))?;
                }
            }
            Ok(())
        }
        Value::Object(expected_fields) => {
            let fields = value
                .as_object()
                .ok_or_else(|| bad_request(&format!("{path} must be an object")))?;
            for (key, expected_value) in expected_fields {
                let field = fields
                    .get(key)
                    .ok_or_else(|| bad_request(&format!("{path}.{key} is required")))?;
                validate_shape(field, expected_value, &format!("{path}.{key}"))?;
            }
            Ok(())
        }
        _ => Err(bad_request(&format!("{path} has an invalid type"))),
    }
}

fn validate_value_bounds(value: &Value, depth: usize) -> Result<(), ApiError> {
    if depth > MAX_VALUE_DEPTH {
        return Err(payload_too_large("Agent settings are nested too deeply"));
    }
    match value {
        Value::String(value) if value.len() > MAX_STRING_BYTES => {
            Err(payload_too_large("Agent setting string is too large"))
        }
        Value::Array(values) => {
            if values.len() > MAX_COLLECTION_ITEMS {
                return Err(payload_too_large("Agent setting array is too large"));
            }
            for value in values {
                validate_value_bounds(value, depth + 1)?;
            }
            Ok(())
        }
        Value::Object(values) => {
            if values.len() > MAX_COLLECTION_ITEMS {
                return Err(payload_too_large("Agent setting object is too large"));
            }
            for (key, value) in values {
                if key.len() > 256 {
                    return Err(payload_too_large("Agent setting key is too large"));
                }
                validate_value_bounds(value, depth + 1)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

fn integer_range(config: &Value, path: &str, min: i64, max: i64) -> Result<(), ApiError> {
    let value = config
        .pointer(path)
        .and_then(Value::as_i64)
        .ok_or_else(|| bad_request(&format!("{path} must be an integer")))?;
    if !(min..=max).contains(&value) {
        return Err(bad_request(&format!(
            "{path} must be between {min} and {max}"
        )));
    }
    Ok(())
}

fn float_range(config: &Value, path: &str, min: f64, max: f64) -> Result<(), ApiError> {
    let value = number(config, path)?;
    if !value.is_finite() || value < min || value > max {
        return Err(bad_request(&format!(
            "{path} must be between {min} and {max}"
        )));
    }
    Ok(())
}

fn number(config: &Value, path: &str) -> Result<f64, ApiError> {
    config
        .pointer(path)
        .and_then(Value::as_f64)
        .ok_or_else(|| bad_request(&format!("{path} must be a number")))
}

fn string<'a>(config: &'a Value, path: &str) -> Result<&'a str, ApiError> {
    config
        .pointer(path)
        .and_then(Value::as_str)
        .ok_or_else(|| bad_request(&format!("{path} must be a string")))
}

fn boolean(config: &Value, path: &str) -> Result<bool, ApiError> {
    config
        .pointer(path)
        .and_then(Value::as_bool)
        .ok_or_else(|| bad_request(&format!("{path} must be a boolean")))
}

fn normalize_timezone(value: &str) -> Result<String, ApiError> {
    let value = value.trim();
    if value.is_empty()
        || value.len() > 128
        || value.chars().any(char::is_control)
        || value.starts_with('/')
        || value.contains("..")
        || value.contains('\\')
    {
        return Err(bad_request("Invalid IANA timezone"));
    }
    if value == "UTC" || timezone_exists(value) {
        return Ok(value.to_owned());
    }
    Err(bad_request(&format!("Invalid IANA timezone: '{value}'")))
}

fn timezone_exists(value: &str) -> bool {
    value.parse::<chrono_tz::Tz>().is_ok()
}

fn executable_on_path(name: &str) -> bool {
    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&path).any(|directory| executable_in_directory(&directory, name))
}

#[cfg(windows)]
fn executable_in_directory(directory: &Path, name: &str) -> bool {
    let extensions = std::env::var_os("PATHEXT")
        .map(|value| value.to_string_lossy().into_owned())
        .unwrap_or_else(|| String::from(".COM;.EXE;.BAT;.CMD"));
    extensions
        .split(';')
        .any(|extension| directory.join(format!("{name}{extension}")).is_file())
}

#[cfg(not(windows))]
fn executable_in_directory(directory: &Path, name: &str) -> bool {
    use std::os::unix::fs::PermissionsExt as _;

    directory
        .join(name)
        .metadata()
        .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
}

#[allow(clippy::too_many_lines)]
pub(super) fn default_running_config() -> Value {
    json!({
        "max_iters": 100,
        "loop": {
            "iteration": {"enabled": true, "max_iterations": null},
            "doom_loop": {
                "enabled": true,
                "window_size": 3,
                "similarity_threshold": 1.0,
                "stages": [
                    {
                        "after": 3,
                        "action": "modify_prompt",
                        "prompt": "[WARNING] Repetitive pattern detected. You are repeating similar actions without progress. Try a completely different approach."
                    },
                    {
                        "after": 4,
                        "action": "stop",
                        "prompt": "Doom loop: agent stuck after 4 consecutive repetitions"
                    }
                ],
                "in_loop_modes": false
            },
            "rubric": {
                "enabled": false,
                "prompt": "You did not call any tool in the last turn. If the task is truly complete, confirm it. Otherwise, continue working with tool calls.",
                "max_interventions": 1,
                "in_loop_modes": false
            },
            "goal": {"max_iterations": 20, "max_tokens": 300_000},
            "mission": {
                "max_iterations": 20,
                "max_retries_per_story": 3,
                "default_verification_instructions": "",
                "default_verify_command": ""
            },
            "custom_modes": []
        },
        "llm_retry_enabled": true,
        "llm_max_retries": 3,
        "llm_backoff_base": 1.0,
        "llm_backoff_cap": 10.0,
        "llm_max_concurrent": 10,
        "llm_max_qpm": 600,
        "llm_rate_limit_pause": 5.0,
        "llm_rate_limit_jitter": 1.0,
        "llm_acquire_timeout": 300.0,
        "shell_command_timeout": 60.0,
        "shell_command_executable": "",
        "max_input_length": 131_072,
        "history_max_length": 10000,
        "context_manager_backend": "light",
        "light_context_config": {
            "strategy": "scroll",
            "dialog_path": "dialog",
            "token_count_estimate_divisor": 4.0,
            "context_compact_config": {
                "enabled": true,
                "compact_threshold_ratio": 0.8,
                "reserve_threshold_ratio": 0.1
            },
            "tool_result_pruning_config": {
                "enabled": true,
                "pruning_recent_n": 2,
                "pruning_old_msg_max_bytes": 3000,
                "pruning_recent_msg_max_bytes": 50000,
                "offload_retention_days": 30,
                "tool_results_cache": "tool_results",
                "exempt_file_extensions": [".md"],
                "exempt_tool_names": ["chat_with_agent"]
            },
            "scroll_config": {
                "db_filename": "history.db",
                "repl_timeout_s": 300,
                "history_retention_days": 30,
                "allow_unsandboxed": false,
                "offload_dialog": false
            },
            "visual_compact_config": {"enabled": false, "effort": "low"}
        },
        "auto_title_config": {"enabled": true, "timeout_seconds": 30.0},
        "memory_manager_backend": "remelight",
        "adbpg_memory_config": null,
        "reme_light_memory_config": {
            "metadata_dir": "mem_metadata",
            "session_dir": "mem_session",
            "mem_session_dir": "mem_agent",
            "resource_dir": "resource",
            "daily_dir": "memory",
            "digest_dir": "digest",
            "auto_memory_inbox_push_enabled": true,
            "auto_dream_inbox_push_enabled": true,
            "daily_paper_inbox_push_enabled": true,
            "auto_memory_interval": 5,
            "dream_cron_enabled": true,
            "dream_cron": "0 23 * * *",
            "daily_paper_cron_enabled": false,
            "daily_paper_cron": "0 9 * * *",
            "daily_paper_use_hf_mirror": false,
            "daily_paper_topics": "",
            "auto_memory_search_config": {"enabled": false, "max_results": 2},
            "embedding_model_config": {
                "backend": "openai",
                "api_key": "",
                "base_url": "",
                "model_name": "",
                "dimensions": 1024,
                "enable_cache": true,
                "use_dimensions": false,
                "max_cache_size": 10000,
                "max_input_length": 8192,
                "max_batch_size": 10,
                "health_check_timeout": 15.0
            },
            "reranker_config": {
                "enabled": false,
                "api_key": "",
                "base_url": "",
                "model_name": "",
                "candidate_multiplier": 3,
                "timeout": 10.0
            },
            "needs_reindex": false,
            "memory_search_enabled": true
        },
        "daily_memory_dir": "memory",
        "approval_level": "AUTO"
    })
}

fn bad_request(detail: &str) -> ApiError {
    (StatusCode::BAD_REQUEST, Json(json!({"detail": detail})))
}

fn payload_too_large(detail: &str) -> ApiError {
    (
        StatusCode::PAYLOAD_TOO_LARGE,
        Json(json!({"detail": detail})),
    )
}

fn transcription_error(status: StatusCode, code: &str, message: &str) -> ApiError {
    (
        status,
        Json(json!({
            "detail": {"code": code, "message": message}
        })),
    )
}

fn internal_error(detail: &str) -> ApiError {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({"detail": detail})),
    )
}
