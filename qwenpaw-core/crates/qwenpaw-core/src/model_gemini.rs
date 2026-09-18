//! Native `GenerateContent` requests and bounded SSE response accumulation.

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use qwenpaw_storage::StoredMessage;
use serde_json::Map;
use serde_json::Value;
use serde_json::json;

use crate::model::{ModelError, ModelEvent, ModelUsage};

#[path = "model_gemini_schema.rs"]
mod schema;

pub(crate) const PROTOCOL: &str = "gemini-generate-content";
const MAX_STREAM_BYTES: usize = 4 * 1024 * 1024;
const MAX_PARTS: usize = 4096;
const MAX_CALLS: usize = 128;

pub(crate) fn endpoint(base: &str, model: &str) -> Result<String, ModelError> {
    let model = model.strip_prefix("models/").unwrap_or(model);
    if !model
        .bytes()
        .next()
        .is_some_and(|byte| byte.is_ascii_alphanumeric())
        || !model
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-_.".contains(&byte))
    {
        return Err(ModelError::Protocol("Invalid Gemini model resource name"));
    }
    let root = base.trim_end_matches('/');
    let version = if root.ends_with("/v1beta") || root.ends_with("/v1") {
        ""
    } else {
        "/v1beta"
    };
    Ok(format!(
        "{root}{version}/models/{model}:streamGenerateContent?alt=sse"
    ))
}

pub(crate) fn request_body(
    messages: &[StoredMessage],
    tools: &[Value],
    parameters: Map<String, Value>,
) -> Result<Value, ModelError> {
    let mut system = Vec::new();
    let mut contents = Vec::<Value>::new();
    let mut calls = BTreeMap::<String, (String, Option<String>)>::new();
    for message in messages {
        if matches!(message.role.as_str(), "system" | "developer") {
            if !message.content.is_empty() {
                system.push(json!({"text": message.content}));
            }
            continue;
        }
        let role = match message.role.as_str() {
            "assistant" => "model",
            "user" | "tool" => "user",
            _ => return Err(ModelError::Protocol("Unsupported Gemini message role")),
        };
        let parts = message_parts(message, &mut calls)?;
        if parts.is_empty() {
            continue;
        }
        if let Some(last) = contents.last_mut()
            && last["role"] == role
        {
            last["parts"]
                .as_array_mut()
                .expect("parts array")
                .extend(parts);
        } else {
            contents.push(json!({"role": role, "parts": parts}));
        }
    }
    let mut body = generation_body(parameters)?;
    body.insert(String::from("contents"), json!(contents));
    if !system.is_empty() {
        body.insert(String::from("systemInstruction"), json!({"parts": system}));
    }
    if !tools.is_empty() {
        let declarations = tools
            .iter()
            .map(|tool| {
                let function = tool
                    .get("function")
                    .ok_or(ModelError::Protocol("Invalid Gemini tool definition"))?;
                let name = function
                    .get("name")
                    .and_then(Value::as_str)
                    .filter(|name| !name.is_empty())
                    .ok_or(ModelError::Protocol("Invalid Gemini tool name"))?;
                let mut declaration = json!({"name": name});
                if let Some(description) = function.get("description") {
                    declaration["description"] = description.clone();
                }
                if let Some(parameters) = function.get("parameters") {
                    declaration["parameters"] = schema::normalize(parameters)?;
                }
                Ok(declaration)
            })
            .collect::<Result<Vec<_>, ModelError>>()?;
        body.insert(
            String::from("tools"),
            json!([{"functionDeclarations": declarations}]),
        );
        body.entry("toolConfig")
            .or_insert_with(|| json!({"functionCallingConfig": {"mode": "AUTO"}}));
    }
    Ok(Value::Object(body))
}

fn message_parts(
    message: &StoredMessage,
    calls: &mut BTreeMap<String, (String, Option<String>)>,
) -> Result<Vec<Value>, ModelError> {
    if let Some(parts) =
        crate::media::wire_parts(message, crate::ModelProtocol::GeminiGenerateContent)
    {
        return Ok(parts);
    }
    if message.role == "tool" {
        let id = message
            .tool_call_id
            .as_deref()
            .ok_or(ModelError::Protocol("Gemini tool result has no call ID"))?;
        let (name, native_id) = calls.get(id).ok_or(ModelError::Protocol(
            "Gemini tool result has no matching call",
        ))?;
        let response = if message.tool_error == Some(true) {
            json!({"error": message.content})
        } else {
            json!({"output": message.content})
        };
        let mut result = json!({"name": name, "response": response});
        if let Some(id) = native_id {
            result["id"] = json!(id);
        }
        return Ok(vec![json!({"functionResponse": result})]);
    }
    let native = if message.role == "assistant" {
        message.provider_content.get(PROTOCOL)
    } else {
        None
    };
    let native_calls = native
        .into_iter()
        .flatten()
        .filter_map(|part| part.get("functionCall"))
        .collect::<Vec<_>>();
    if native.is_some() && native_calls.len() != message.tool_calls.len() {
        return Err(ModelError::Protocol(
            "Gemini native history does not match tool calls",
        ));
    }
    let mut parts = Vec::new();
    if !message.content.is_empty() {
        parts.push(json!({"text": message.content}));
    }
    for (index, call) in message.tool_calls.iter().enumerate() {
        if native.is_some() && native_calls[index]["name"] != call.function.name {
            return Err(ModelError::Protocol(
                "Gemini native history does not match tool name",
            ));
        }
        let args: Value = serde_json::from_str(&call.function.arguments)?;
        if !args.is_object() {
            return Err(ModelError::Protocol(
                "Gemini tool arguments must be an object",
            ));
        }
        let id = if native.is_some() {
            native_calls
                .get(index)
                .and_then(|call| call.get("id"))
                .and_then(Value::as_str)
                .map(str::to_owned)
        } else {
            Some(call.id.clone())
        };
        calls.insert(call.id.clone(), (call.function.name.clone(), id));
        parts.push(
            json!({"functionCall": {"id": call.id, "name": call.function.name, "args": args}}),
        );
    }
    Ok(native.cloned().unwrap_or(parts))
}

fn camel_case(key: &str) -> String {
    let mut parts = key.split('_');
    let mut result = parts.next().unwrap_or_default().to_owned();
    for part in parts {
        let mut chars = part.chars();
        if let Some(first) = chars.next() {
            result.extend(first.to_uppercase());
            result.push_str(chars.as_str());
        }
    }
    result
}

fn generation_body(mut parameters: Map<String, Value>) -> Result<Map<String, Value>, ModelError> {
    if let Some(value) = parameters.remove("max_tokens")
        && !value.is_null()
    {
        parameters.entry("max_output_tokens").or_insert(value);
    }
    let disabled = parameters
        .remove("disable_thinking")
        .and_then(|value| value.as_bool())
        == Some(true);
    let enabled = !disabled
        && parameters
            .remove("thinking_enable")
            .and_then(|value| value.as_bool())
            == Some(true);
    parameters.remove("thinking_enable");
    let budget = parameters.remove("thinking_budget").unwrap_or(json!(1024));
    let thinking = parameters
        .remove("thinking_config")
        .or_else(|| parameters.remove("thinkingConfig"));
    let mut generation = Map::new();
    let mut body = Map::new();
    if let Some(choice) = parameters.remove("tool_choice") {
        let name = choice
            .as_str()
            .or_else(|| choice.pointer("/function/name").and_then(Value::as_str))
            .ok_or(ModelError::Protocol("Invalid Gemini tool choice"))?;
        let config = match name {
            "auto" => json!({"mode": "AUTO"}),
            "none" => json!({"mode": "NONE"}),
            "required" => json!({"mode": "ANY"}),
            "" => return Err(ModelError::Protocol("Invalid Gemini tool choice")),
            name => json!({"mode": "ANY", "allowedFunctionNames": [name]}),
        };
        body.insert(
            String::from("toolConfig"),
            json!({"functionCallingConfig": config}),
        );
    }
    for (key, value) in parameters {
        if value.is_null() {
            continue;
        }
        let key = camel_case(&key);
        if matches!(
            key.as_str(),
            "contents"
                | "systemInstruction"
                | "generationConfig"
                | "tools"
                | "model"
                | "messages"
                | "stream"
                | "streamOptions"
        ) {
            return Err(ModelError::Protocol(
                "Gemini generation parameters override protocol fields",
            ));
        }
        if matches!(
            key.as_str(),
            "safetySettings" | "cachedContent" | "toolConfig" | "serviceTier" | "store"
        ) {
            body.insert(key, config_keys(&value));
        } else if key == "responseSchema" {
            generation.insert(key, schema::normalize(&value)?);
        } else {
            generation.insert(key, value);
        }
    }
    let thinking = if let Some(value) = thinking.filter(|_| !disabled) {
        let values = value.as_object().ok_or(ModelError::Protocol(
            "Gemini thinking config must be an object",
        ))?;
        Value::Object(
            values
                .iter()
                .map(|(key, value)| (camel_case(key), value.clone()))
                .collect(),
        )
    } else {
        json!({"includeThoughts": enabled, "thinkingBudget": if enabled {budget} else {json!(0)}})
    };
    generation.insert(String::from("thinkingConfig"), thinking);
    body.insert(String::from("generationConfig"), Value::Object(generation));
    Ok(body)
}

fn config_keys(value: &Value) -> Value {
    match value {
        Value::Object(values) => Value::Object(
            values
                .iter()
                .map(|(key, value)| (camel_case(key), config_keys(value)))
                .collect(),
        ),
        Value::Array(values) => Value::Array(values.iter().map(config_keys).collect()),
        _ => value.clone(),
    }
}

#[derive(Default)]
pub(crate) struct Decoder {
    bytes: usize,
    parts: Vec<Value>,
    calls: BTreeSet<String>,
    usage: Map<String, Value>,
    finished: bool,
}

impl Decoder {
    pub(crate) fn parse(&mut self, data: &str) -> Result<Vec<ModelEvent>, ModelError> {
        self.bytes = self.bytes.saturating_add(data.len());
        if self.bytes > MAX_STREAM_BYTES {
            return Err(ModelError::Protocol("Gemini stream exceeds size limit"));
        }
        let response: Value = serde_json::from_str(data)?;
        if !response.is_object() {
            return Err(ModelError::Protocol("Invalid Gemini response"));
        }
        if response.get("error").is_some() {
            return Err(ModelError::Protocol("Gemini returned a streaming error"));
        }
        if response
            .pointer("/promptFeedback/blockReason")
            .and_then(Value::as_str)
            .is_some_and(|reason| reason != "BLOCK_REASON_UNSPECIFIED")
        {
            return Err(ModelError::Protocol("Gemini blocked the prompt"));
        }
        if let Some(usage) = response.get("usageMetadata") {
            let usage = usage
                .as_object()
                .ok_or(ModelError::Protocol("Invalid Gemini usage"))?;
            for key in [
                "promptTokenCount",
                "candidatesTokenCount",
                "thoughtsTokenCount",
                "cachedContentTokenCount",
                "totalTokenCount",
            ] {
                if let Some(value) = usage.get(key) {
                    if value.as_u64().is_none() {
                        return Err(ModelError::Protocol("Invalid Gemini token count"));
                    }
                    self.usage.insert(key.to_owned(), value.clone());
                }
            }
        }
        let mut events = Vec::new();
        if let Some(candidates) = response.get("candidates") {
            let candidates = candidates
                .as_array()
                .ok_or(ModelError::Protocol("Invalid Gemini candidates"))?;
            if candidates.iter().any(|candidate| {
                !candidate.is_object()
                    || candidate
                        .get("index")
                        .is_some_and(|index| index.as_u64().is_none())
            }) {
                return Err(ModelError::Protocol("Invalid Gemini candidate"));
            }
            if let Some(candidate) = candidates
                .iter()
                .find(|candidate| candidate.get("index").and_then(Value::as_u64).unwrap_or(0) == 0)
            {
                self.candidate(candidate, &mut events)?;
            }
        }
        Ok(events)
    }

    fn candidate(
        &mut self,
        candidate: &Value,
        events: &mut Vec<ModelEvent>,
    ) -> Result<(), ModelError> {
        if let Some(content) = candidate.get("content") {
            if self.finished {
                return Err(ModelError::Protocol("Gemini content after completion"));
            }
            if content.get("role").is_some_and(|role| role != "model") {
                return Err(ModelError::Protocol("Invalid Gemini response role"));
            }
            let parts = content
                .get("parts")
                .and_then(Value::as_array)
                .ok_or(ModelError::Protocol("Invalid Gemini response parts"))?;
            for part in parts {
                if self.parts.len() >= MAX_PARTS {
                    return Err(ModelError::Protocol("Too many Gemini response parts"));
                }
                if !part.is_object()
                    || part.get("thought").is_some_and(|value| !value.is_boolean())
                    || (part.get("functionCall").is_some() && part.get("text").is_some())
                {
                    return Err(ModelError::Protocol("Invalid Gemini response part"));
                }
                if let Some(call) = part.get("functionCall") {
                    let name = call
                        .get("name")
                        .and_then(Value::as_str)
                        .filter(|name| !name.is_empty())
                        .ok_or(ModelError::Protocol("Gemini function call has no name"))?;
                    let args = call.get("args").cloned().unwrap_or(json!({}));
                    if !args.is_object() {
                        return Err(ModelError::Protocol("Invalid Gemini function arguments"));
                    }
                    let id = match call.get("id") {
                        None => format!("gemini_{}", uuid::Uuid::now_v7()),
                        Some(value) => value
                            .as_str()
                            .filter(|id| !id.is_empty())
                            .ok_or(ModelError::Protocol("Invalid Gemini call ID"))?
                            .to_owned(),
                    };
                    let index = self.calls.len();
                    if index >= MAX_CALLS || !self.calls.insert(id.clone()) {
                        return Err(ModelError::Protocol(
                            "Duplicate or excessive Gemini function calls",
                        ));
                    }
                    events.push(ModelEvent::ToolCallDelta {
                        index,
                        id: Some(id),
                        name: Some(name.to_owned()),
                        arguments: Some(args.to_string()),
                    });
                } else if let Some(text) = part.get("text") {
                    let text = text
                        .as_str()
                        .ok_or(ModelError::Protocol("Invalid Gemini text"))?;
                    if part.get("thought") != Some(&Value::Bool(true)) && !text.is_empty() {
                        events.push(ModelEvent::TextDelta(text.to_owned()));
                    }
                } else if part.get("thoughtSignature").is_none() {
                    return Err(ModelError::Protocol("Unsupported Gemini response part"));
                }
                if part
                    .get("thoughtSignature")
                    .is_some_and(|value| !value.is_string())
                {
                    return Err(ModelError::Protocol("Invalid Gemini thought signature"));
                }
                self.parts.push(part.clone());
            }
        }
        if let Some(reason) = candidate.get("finishReason") {
            let reason = reason
                .as_str()
                .ok_or(ModelError::Protocol("Invalid Gemini finish reason"))?;
            match reason {
                "STOP" => self.finished = true,
                "MAX_TOKENS" if self.calls.is_empty() => self.finished = true,
                "FINISH_REASON_UNSPECIFIED" | "" => {}
                _ => {
                    return Err(ModelError::Protocol(
                        "Gemini generation did not complete successfully",
                    ));
                }
            }
        }
        Ok(())
    }

    pub(crate) fn finish(&self) -> Result<Vec<ModelEvent>, ModelError> {
        if !self.finished {
            return Err(ModelError::Protocol(
                "Gemini stream ended before finishReason",
            ));
        }
        let count = |key: &str| {
            self.usage
                .get(key)
                .and_then(Value::as_u64)
                .unwrap_or_default()
        };
        let prompt = count("promptTokenCount");
        let completion = if self.usage.contains_key("totalTokenCount") {
            count("totalTokenCount")
                .checked_sub(prompt)
                .ok_or(ModelError::Protocol("Invalid Gemini total token count"))?
        } else {
            count("candidatesTokenCount").saturating_add(count("thoughtsTokenCount"))
        };
        let mut events = vec![ModelEvent::ProviderContent {
            protocol: PROTOCOL,
            content: self.parts.clone(),
        }];
        if !self.usage.is_empty() {
            events.push(ModelEvent::Usage(ModelUsage {
                prompt_tokens: prompt,
                completion_tokens: completion,
                cache_read_tokens: count("cachedContentTokenCount"),
                cache_write_tokens: 0,
                cache_eligible_input_tokens: prompt,
                cache_observed: self.usage.contains_key("cachedContentTokenCount"),
            }));
        }
        Ok(events)
    }
}

#[cfg(test)]
#[path = "model_gemini_tests.rs"]
mod tests;
