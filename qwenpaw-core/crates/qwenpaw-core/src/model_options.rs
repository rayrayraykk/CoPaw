use std::collections::BTreeMap;

use serde_json::Map;
use serde_json::Value;

use crate::model::ModelConfigError;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ModelProtocol {
    #[default]
    OpenAIChat,
    OpenAIResponses,
    AnthropicMessages,
    GeminiGenerateContent,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ModelAuthMode {
    #[default]
    ApiKey,
    BearerToken,
}

/// Provider-owned request settings, kept out of public configuration responses.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct ModelRequestOptions {
    pub provider_id: Option<String>,
    pub protocol: ModelProtocol,
    pub auth_mode: ModelAuthMode,
    pub custom_headers: BTreeMap<String, String>,
    pub generate_kwargs: Map<String, Value>,
    pub model_generate_kwargs: BTreeMap<String, Map<String, Value>>,
    pub map_openai_token_limit: bool,
}

impl std::fmt::Debug for ModelRequestOptions {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ModelRequestOptions")
            .field("header_count", &self.custom_headers.len())
            .field("parameter_count", &self.generate_kwargs.len())
            .field("model_count", &self.model_generate_kwargs.len())
            .finish_non_exhaustive()
    }
}

impl ModelRequestOptions {
    pub(crate) fn validate(&self) -> Result<(), ModelConfigError> {
        let invalid = ModelConfigError::InvalidRequestOptions;
        if self.provider_id.as_ref().is_some_and(|value| {
            value.is_empty() || value.len() > 256 || value.chars().any(char::is_control)
        }) {
            return Err(invalid);
        }
        if self.custom_headers.len() > 256 || self.model_generate_kwargs.len() > 4096 {
            return Err(invalid);
        }
        for (name, value) in &self.custom_headers {
            if name.len() > 256
                || value.len() > 16 * 1024
                || reqwest::header::HeaderName::from_bytes(name.as_bytes()).is_err()
                || reqwest::header::HeaderValue::from_str(value).is_err()
            {
                return Err(invalid);
            }
        }
        for parameters in
            std::iter::once(&self.generate_kwargs).chain(self.model_generate_kwargs.values())
        {
            let mut pending = parameters
                .values()
                .map(|value| (value, 0))
                .collect::<Vec<_>>();
            let mut count = 0;
            while let Some((value, depth)) = pending.pop() {
                count += 1;
                if depth > 16 || count > 4096 {
                    return Err(invalid);
                }
                match value {
                    Value::Object(values) => {
                        pending.extend(values.values().map(|value| (value, depth + 1)));
                    }
                    Value::Array(values) => {
                        pending.extend(values.iter().map(|value| (value, depth + 1)));
                    }
                    Value::String(value) if value.len() > 16 * 1024 => return Err(invalid),
                    _ => {}
                }
            }
            for values in std::iter::once(parameters)
                .chain(parameters.get("extra_body").and_then(Value::as_object))
            {
                if ["model", "messages", "stream", "stream_options", "tools"]
                    .iter()
                    .any(|key| values.contains_key(*key))
                {
                    return Err(invalid);
                }
                if self.protocol == ModelProtocol::GeminiGenerateContent
                    && [
                        "contents",
                        "systemInstruction",
                        "system_instruction",
                        "generationConfig",
                        "generation_config",
                    ]
                    .iter()
                    .any(|key| values.contains_key(*key))
                {
                    return Err(invalid);
                }
                if self.protocol == ModelProtocol::OpenAIResponses
                    && [
                        "input",
                        "instructions",
                        "previous_response_id",
                        "conversation",
                        "store",
                        "background",
                        "truncation",
                    ]
                    .iter()
                    .any(|key| values.contains_key(*key))
                {
                    return Err(invalid);
                }
            }
            if parameters
                .get("extra_body")
                .is_some_and(|value| !value.is_null() && !value.is_object())
            {
                return Err(invalid);
            }
        }
        Ok(())
    }

    pub(crate) fn generation_for(&self, model: &str) -> Map<String, Value> {
        let mut parameters = self.generate_kwargs.clone();
        if let Some(overrides) = self.model_generate_kwargs.get(model) {
            merge(&mut parameters, overrides);
        }
        if self.map_openai_token_limit {
            let name = model
                .rsplit('/')
                .next()
                .unwrap_or(model)
                .to_ascii_lowercase();
            if (name.starts_with("gpt-5")
                || (name.starts_with('o')
                    && name.as_bytes().get(1).is_some_and(u8::is_ascii_digit)))
                && let Some(limit) = parameters.remove("max_tokens")
                && !limit.is_null()
            {
                parameters.entry("max_completion_tokens").or_insert(limit);
            }
        }
        // These optional SDK parameters use omission rather than JSON null.
        if self.protocol == ModelProtocol::OpenAIChat {
            for key in ["max_tokens", "temperature", "top_p"] {
                if parameters.get(key).is_some_and(Value::is_null) {
                    parameters.remove(key);
                }
            }
        }
        if let Some(Value::Object(extra)) = parameters.remove("extra_body") {
            merge(&mut parameters, &extra);
        }
        parameters
    }
}

fn merge(base: &mut Map<String, Value>, overrides: &Map<String, Value>) {
    for (key, value) in overrides {
        if let Some(Value::Object(current)) = base.get_mut(key)
            && let Value::Object(overrides) = value
        {
            merge(current, overrides);
        } else {
            base.insert(key.clone(), value.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use serde_json::json;

    #[test]
    fn merges_model_overrides_without_mutation_and_maps_openai_token_limits() {
        let options = ModelRequestOptions {
            generate_kwargs: json!({"temperature": 0.6, "max_tokens": 64, "top_p": null,
                "extra_body": {"nested": {"base": true, "overridden": 1}}})
            .as_object()
            .unwrap()
            .clone(),
            model_generate_kwargs: BTreeMap::from([(
                String::from("vendor/gpt-5-test"),
                json!({"temperature": 0.1, "max_completion_tokens": 32,
                    "extra_body": {"nested": {"overridden": 2}}})
                .as_object()
                .unwrap()
                .clone(),
            )]),
            map_openai_token_limit: true,
            ..ModelRequestOptions::default()
        };
        options.validate().unwrap();
        let before = options.clone();
        assert_eq!(
            Value::Object(options.generation_for("vendor/gpt-5-test")),
            json!({"temperature": 0.1, "max_completion_tokens": 32,
                "nested": {"base": true, "overridden": 2}})
        );
        assert_eq!(
            Value::Object(options.generation_for("o3")),
            json!({"temperature": 0.6, "max_completion_tokens": 64,
                "nested": {"base": true, "overridden": 1}})
        );
        assert_eq!(
            Value::Object(options.generation_for("local-model")),
            json!({"temperature": 0.6, "max_tokens": 64,
                "nested": {"base": true, "overridden": 1}})
        );
        assert_eq!(options, before);
    }

    #[test]
    fn rejects_protocol_overrides_invalid_headers_and_unbounded_options() {
        for value in [
            json!({"stream": false}),
            json!({"extra_body": {"model": "override"}}),
            json!({"extra_body": "invalid"}),
            json!({"tools": []}),
        ] {
            let options = ModelRequestOptions {
                generate_kwargs: value.as_object().unwrap().clone(),
                ..Default::default()
            };
            assert_eq!(
                options.validate(),
                Err(ModelConfigError::InvalidRequestOptions)
            );
        }
        let mut options = ModelRequestOptions::default();
        options
            .custom_headers
            .insert(String::from("x-secret"), String::from("private\ninjection"));
        assert_eq!(
            options.validate(),
            Err(ModelConfigError::InvalidRequestOptions)
        );
        assert!(!format!("{options:?}").contains("private"));
        options.custom_headers.clear();
        let mut value = Value::Null;
        for _ in 0..18 {
            value = json!({"nested": value});
        }
        options
            .generate_kwargs
            .insert(String::from("extra_body"), value);
        assert_eq!(
            options.validate(),
            Err(ModelConfigError::InvalidRequestOptions)
        );
    }
}
