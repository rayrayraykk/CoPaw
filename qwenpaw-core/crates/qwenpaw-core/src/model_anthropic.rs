//! Native Messages encoding and bounded Anthropic streaming state.

use std::collections::BTreeMap;

use qwenpaw_storage::StoredMessage;
use serde_json::Map;
use serde_json::Value;
use serde_json::json;

use crate::model::ModelError;
use crate::model::ModelEvent;
use crate::model::ModelUsage;

pub(crate) const PROTOCOL: &str = "anthropic-messages";
const MAX_STREAM_BYTES: usize = 4 * 1024 * 1024;
const MAX_BLOCKS: usize = 128;

pub(crate) fn endpoint(base: &str) -> String {
    let base = base.trim_end_matches('/');
    if base.ends_with("/v1") {
        format!("{base}/messages")
    } else {
        format!("{base}/v1/messages")
    }
}

pub(crate) fn request_body(
    model: &str,
    messages: &[StoredMessage],
    tools: &[Value],
    mut parameters: Map<String, Value>,
) -> Result<Value, ModelError> {
    let mut system = Vec::new();
    let mut conversation = Vec::<Value>::new();
    for message in messages {
        if matches!(message.role.as_str(), "system" | "developer") {
            if !message.content.is_empty() {
                system.push(json!({"type": "text", "text": message.content}));
            }
            continue;
        }
        let role = if message.role == "tool" {
            "user"
        } else {
            &message.role
        };
        if !matches!(role, "user" | "assistant") {
            return Err(ModelError::Protocol("Unsupported Anthropic message role"));
        }
        let content = message_content(message)?;
        if content.is_empty() {
            continue;
        }
        if let Some(previous) = conversation.last_mut()
            && previous["role"] == role
        {
            previous["content"]
                .as_array_mut()
                .expect("message content is an array")
                .extend(content);
        } else {
            conversation.push(json!({"role": role, "content": content}));
        }
    }
    let max_tokens = match parameters.remove("max_tokens") {
        None => 16384,
        Some(value) => value.as_u64().filter(|value| *value > 0).unwrap_or(8192),
    };
    parameters.insert(String::from("max_tokens"), json!(max_tokens));
    apply_thinking(&mut parameters, max_tokens);
    parameters.insert(String::from("model"), json!(model));
    parameters.insert(String::from("stream"), json!(true));
    parameters.insert(String::from("messages"), json!(conversation));
    if !system.is_empty() {
        parameters.insert(String::from("system"), json!(system));
    }
    if !tools.is_empty() {
        let definitions = tools
            .iter()
            .map(|tool| {
                let function = &tool["function"];
                let name = function["name"]
                    .as_str()
                    .ok_or(ModelError::Protocol("Invalid Anthropic tool definition"))?;
                let schema = function
                    .get("parameters")
                    .filter(|schema| schema.is_object())
                    .ok_or(ModelError::Protocol("Invalid Anthropic tool schema"))?;
                let mut definition = json!({"name": name, "input_schema": schema});
                if let Some(description) = function.get("description") {
                    definition["description"] = description.clone();
                }
                Ok(definition)
            })
            .collect::<Result<Vec<_>, ModelError>>()?;
        parameters.insert(String::from("tools"), json!(definitions));
        let choice = parameters
            .entry("tool_choice")
            .or_insert_with(|| json!({"type": "auto"}));
        if let Some(value) = choice.as_str() {
            *choice = json!({"type": if value == "required" { "any" } else { value }});
        }
    }
    Ok(Value::Object(parameters))
}

fn message_content(message: &StoredMessage) -> Result<Vec<Value>, ModelError> {
    if let Some(parts) = crate::media::wire_parts(message, crate::ModelProtocol::AnthropicMessages)
    {
        return Ok(parts);
    }
    if message.role == "assistant"
        && let Some(content) = message.provider_content.get(PROTOCOL)
    {
        return Ok(content.clone());
    }
    if message.role == "tool" {
        let id = message
            .tool_call_id
            .as_deref()
            .ok_or(ModelError::Protocol("Anthropic tool result has no call ID"))?;
        let mut result =
            json!({"type": "tool_result", "tool_use_id": id, "content": message.content});
        if message.tool_error == Some(true) {
            result["is_error"] = json!(true);
        }
        return Ok(vec![result]);
    }
    let mut content = Vec::new();
    if !message.content.is_empty() {
        content.push(json!({"type": "text", "text": message.content}));
    }
    for call in &message.tool_calls {
        let input: Value = serde_json::from_str(&call.function.arguments)?;
        if !input.is_object() {
            return Err(ModelError::Protocol(
                "Anthropic tool input must be an object",
            ));
        }
        content.push(
            json!({"type": "tool_use", "id": call.id, "name": call.function.name, "input": input}),
        );
    }
    Ok(content)
}

fn apply_thinking(parameters: &mut Map<String, Value>, max_tokens: u64) {
    let enabled = parameters
        .remove("thinking_enable")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    let budget = parameters
        .remove("thinking_budget")
        .and_then(|value| value.as_u64())
        .filter(|value| *value > 0)
        .unwrap_or(max_tokens / 2);
    if parameters
        .remove("disable_thinking")
        .and_then(|value| value.as_bool())
        == Some(true)
    {
        parameters.insert(String::from("thinking"), json!({"type": "disabled"}));
    } else if enabled && !parameters.contains_key("thinking") {
        parameters.insert(
            String::from("thinking"),
            json!({"type": "enabled", "budget_tokens": budget}),
        );
        if budget >= max_tokens {
            parameters.insert(
                String::from("max_tokens"),
                json!(budget.saturating_add(1024)),
            );
        }
    }
}

struct Block {
    value: Value,
    input: String,
    closed: bool,
}

#[derive(Default)]
pub(crate) struct Decoder {
    started: bool,
    blocks: BTreeMap<usize, Block>,
    usage: Map<String, Value>,
    bytes: usize,
}

impl Decoder {
    pub(crate) fn parse(&mut self, data: &str) -> Result<(Vec<ModelEvent>, bool), ModelError> {
        self.bytes = self.bytes.saturating_add(data.len());
        if self.bytes > MAX_STREAM_BYTES {
            return Err(ModelError::Protocol(
                "Anthropic stream exceeded its size limit",
            ));
        }
        let event: Value = serde_json::from_str(data)?;
        let kind = event["type"]
            .as_str()
            .ok_or(ModelError::Protocol("Anthropic event has no type"))?;
        if kind == "message_start" {
            if self.started {
                return Err(ModelError::Protocol("Duplicate Anthropic message_start"));
            }
            self.started = true;
            self.update_usage(&event["message"]["usage"])?;
            return Ok((Vec::new(), false));
        }
        if kind == "error" {
            return Err(ModelError::Protocol("Anthropic stream reported an error"));
        }
        if matches!(
            kind,
            "content_block_start"
                | "content_block_delta"
                | "content_block_stop"
                | "message_delta"
                | "message_stop"
        ) && !self.started
        {
            return Err(ModelError::Protocol(
                "Anthropic event preceded message_start",
            ));
        }
        match kind {
            "content_block_start" => self.start_block(&event).map(|events| (events, false)),
            "content_block_delta" => self.delta(&event).map(|events| (events, false)),
            "content_block_stop" => self.stop_block(&event).map(|events| (events, false)),
            "message_delta" => {
                self.update_usage(&event["usage"])?;
                Ok((Vec::new(), false))
            }
            "message_stop" => {
                if self.blocks.values().any(|block| !block.closed) {
                    return Err(ModelError::Protocol(
                        "Anthropic message ended with an open content block",
                    ));
                }
                let mut events = Vec::new();
                if !self.usage.is_empty() {
                    events.push(ModelEvent::Usage(self.normalized_usage()));
                }
                events.push(ModelEvent::ProviderContent {
                    protocol: PROTOCOL,
                    content: std::mem::take(&mut self.blocks)
                        .into_values()
                        .map(|block| block.value)
                        .collect(),
                });
                Ok((events, true))
            }
            _ => Ok((Vec::new(), false)),
        }
    }

    fn start_block(&mut self, event: &Value) -> Result<Vec<ModelEvent>, ModelError> {
        let index = block_index(event)?;
        if self.blocks.contains_key(&index) || self.blocks.len() >= MAX_BLOCKS {
            return Err(ModelError::Protocol(
                "Duplicate or excessive Anthropic content blocks",
            ));
        }
        let value = event["content_block"].clone();
        let kind = value["type"]
            .as_str()
            .ok_or(ModelError::Protocol("Anthropic content block has no type"))?;
        let events = match kind {
            "text" => {
                let text = string(&value, "text")?;
                if text.is_empty() {
                    Vec::new()
                } else {
                    vec![ModelEvent::TextDelta(text.to_owned())]
                }
            }
            "tool_use" => {
                let id = string(&value, "id")?.to_owned();
                let name = string(&value, "name")?.to_owned();
                if id.is_empty() || name.is_empty() || !value["input"].is_object() {
                    return Err(ModelError::Protocol("Invalid Anthropic tool block"));
                }
                if self
                    .blocks
                    .values()
                    .any(|block| block.value["type"] == "tool_use" && block.value["id"] == id)
                {
                    return Err(ModelError::Protocol("Duplicate Anthropic tool call ID"));
                }
                vec![ModelEvent::ToolCallDelta {
                    index,
                    id: Some(id),
                    name: Some(name),
                    arguments: None,
                }]
            }
            _ => Vec::new(),
        };
        self.blocks.insert(
            index,
            Block {
                value,
                input: String::new(),
                closed: false,
            },
        );
        Ok(events)
    }

    fn delta(&mut self, event: &Value) -> Result<Vec<ModelEvent>, ModelError> {
        let index = block_index(event)?;
        let block = self
            .blocks
            .get_mut(&index)
            .filter(|block| !block.closed)
            .ok_or(ModelError::Protocol("Anthropic delta has no open block"))?;
        let delta = &event["delta"];
        let kind = string(delta, "type")?;
        let mut events = Vec::new();
        match kind {
            "text_delta" | "thinking_delta" | "signature_delta" => {
                let (block_type, field) = match kind {
                    "text_delta" => ("text", "text"),
                    "thinking_delta" => ("thinking", "thinking"),
                    _ => ("thinking", "signature"),
                };
                if block.value["type"] != block_type {
                    return Err(ModelError::Protocol(
                        "Anthropic delta type does not match its block",
                    ));
                }
                let value = string(delta, field)?;
                let mut current = block.value[field].as_str().unwrap_or_default().to_owned();
                current.push_str(value);
                block.value[field] = json!(current);
                if kind == "text_delta" && !value.is_empty() {
                    events.push(ModelEvent::TextDelta(value.to_owned()));
                }
            }
            "input_json_delta"
                if matches!(
                    block.value["type"].as_str(),
                    Some("tool_use" | "server_tool_use")
                ) =>
            {
                let partial = string(delta, "partial_json")?;
                block.input.push_str(partial);
                if block.value["type"] == "tool_use" {
                    events.push(ModelEvent::ToolCallDelta {
                        index,
                        id: None,
                        name: None,
                        arguments: Some(partial.to_owned()),
                    });
                }
            }
            _ => {}
        }
        Ok(events)
    }

    fn stop_block(&mut self, event: &Value) -> Result<Vec<ModelEvent>, ModelError> {
        let index = block_index(event)?;
        let block = self
            .blocks
            .get_mut(&index)
            .filter(|block| !block.closed)
            .ok_or(ModelError::Protocol("Anthropic stop has no open block"))?;
        block.closed = true;
        let mut events = Vec::new();
        if matches!(
            block.value["type"].as_str(),
            Some("tool_use" | "server_tool_use")
        ) {
            if block.input.is_empty() {
                if block.value["type"] == "tool_use" {
                    events.push(ModelEvent::ToolCallDelta {
                        index,
                        id: None,
                        name: None,
                        arguments: Some(serde_json::to_string(&block.value["input"])?),
                    });
                }
            } else {
                let input: Value = serde_json::from_str(&block.input)?;
                if !input.is_object() {
                    return Err(ModelError::Protocol(
                        "Anthropic tool input must be an object",
                    ));
                }
                block.value["input"] = input;
            }
        }
        Ok(events)
    }

    fn update_usage(&mut self, value: &Value) -> Result<(), ModelError> {
        for key in [
            "input_tokens",
            "output_tokens",
            "cache_creation_input_tokens",
            "cache_read_input_tokens",
        ] {
            if let Some(value) = value.get(key) {
                if !value.is_u64() {
                    return Err(ModelError::Protocol("Invalid Anthropic token usage"));
                }
                self.usage.insert(key.to_owned(), value.clone());
            }
        }
        Ok(())
    }

    fn normalized_usage(&self) -> ModelUsage {
        let count = |key| {
            self.usage
                .get(key)
                .and_then(Value::as_u64)
                .unwrap_or_default()
        };
        let cache_read_tokens = count("cache_read_input_tokens");
        let cache_write_tokens = count("cache_creation_input_tokens");
        let prompt_tokens = count("input_tokens")
            .saturating_add(cache_read_tokens)
            .saturating_add(cache_write_tokens);
        let cache_observed = self.usage.contains_key("cache_read_input_tokens")
            || self.usage.contains_key("cache_creation_input_tokens");
        ModelUsage {
            prompt_tokens,
            completion_tokens: count("output_tokens"),
            cache_read_tokens,
            cache_write_tokens,
            cache_eligible_input_tokens: if cache_observed { prompt_tokens } else { 0 },
            cache_observed,
        }
    }
}

fn block_index(event: &Value) -> Result<usize, ModelError> {
    event["index"]
        .as_u64()
        .and_then(|value| usize::try_from(value).ok())
        .filter(|value| *value < MAX_BLOCKS)
        .ok_or(ModelError::Protocol(
            "Invalid Anthropic content block index",
        ))
}

fn string<'a>(value: &'a Value, key: &str) -> Result<&'a str, ModelError> {
    value[key]
        .as_str()
        .ok_or(ModelError::Protocol("Invalid Anthropic event field"))
}

#[cfg(test)]
#[path = "model_anthropic_tests.rs"]
mod tests;
