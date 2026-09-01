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

const DEFAULT_BASE_URL: &str = "https://dashscope.aliyuncs.com/compatible-mode/v1";
const DEFAULT_MODEL: &str = "qwen3-coder-plus";
const MAX_BASE_URL_BYTES: usize = 2_048;
const MAX_MODEL_ID_BYTES: usize = 256;
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
    TextDelta(String),
    ToolCallDelta {
        index: usize,
        id: Option<String>,
        name: Option<String>,
        arguments: Option<String>,
    },
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
        Ok(self)
    }
}

#[derive(Clone)]
pub(crate) struct ModelClient {
    config: Arc<RwLock<ModelConfig>>,
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
            config: Arc::new(RwLock::new(config)),
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
            .clone()
    }

    pub(crate) fn replace_config(&self, config: ModelConfig) {
        *self
            .config
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = config;
    }

    pub(crate) async fn chat_stream(
        &self,
        model: &str,
        messages: &[StoredMessage],
        tools: &[Value],
    ) -> Result<DeltaStream, ModelError> {
        let config = self.config_snapshot();
        let url = format!("{}/chat/completions", config.base_url);
        let context = build_context(messages, self.context_limits)?;
        let body = ChatCompletionRequest {
            model,
            messages: &context,
            stream: true,
            tools,
            tool_choice: "auto",
        };
        let mut request = self.client.post(url).json(&body);
        if let Some(api_key) = &config.api_key {
            request = request.bearer_auth(api_key);
        }
        let response = tokio::time::timeout(self.transport_limits.header_timeout, request.send())
            .await
            .map_err(|_| ModelError::HeaderTimeout)??;
        let status = response.status();
        if !status.is_success() {
            let message =
                read_error_body(response, self.transport_limits.stream_idle_timeout).await?;
            return Err(ModelError::HttpStatus {
                status: status.as_u16(),
                message,
            });
        }
        validate_event_stream_content_type(&response)?;
        Ok(model_event_stream(
            Box::pin(response.bytes_stream()),
            self.transport_limits.stream_idle_timeout,
        ))
    }
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
}

fn model_event_stream(source: ByteStream, idle_timeout: Duration) -> DeltaStream {
    let state = ModelStreamState {
        source,
        decoder: SseDecoder::default(),
        pending: VecDeque::new(),
        idle_timeout,
        source_finished: false,
        done: false,
    };
    Box::pin(futures_util::stream::unfold(
        state,
        |mut state| async move {
            loop {
                if state.done {
                    return None;
                }
                if let Some(event) = state.pending.pop_front() {
                    return Some((Ok(event), state));
                }
                match state.decoder.next_data() {
                    Ok(Some(data)) if data.trim() == "[DONE]" => {
                        return None;
                    }
                    Ok(Some(data)) => match parse_delta(&data) {
                        Ok(events) => {
                            state.pending.extend(events);
                            continue;
                        }
                        Err(error) => return Some(stream_error(state, error)),
                    },
                    Ok(None) if state.source_finished => {
                        return Some(stream_error(state, ModelError::UnexpectedEnd));
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
    #[error("base URL must be an HTTP(S) URL of at most 2048 bytes")]
    InvalidBaseUrl,
    #[error("base URL must not contain embedded credentials")]
    CredentialsInBaseUrl,
    #[error("base URL must not contain a query string or fragment")]
    QueryOrFragmentInBaseUrl,
    #[error("default model ID must contain 1 through 256 bytes")]
    InvalidModelId,
}

#[derive(Debug, Serialize)]
struct ChatCompletionRequest<'a> {
    model: &'a str,
    messages: &'a [StoredMessage],
    stream: bool,
    tools: &'a [Value],
    tool_choice: &'static str,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionChunk {
    choices: Vec<ChatCompletionChoice>,
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

fn parse_delta(data: &str) -> Result<Vec<ModelEvent>, ModelError> {
    let chunk: ChatCompletionChunk = serde_json::from_str(data)?;
    let Some(choice) = chunk.choices.first() else {
        return Ok(Vec::new());
    };
    let mut events = Vec::new();
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
    Ok(events)
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum ModelError {
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
