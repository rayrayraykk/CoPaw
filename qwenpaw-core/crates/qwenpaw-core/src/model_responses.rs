//! Native Responses requests with locally owned history and bounded SSE decoding.

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use qwenpaw_storage::StoredMessage;
use serde_json::{Map, Value, json};

use crate::model::{ModelError, ModelEvent, ModelUsage};

pub(crate) const PROTOCOL: &str = "openai-responses";
const MAX_STREAM_BYTES: usize = 4 * 1024 * 1024;
const MAX_ITEMS: usize = 4096;
const MAX_CALLS: usize = 128;

pub(crate) fn request_body(
    model: &str,
    messages: &[StoredMessage],
    tools: &[Value],
    mut parameters: Map<String, Value>,
) -> Result<Value, ModelError> {
    let input = request_input(messages)?;
    for key in ["max_completion_tokens", "max_tokens"] {
        if let Some(value) = parameters.remove(key).filter(|value| !value.is_null()) {
            parameters.entry("max_output_tokens").or_insert(value);
        }
    }
    for key in ["temperature", "top_p", "max_output_tokens"] {
        if parameters.get(key).is_some_and(Value::is_null) {
            parameters.remove(key);
        }
    }
    if parameters
        .remove("disable_thinking")
        .and_then(|value| value.as_bool())
        == Some(true)
    {
        parameters.remove("reasoning");
        // Keep the original provider's explicit model allowlist, without guessing.
        if matches!(
            model
                .rsplit('/')
                .next()
                .unwrap_or(model)
                .to_ascii_lowercase()
                .as_str(),
            "gpt-5.5"
                | "gpt-5.5-2026-04-23"
                | "gpt-5.6"
                | "gpt-5.6-luna"
                | "gpt-5.6-sol"
                | "gpt-5.6-terra"
        ) {
            parameters.insert("reasoning".into(), json!({"effort": "none"}));
        }
    }
    let mut include = match parameters.remove("include") {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Array(values)) if values.iter().all(Value::is_string) => values,
        _ => return Err(invalid("Invalid Responses include option")),
    };
    let encrypted = json!("reasoning.encrypted_content");
    if !include.contains(&encrypted) {
        include.push(encrypted);
    }
    let native_tools = tools
        .iter()
        .map(|tool| {
            let mut function = tool
                .get("function")
                .and_then(Value::as_object)
                .cloned()
                .ok_or(invalid("Invalid Responses tool definition"))?;
            if function
                .get("name")
                .and_then(Value::as_str)
                .is_none_or(str::is_empty)
            {
                return Err(invalid("Invalid Responses tool name"));
            }
            function.insert("type".into(), json!("function"));
            function.entry("strict").or_insert(json!(false));
            Ok(Value::Object(function))
        })
        .collect::<Result<Vec<_>, ModelError>>()?;
    if let Some(choice) = parameters.get_mut("tool_choice")
        && choice["type"] == "function"
        && let Some(name) = choice.pointer("/function/name").and_then(Value::as_str)
    {
        *choice = json!({"type": "function", "name": name});
    }
    parameters.insert("model".into(), json!(model));
    parameters.insert("input".into(), json!(input));
    parameters.insert("stream".into(), json!(true));
    parameters.insert("store".into(), json!(false));
    parameters.insert("include".into(), json!(include));
    if !native_tools.is_empty() {
        parameters.insert("tools".into(), json!(native_tools));
        parameters.entry("tool_choice").or_insert(json!("auto"));
    }
    Ok(Value::Object(parameters))
}

fn request_input(messages: &[StoredMessage]) -> Result<Vec<Value>, ModelError> {
    let mut input = Vec::new();
    let mut calls = BTreeSet::new();
    for message in messages {
        if message.role == "tool" {
            let id = message
                .tool_call_id
                .as_deref()
                .ok_or(invalid("Missing Responses call ID"))?;
            if !calls.remove(id) {
                return Err(invalid("Responses tool result has no matching call"));
            }
            input.push(
                json!({"type": "function_call_output", "call_id": id, "output": message.content}),
            );
            continue;
        }
        if !matches!(
            message.role.as_str(),
            "user" | "assistant" | "system" | "developer"
        ) {
            return Err(invalid("Unsupported Responses message role"));
        }
        if message.role == "assistant"
            && let Some(native) = message.provider_content.get(PROTOCOL)
        {
            let native_calls = native
                .iter()
                .filter(|item| item["type"] == "function_call")
                .collect::<Vec<_>>();
            if native_calls.len() != message.tool_calls.len() {
                return Err(invalid(
                    "Responses native history does not match tool calls",
                ));
            }
            for (native, call) in native_calls.iter().zip(&message.tool_calls) {
                if native["call_id"] != call.id
                    || native["name"] != call.function.name
                    || native["arguments"] != call.function.arguments
                    || !calls.insert(call.id.clone())
                {
                    return Err(invalid(
                        "Responses native history has inconsistent tool calls",
                    ));
                }
            }
            input.extend(native.iter().cloned());
            continue;
        }
        if let Some(parts) =
            crate::media::wire_parts(message, crate::ModelProtocol::OpenAIResponses)
        {
            input.push(json!({"role": message.role, "content": parts}));
        } else if !message.content.is_empty() {
            input.push(json!({"role": message.role, "content": message.content}));
        }
        for call in &message.tool_calls {
            if !calls.insert(call.id.clone()) {
                return Err(invalid("Duplicate Responses history call ID"));
            }
            input.push(json!({"type": "function_call", "call_id": call.id,
                "name": call.function.name, "arguments": call.function.arguments}));
        }
    }
    Ok(input)
}

#[derive(Default)]
pub(crate) struct Decoder {
    bytes: usize,
    response_id: Option<String>,
    text: String,
    completed_items: BTreeMap<usize, Value>,
}

impl Decoder {
    pub(crate) fn parse(&mut self, data: &str) -> Result<(Vec<ModelEvent>, bool), ModelError> {
        self.bytes = self.bytes.saturating_add(data.len());
        if self.bytes > MAX_STREAM_BYTES {
            return Err(invalid("Responses stream exceeds size limit"));
        }
        let event: Value = serde_json::from_str(data)?;
        let kind = string(&event, "type")?;
        if let Some(id) = event.get("response_id") {
            self.check_id(id)?;
        }
        match kind {
            "response.created" | "response.in_progress" => {
                self.check_id(&event["response"]["id"])?;
            }
            "response.output_text.delta" | "response.refusal.delta" => {
                let delta = string(&event, "delta")?;
                self.text.push_str(delta);
                return Ok((vec![ModelEvent::TextDelta(delta.to_owned())], false));
            }
            "response.output_item.done" => {
                let index = event["output_index"]
                    .as_u64()
                    .and_then(|value| usize::try_from(value).ok())
                    .filter(|index| *index < MAX_ITEMS)
                    .ok_or(invalid("Invalid Responses output index"))?;
                if !event["item"].is_object()
                    || self
                        .completed_items
                        .insert(index, event["item"].clone())
                        .is_some()
                {
                    return Err(invalid("Invalid or duplicate Responses output item"));
                }
            }
            "response.completed" => {
                self.check_id(&event["response"]["id"])?;
                return self
                    .complete(&event["response"])
                    .map(|events| (events, true));
            }
            "error" | "response.failed" | "response.incomplete" | "response.cancelled" => {
                return Err(invalid(
                    "Responses generation did not complete successfully",
                ));
            }
            _ => {}
        }
        Ok((Vec::new(), false))
    }

    fn check_id(&mut self, value: &Value) -> Result<(), ModelError> {
        let id = value
            .as_str()
            .filter(|id| !id.is_empty())
            .ok_or(invalid("Invalid Responses response ID"))?;
        if self
            .response_id
            .as_deref()
            .is_some_and(|current| current != id)
        {
            return Err(invalid("Responses response ID changed within stream"));
        }
        self.response_id = Some(id.to_owned());
        Ok(())
    }

    fn complete(&self, response: &Value) -> Result<Vec<ModelEvent>, ModelError> {
        if response["status"] != "completed"
            || response.get("error").is_some_and(|error| !error.is_null())
        {
            return Err(invalid("Invalid Responses completion status"));
        }
        let output = response["output"]
            .as_array()
            .filter(|items| items.len() <= MAX_ITEMS)
            .ok_or(invalid("Invalid Responses output"))?;
        for (index, item) in &self.completed_items {
            if output.get(*index) != Some(item) {
                return Err(invalid("Responses final output changed a completed item"));
            }
        }
        let mut text = String::new();
        let mut calls = BTreeSet::new();
        let mut events = Vec::new();
        for item in output {
            match string(item, "type")? {
                "message" => {
                    if item["role"] != "assistant" || item["status"] != "completed" {
                        return Err(invalid("Invalid Responses output message"));
                    }
                    for part in item["content"]
                        .as_array()
                        .ok_or(invalid("Invalid Responses message content"))?
                    {
                        match string(part, "type")? {
                            "output_text" => text.push_str(string(part, "text")?),
                            "refusal" => text.push_str(string(part, "refusal")?),
                            _ => return Err(invalid("Unsupported Responses message content")),
                        }
                    }
                }
                "function_call" => {
                    if item
                        .get("status")
                        .is_some_and(|status| status != "completed")
                    {
                        return Err(invalid("Incomplete Responses function call"));
                    }
                    let id = string(item, "call_id")?;
                    let name = string(item, "name")?;
                    let arguments = string(item, "arguments")?;
                    let index = calls.len();
                    if id.is_empty()
                        || name.is_empty()
                        || index >= MAX_CALLS
                        || !calls.insert(id.to_owned())
                        || !serde_json::from_str::<Value>(arguments)?.is_object()
                    {
                        return Err(invalid("Invalid Responses function call"));
                    }
                    events.push(ModelEvent::ToolCallDelta {
                        index,
                        id: Some(id.to_owned()),
                        name: Some(name.to_owned()),
                        arguments: Some(arguments.to_owned()),
                    });
                }
                "reasoning" => {
                    if !item["summary"].is_array()
                        || !item["id"].is_string()
                        || item
                            .get("encrypted_content")
                            .is_some_and(|value| !value.is_null() && !value.is_string())
                    {
                        return Err(invalid("Invalid Responses reasoning item"));
                    }
                }
                _ => return Err(invalid("Unsupported Responses output item")),
            }
        }
        if !text.starts_with(&self.text) {
            return Err(invalid("Responses completion does not match streamed text"));
        }
        if text.len() > self.text.len() {
            events.insert(0, ModelEvent::TextDelta(text[self.text.len()..].to_owned()));
        }
        events.push(ModelEvent::ProviderContent {
            protocol: PROTOCOL,
            content: output.clone(),
        });
        if let Some(usage) = response.get("usage").filter(|value| !value.is_null()) {
            events.push(usage_event(usage)?);
        }
        Ok(events)
    }
}

fn usage_event(usage: &Value) -> Result<ModelEvent, ModelError> {
    let prompt = count(usage, "input_tokens")?;
    let completion = count(usage, "output_tokens")?;
    let cached = usage.pointer("/input_tokens_details/cached_tokens");
    let cache_read_tokens = match cached {
        Some(value) => value
            .as_u64()
            .ok_or(invalid("Invalid Responses cached token count"))?,
        None => 0,
    };
    if cache_read_tokens > prompt {
        return Err(invalid("Responses cached tokens exceed input tokens"));
    }
    Ok(ModelEvent::Usage(ModelUsage {
        prompt_tokens: prompt,
        completion_tokens: completion,
        cache_read_tokens,
        cache_write_tokens: 0,
        cache_eligible_input_tokens: prompt,
        cache_observed: cached.is_some(),
    }))
}

fn invalid(message: &'static str) -> ModelError {
    ModelError::Protocol(message)
}

fn string<'a>(value: &'a Value, key: &str) -> Result<&'a str, ModelError> {
    value
        .get(key)
        .and_then(Value::as_str)
        .ok_or(invalid("Invalid Responses string field"))
}

fn count(value: &Value, key: &str) -> Result<u64, ModelError> {
    value
        .get(key)
        .and_then(Value::as_u64)
        .ok_or(invalid("Invalid Responses token count"))
}

#[cfg(test)]
#[path = "model_responses_tests.rs"]
mod tests;
