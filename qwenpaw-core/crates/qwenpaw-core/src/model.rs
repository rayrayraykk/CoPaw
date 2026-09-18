use std::collections::VecDeque;
use std::env;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::RwLock;
use std::time::Duration;

use bytes::Bytes;
use futures_util::Stream;
use futures_util::StreamExt;
use qwenpaw_storage::StoredMessage;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;

use crate::context::ContextLimits;
use crate::context::build_context;
use crate::model_anthropic;
use crate::model_gemini;
use crate::model_options::ModelAuthMode;
use crate::model_options::ModelProtocol;
use crate::model_options::ModelRequestOptions;
use crate::model_responses;

const DEFAULT_BASE_URL: &str = "https://dashscope.aliyuncs.com/compatible-mode/v1";
const DEFAULT_MODEL: &str = "qwen3-coder-plus";
const MAX_BASE_URL_BYTES: usize = 2_048;
const MAX_MODEL_ID_BYTES: usize = 256;
const MAX_API_KEY_BYTES: usize = 8_192;
const MAX_ERROR_BODY_BYTES: usize = 65_536;
const MAX_SSE_EVENT_BYTES: usize = 262_144;
const DEFAULT_HEADER_TIMEOUT_MS: u64 = 60_000;
const DEFAULT_STREAM_IDLE_TIMEOUT_MS: u64 = 60_000;
const MIN_MODEL_TIMEOUT_MS: u64 = 100;
const MAX_MODEL_TIMEOUT_MS: u64 = 300_000;

pub(crate) type DeltaStream = Pin<Box<dyn Stream<Item = Result<ModelEvent, ModelError>> + Send>>;
type ByteStream = Pin<Box<dyn Stream<Item = Result<Bytes, reqwest::Error>> + Send>>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ModelEvent {
    ProviderIdentity(String),
    TextDelta(String),
    ToolCallDelta {
        index: usize,
        id: Option<String>,
        name: Option<String>,
        arguments: Option<String>,
    },
    Usage(ModelUsage),
    ProviderContent {
        protocol: &'static str,
        content: Vec<Value>,
    },
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ModelUsage {
    pub(crate) prompt_tokens: u64,
    pub(crate) completion_tokens: u64,
    pub(crate) cache_read_tokens: u64,
    pub(crate) cache_write_tokens: u64,
    pub(crate) cache_eligible_input_tokens: u64,
    pub(crate) cache_observed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelConfig {
    pub api_key: Option<String>,
    pub base_url: String,
    pub default_model: String,
}

impl ModelConfig {
    #[must_use]
    pub fn from_env() -> Self {
        let api_key = env::var("QWENPAW_API_KEY")
            .or_else(|_| env::var("OPENAI_API_KEY"))
            .ok()
            .filter(|value| !value.is_empty());
        let base_url =
            env::var("QWENPAW_BASE_URL").unwrap_or_else(|_| String::from(DEFAULT_BASE_URL));
        let default_model =
            env::var("QWENPAW_MODEL").unwrap_or_else(|_| String::from(DEFAULT_MODEL));
        Self {
            api_key,
            base_url,
            default_model,
        }
    }

    pub(crate) fn normalize(mut self) -> Result<Self, ModelConfigError> {
        let base_url = self.base_url.trim().trim_end_matches('/').to_owned();
        let default_model = self.default_model.trim().to_owned();
        base_url.clone_into(&mut self.base_url);
        default_model.clone_into(&mut self.default_model);
        if self.base_url.is_empty() || self.base_url.len() > MAX_BASE_URL_BYTES {
            return Err(ModelConfigError::InvalidBaseUrl);
        }
        let url =
            reqwest::Url::parse(&self.base_url).map_err(|_| ModelConfigError::InvalidBaseUrl)?;
        if !matches!(url.scheme(), "http" | "https") || url.host().is_none() {
            return Err(ModelConfigError::InvalidBaseUrl);
        }
        if !url.username().is_empty() || url.password().is_some() {
            return Err(ModelConfigError::CredentialsInBaseUrl);
        }
        if url.query().is_some() || url.fragment().is_some() {
            return Err(ModelConfigError::QueryOrFragmentInBaseUrl);
        }
        if self.default_model.is_empty() || self.default_model.len() > MAX_MODEL_ID_BYTES {
            return Err(ModelConfigError::InvalidModelId);
        }
        if self.api_key.as_ref().is_some_and(|api_key| {
            api_key.is_empty()
                || api_key.len() > MAX_API_KEY_BYTES
                || api_key.chars().any(char::is_control)
        }) {
            return Err(ModelConfigError::InvalidApiKey);
        }
        Ok(self)
    }
}

#[derive(Clone)]
pub(crate) struct ModelRuntime {
    pub(crate) config: ModelConfig,
    pub(crate) options: ModelRequestOptions,
}

#[derive(Clone)]
pub(crate) struct ModelClient {
    config: Arc<RwLock<ModelRuntime>>,
    client: reqwest::Client,
    context_limits: ContextLimits,
    transport_limits: ModelTransportLimits,
}

impl ModelClient {
    pub(crate) fn new(config: ModelConfig) -> Result<Self, ModelError> {
        Self::with_limits(config, ModelTransportLimits::from_env())
    }

    fn with_limits(
        config: ModelConfig,
        transport_limits: ModelTransportLimits,
    ) -> Result<Self, ModelError> {
        let client = reqwest::Client::builder()
            .connect_timeout(transport_limits.header_timeout)
            .redirect(reqwest::redirect::Policy::none())
            .build()?;
        Ok(Self {
            config: Arc::new(RwLock::new(ModelRuntime {
                config,
                options: ModelRequestOptions::default(),
            })),
            client,
            context_limits: ContextLimits::from_env(),
            transport_limits,
        })
    }

    pub(crate) fn default_model(&self) -> String {
        self.config_snapshot().default_model
    }

    pub(crate) fn config_snapshot(&self) -> ModelConfig {
        self.config
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .config
            .clone()
    }

    pub(crate) fn runtime_snapshot(&self) -> ModelRuntime {
        self.config
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    pub(crate) fn write_runtime(&self) -> std::sync::RwLockWriteGuard<'_, ModelRuntime> {
        self.config
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub(crate) fn replace_runtime(&self, runtime: ModelRuntime) {
        *self.write_runtime() = runtime;
    }

    pub(crate) fn with_runtime(&self, runtime: ModelRuntime) -> Self {
        Self {
            config: Arc::new(RwLock::new(runtime)),
            ..self.clone()
        }
    }

    pub(crate) async fn chat_stream(
        &self,
        model: &str,
        messages: &[StoredMessage],
        tools: &[Value],
    ) -> Result<DeltaStream, ModelError> {
        let runtime = self.runtime_snapshot();
        let config = &runtime.config;
        let context = build_context(messages, self.context_limits)?;
        let (url, body) = model_request(&runtime, model, &context, tools)?;
        let mut request = self.client.post(url).json(&body);
        let anthropic = runtime.options.protocol == ModelProtocol::AnthropicMessages;
        let gemini = runtime.options.protocol == ModelProtocol::GeminiGenerateContent;
        if anthropic {
            request = request.header("anthropic-version", "2023-06-01");
        }
        if let Some(api_key) = &config.api_key {
            request = if anthropic && runtime.options.auth_mode == ModelAuthMode::ApiKey {
                request.header("x-api-key", api_key)
            } else if gemini && runtime.options.auth_mode == ModelAuthMode::ApiKey {
                request.header("x-goog-api-key", api_key)
            } else {
                request.bearer_auth(api_key)
            };
        }
        let mut headers = reqwest::header::HeaderMap::new();
        for (name, value) in &runtime.options.custom_headers {
            if runtime.options.auth_mode == ModelAuthMode::BearerToken
                && ((anthropic && name.eq_ignore_ascii_case("x-api-key"))
                    || (gemini && name.eq_ignore_ascii_case("x-goog-api-key")))
            {
                continue;
            }
            headers.insert(
                reqwest::header::HeaderName::from_bytes(name.as_bytes())
                    .expect("validated header name"),
                reqwest::header::HeaderValue::from_str(value).expect("validated header value"),
            );
        }
        request = request.headers(headers);
        let response = tokio::time::timeout(self.transport_limits.header_timeout, request.send())
            .await
            .map_err(|_| ModelError::HeaderTimeout)??;
        let status = response.status();
        if !status.is_success() {
            let mut message =
                read_error_body(response, self.transport_limits.stream_idle_timeout).await?;
            for secret in config
                .api_key
                .iter()
                .chain(runtime.options.custom_headers.values())
            {
                if !secret.is_empty() {
                    message = message.replace(secret, "[REDACTED]");
                    if let Some((scheme, token)) = secret.split_once(' ')
                        && scheme.eq_ignore_ascii_case("bearer")
                        && !token.is_empty()
                    {
                        message = message.replace(token, "[REDACTED]");
                    }
                }
            }
            return Err(ModelError::HttpStatus {
                status: status.as_u16(),
                message,
            });
        }
        validate_event_stream_content_type(&response)?;
        Ok(model_event_stream(
            Box::pin(response.bytes_stream()),
            self.transport_limits.stream_idle_timeout,
            runtime.options.protocol,
            runtime
                .options
                .provider_id
                .clone()
                .or_else(|| anthropic.then(|| String::from("anthropic")))
                .or_else(|| gemini.then(|| String::from("gemini"))),
        ))
    }
}

fn model_request(
    runtime: &ModelRuntime,
    model: &str,
    messages: &[StoredMessage],
    tools: &[Value],
) -> Result<(String, Value), ModelError> {
    let parameters = runtime.options.generation_for(model);
    if runtime.options.protocol == ModelProtocol::OpenAIResponses {
        return Ok((
            format!("{}/responses", runtime.config.base_url),
            model_responses::request_body(model, messages, tools, parameters)?,
        ));
    }
    if runtime.options.protocol == ModelProtocol::GeminiGenerateContent {
        return Ok((
            model_gemini::endpoint(&runtime.config.base_url, model)?,
            model_gemini::request_body(messages, tools, parameters)?,
        ));
    }
    if runtime.options.protocol == ModelProtocol::AnthropicMessages {
        return Ok((
            model_anthropic::endpoint(&runtime.config.base_url),
            model_anthropic::request_body(model, messages, tools, parameters)?,
        ));
    }
    let mut body = serde_json::to_value(ChatCompletionRequest {
        model,
        messages,
        stream: true,
        stream_options: ChatCompletionStreamOptions {
            include_usage: true,
        },
        tools,
        tool_choice: "auto",
    })?;
    // Opaque native history is persisted locally, never part of OpenAI's wire format.
    for (message, stored) in body["messages"]
        .as_array_mut()
        .expect("messages are an array")
        .iter_mut()
        .zip(messages)
    {
        let message = message.as_object_mut().expect("message is an object");
        message.remove("provider_content");
        message.remove("tool_error");
        message.remove("user_input");
        if let Some(parts) = crate::media::wire_parts(stored, ModelProtocol::OpenAIChat) {
            message.insert(String::from("content"), Value::Array(parts));
        }
    }
    body.as_object_mut()
        .expect("chat request is an object")
        .extend(parameters);
    Ok((
        format!("{}/chat/completions", runtime.config.base_url),
        body,
    ))
}

#[derive(Debug, Clone, Copy)]
struct ModelTransportLimits {
    header_timeout: Duration,
    stream_idle_timeout: Duration,
}

impl ModelTransportLimits {
    fn from_env() -> Self {
        Self {
            header_timeout: timeout_from_env(
                "QWENPAW_MODEL_HEADER_TIMEOUT_MS",
                DEFAULT_HEADER_TIMEOUT_MS,
            ),
            stream_idle_timeout: timeout_from_env(
                "QWENPAW_MODEL_STREAM_IDLE_TIMEOUT_MS",
                DEFAULT_STREAM_IDLE_TIMEOUT_MS,
            ),
        }
    }
}

struct ModelStreamState {
    source: ByteStream,
    decoder: SseDecoder,
    pending: VecDeque<ModelEvent>,
    idle_timeout: Duration,
    source_finished: bool,
    done: bool,
    anthropic: Option<model_anthropic::Decoder>,
    gemini: Option<model_gemini::Decoder>,
    responses: Option<model_responses::Decoder>,
}

impl ModelStreamState {
    fn finish_source(&mut self) -> Result<(), ModelError> {
        if self.responses.is_some() {
            return Err(ModelError::Protocol(
                "Responses stream ended before completion",
            ));
        }
        if let Some(gemini) = &self.gemini {
            self.pending.extend(gemini.finish()?);
            self.done = true;
            Ok(())
        } else if self.anthropic.is_some() {
            Err(ModelError::Protocol(
                "Anthropic stream ended before message_stop",
            ))
        } else {
            Err(ModelError::UnexpectedEnd)
        }
    }
}

pub(super) fn model_event_stream(
    source: ByteStream,
    idle_timeout: Duration,
    protocol: ModelProtocol,
    provider_id: Option<String>,
) -> DeltaStream {
    let state = ModelStreamState {
        source,
        decoder: SseDecoder::default(),
        pending: provider_id
            .map(ModelEvent::ProviderIdentity)
            .into_iter()
            .collect(),
        idle_timeout,
        source_finished: false,
        done: false,
        anthropic: (protocol == ModelProtocol::AnthropicMessages)
            .then(model_anthropic::Decoder::default),
        gemini: (protocol == ModelProtocol::GeminiGenerateContent)
            .then(model_gemini::Decoder::default),
        responses: (protocol == ModelProtocol::OpenAIResponses)
            .then(model_responses::Decoder::default),
    };
    Box::pin(futures_util::stream::unfold(
        state,
        |mut state| async move {
            loop {
                if let Some(event) = state.pending.pop_front() {
                    return Some((Ok(event), state));
                }
                if state.done {
                    return None;
                }
                match state.decoder.next_data() {
                    Ok(Some(data))
                        if state.anthropic.is_none()
                            && state.gemini.is_none()
                            && state.responses.is_none()
                            && data.trim() == "[DONE]" =>
                    {
                        return None;
                    }
                    Ok(Some(data)) if state.gemini.is_some() => {
                        match state.gemini.as_mut().expect("native decoder").parse(&data) {
                            Ok(events) => {
                                state.pending.extend(events);
                                continue;
                            }
                            Err(error) => return Some(stream_error(state, error)),
                        }
                    }
                    Ok(Some(data)) if state.anthropic.is_some() || state.responses.is_some() => {
                        let result = if let Some(responses) = &mut state.responses {
                            responses.parse(&data)
                        } else {
                            state
                                .anthropic
                                .as_mut()
                                .expect("native decoder")
                                .parse(&data)
                        };
                        match result {
                            Ok((events, done)) => {
                                state.pending.extend(events);
                                state.done = done;
                                continue;
                            }
                            Err(error) => return Some(stream_error(state, error)),
                        }
                    }
                    Ok(Some(data)) => match parse_delta(&data) {
                        Ok(events) => {
                            state.pending.extend(events);
                            continue;
                        }
                        Err(error) => return Some(stream_error(state, error)),
                    },
                    Ok(None) if state.source_finished => {
                        if let Err(error) = state.finish_source() {
                            return Some(stream_error(state, error));
                        }
                        continue;
                    }
                    Ok(None) => {}
                    Err(error) => return Some(stream_error(state, error)),
                }
                let chunk = tokio::time::timeout(state.idle_timeout, state.source.next()).await;
                match chunk {
                    Err(_) => return Some(stream_error(state, ModelError::StreamIdleTimeout)),
                    Ok(Some(Ok(chunk))) => {
                        if let Err(error) = state.decoder.push(&chunk) {
                            return Some(stream_error(state, error));
                        }
                    }
                    Ok(Some(Err(error))) => {
                        return Some(stream_error(state, ModelError::Request(error)));
                    }
                    Ok(None) => {
                        state.source_finished = true;
                        state.decoder.finish();
                    }
                }
            }
        },
    ))
}

fn stream_error(
    mut state: ModelStreamState,
    error: ModelError,
) -> (Result<ModelEvent, ModelError>, ModelStreamState) {
    state.done = true;
    (Err(error), state)
}

#[derive(Default)]
struct SseDecoder {
    buffer: Vec<u8>,
    finished: bool,
}

impl SseDecoder {
    fn push(&mut self, chunk: &[u8]) -> Result<(), ModelError> {
        self.buffer.extend_from_slice(chunk);
        self.validate_next_event_size()
    }

    fn finish(&mut self) {
        self.finished = true;
    }

    fn next_data(&mut self) -> Result<Option<String>, ModelError> {
        loop {
            let event = if let Some((event_end, delimiter_len)) = sse_event_end(&self.buffer) {
                if event_end > MAX_SSE_EVENT_BYTES {
                    return Err(ModelError::EventTooLarge);
                }
                let event = self.buffer.drain(..event_end).collect::<Vec<_>>();
                self.buffer.drain(..delimiter_len);
                event
            } else if self.finished && !self.buffer.is_empty() {
                if self.buffer.len() > MAX_SSE_EVENT_BYTES {
                    return Err(ModelError::EventTooLarge);
                }
                std::mem::take(&mut self.buffer)
            } else {
                self.validate_next_event_size()?;
                return Ok(None);
            };
            if let Some(data) = parse_sse_data(&event)? {
                return Ok(Some(data));
            }
        }
    }

    fn validate_next_event_size(&self) -> Result<(), ModelError> {
        if sse_event_end(&self.buffer).map_or(self.buffer.len(), |(event_end, _)| event_end)
            > MAX_SSE_EVENT_BYTES
        {
            return Err(ModelError::EventTooLarge);
        }
        Ok(())
    }
}

fn parse_sse_data(bytes: &[u8]) -> Result<Option<String>, ModelError> {
    let text = std::str::from_utf8(bytes).map_err(|_| ModelError::InvalidUtf8)?;
    let mut data = Vec::new();
    for line in text.split(['\r', '\n']) {
        if line.starts_with(':') {
            continue;
        }
        let (field, value) = line.split_once(':').unwrap_or((line, ""));
        let value = value.strip_prefix(' ').unwrap_or(value);
        if field == "data" {
            data.push(value);
        }
    }
    if data.is_empty() {
        Ok(None)
    } else {
        Ok(Some(data.join("\n")))
    }
}

fn sse_event_end(buffer: &[u8]) -> Option<(usize, usize)> {
    [
        b"\r\n\r\n".as_slice(),
        b"\n\n".as_slice(),
        b"\r\r".as_slice(),
    ]
    .into_iter()
    .filter_map(|delimiter| {
        buffer
            .windows(delimiter.len())
            .position(|window| window == delimiter)
            .map(|position| (position, delimiter.len()))
    })
    .min_by_key(|(position, _)| *position)
}

fn validate_event_stream_content_type(response: &reqwest::Response) -> Result<(), ModelError> {
    let is_event_stream = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .is_some_and(|mime| mime.trim().eq_ignore_ascii_case("text/event-stream"));
    if !is_event_stream {
        return Err(ModelError::UnexpectedContentType);
    }
    Ok(())
}

async fn read_error_body(
    response: reqwest::Response,
    idle_timeout: Duration,
) -> Result<String, ModelError> {
    if response
        .content_length()
        .is_some_and(|length| length > MAX_ERROR_BODY_BYTES as u64)
    {
        return Err(ModelError::ErrorBodyTooLarge);
    }
    let mut body = Vec::new();
    let mut stream = response.bytes_stream();
    loop {
        match tokio::time::timeout(idle_timeout, stream.next()).await {
            Err(_) => return Err(ModelError::StreamIdleTimeout),
            Ok(Some(Ok(chunk))) => {
                if body.len().saturating_add(chunk.len()) > MAX_ERROR_BODY_BYTES {
                    return Err(ModelError::ErrorBodyTooLarge);
                }
                body.extend_from_slice(&chunk);
            }
            Ok(Some(Err(error))) => return Err(ModelError::Request(error)),
            Ok(None) => break,
        }
    }
    Ok(String::from_utf8_lossy(&body).trim().to_owned())
}

fn timeout_from_env(key: &str, default_ms: u64) -> Duration {
    let milliseconds = env::var(key)
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(default_ms)
        .clamp(MIN_MODEL_TIMEOUT_MS, MAX_MODEL_TIMEOUT_MS);
    Duration::from_millis(milliseconds)
}

#[derive(Debug, Clone, Copy, thiserror::Error, PartialEq, Eq)]
pub(crate) enum ModelConfigError {
    #[error("model request options contain invalid headers, parameters, or protocol overrides")]
    InvalidRequestOptions,
    #[error("base URL must be an HTTP(S) URL of at most 2048 bytes")]
    InvalidBaseUrl,
    #[error("base URL must not contain embedded credentials")]
    CredentialsInBaseUrl,
    #[error("base URL must not contain a query string or fragment")]
    QueryOrFragmentInBaseUrl,
    #[error("default model ID must contain 1 through 256 bytes")]
    InvalidModelId,
    #[error("API key must contain 1 through 8192 bytes without control characters")]
    InvalidApiKey,
}

#[derive(Debug, Serialize)]
struct ChatCompletionRequest<'a> {
    model: &'a str,
    messages: &'a [StoredMessage],
    stream: bool,
    stream_options: ChatCompletionStreamOptions,
    tools: &'a [Value],
    tool_choice: &'static str,
}

#[derive(Debug, Clone, Copy, Serialize)]
struct ChatCompletionStreamOptions {
    include_usage: bool,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionChunk {
    #[serde(default)]
    choices: Vec<ChatCompletionChoice>,
    #[serde(default)]
    usage: Option<ChatCompletionUsage>,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionChoice {
    delta: ChatCompletionDelta,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionDelta {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    tool_calls: Vec<ChatCompletionToolCallDelta>,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionToolCallDelta {
    index: usize,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    function: Option<ChatCompletionFunctionDelta>,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionFunctionDelta {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    arguments: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct ChatCompletionUsage {
    #[serde(default, alias = "input_tokens")]
    prompt_tokens: u64,
    #[serde(default, alias = "output_tokens")]
    completion_tokens: u64,
    #[serde(default, alias = "input_tokens_details")]
    prompt_tokens_details: Option<PromptTokenDetails>,
    #[serde(default)]
    cache_read_input_tokens: Option<u64>,
    #[serde(default)]
    cache_creation_input_tokens: Option<u64>,
}

#[derive(Debug, Default, Deserialize)]
struct PromptTokenDetails {
    #[serde(default)]
    cached_tokens: u64,
}

fn parse_delta(data: &str) -> Result<Vec<ModelEvent>, ModelError> {
    let chunk: ChatCompletionChunk = serde_json::from_str(data)?;
    let mut events = Vec::new();
    if let Some(choice) = chunk.choices.first() {
        if let Some(content) = &choice.delta.content
            && !content.is_empty()
        {
            events.push(ModelEvent::TextDelta(content.clone()));
        }
        events.extend(choice.delta.tool_calls.iter().map(|call| {
            ModelEvent::ToolCallDelta {
                index: call.index,
                id: call.id.clone(),
                name: call
                    .function
                    .as_ref()
                    .and_then(|function| function.name.clone()),
                arguments: call
                    .function
                    .as_ref()
                    .and_then(|function| function.arguments.clone()),
            }
        }));
    }
    if let Some(usage) = chunk.usage {
        events.push(ModelEvent::Usage(normalize_usage(usage)));
    }
    Ok(events)
}

fn normalize_usage(usage: ChatCompletionUsage) -> ModelUsage {
    let details_observed = usage.prompt_tokens_details.is_some();
    let detail_cache_read = usage
        .prompt_tokens_details
        .map_or(0, |details| details.cached_tokens);
    let cache_read_tokens = usage.cache_read_input_tokens.unwrap_or(detail_cache_read);
    let cache_write_tokens = usage.cache_creation_input_tokens.unwrap_or_default();
    let cache_observed = details_observed
        || usage.cache_read_input_tokens.is_some()
        || usage.cache_creation_input_tokens.is_some();
    let cache_is_valid =
        cache_read_tokens.saturating_add(cache_write_tokens) <= usage.prompt_tokens;
    ModelUsage {
        prompt_tokens: usage.prompt_tokens,
        completion_tokens: usage.completion_tokens,
        cache_read_tokens: if cache_observed && cache_is_valid {
            cache_read_tokens
        } else {
            0
        },
        cache_write_tokens: if cache_observed && cache_is_valid {
            cache_write_tokens
        } else {
            0
        },
        cache_eligible_input_tokens: if cache_observed && cache_is_valid {
            usage.prompt_tokens
        } else {
            0
        },
        cache_observed: cache_observed && cache_is_valid,
    }
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum ModelError {
    #[error("{0}")]
    Protocol(&'static str),
    #[error("model context could not be built: {0}")]
    Context(#[from] crate::context::ContextError),
    #[error("model request failed: {0}")]
    Request(#[from] reqwest::Error),
    #[error("model response headers exceeded the configured timeout")]
    HeaderTimeout,
    #[error("model stream exceeded the configured idle timeout")]
    StreamIdleTimeout,
    #[error("model returned HTTP {status}: {message}")]
    HttpStatus { status: u16, message: String },
    #[error("model error body exceeded the 65536-byte limit")]
    ErrorBodyTooLarge,
    #[error("model response did not use text/event-stream")]
    UnexpectedContentType,
    #[error("model SSE event exceeded the 262144-byte limit")]
    EventTooLarge,
    #[error("model SSE event was not UTF-8")]
    InvalidUtf8,
    #[error("model stream ended before the [DONE] event")]
    UnexpectedEnd,
    #[error("model returned invalid JSON: {0}")]
    InvalidJson(#[from] serde_json::Error),
}

#[cfg(test)]
#[path = "model_tests.rs"]
mod tests;
