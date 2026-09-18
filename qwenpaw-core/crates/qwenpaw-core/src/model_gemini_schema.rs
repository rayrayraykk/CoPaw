use serde_json::{Value, json};

use crate::model::ModelError;

pub(super) fn normalize(schema: &Value) -> Result<Value, ModelError> {
    walk(
        schema,
        schema.get("$defs").unwrap_or(&Value::Null),
        &[],
        0,
        &mut 16_384,
    )
}

fn walk(
    value: &Value,
    definitions: &Value,
    visited: &[String],
    depth: usize,
    remaining: &mut usize,
) -> Result<Value, ModelError> {
    *remaining = remaining.checked_sub(1).ok_or(ModelError::Protocol(
        "Gemini tool schema exceeds expansion limit",
    ))?;
    if depth > 32 {
        return Err(ModelError::Protocol(
            "Gemini tool schema exceeds depth limit",
        ));
    }
    let Some(input) = value.as_object() else {
        return Ok(value.clone());
    };
    let mut schema = input.clone();
    schema.remove("$defs");
    schema.remove("$schema");
    schema.remove("additionalProperties");
    if let Some(reference) = schema.remove("$ref") {
        let name = reference
            .as_str()
            .and_then(|value| value.strip_prefix("#/$defs/"))
            .ok_or(ModelError::Protocol("Unsupported Gemini schema reference"))?;
        let target = definitions
            .get(name)
            .ok_or(ModelError::Protocol("Unresolved Gemini schema reference"))?;
        let mut path = visited.to_vec();
        let resolved = if path.iter().any(|item| item == name) {
            json!({"type": "OBJECT", "description": format!("(circular: {name})")})
        } else {
            path.push(name.to_owned());
            walk(target, definitions, &path, depth + 1, remaining)?
        };
        let mut resolved = resolved
            .as_object()
            .cloned()
            .ok_or(ModelError::Protocol("Invalid Gemini schema reference"))?;
        resolved.extend(schema);
        schema = resolved;
    }
    if let Some(constant) = schema.remove("const") {
        schema.entry("enum").or_insert(json!([constant]));
    }
    if let Some(choices) = schema.get("anyOf").and_then(Value::as_array) {
        let filtered = choices
            .iter()
            .filter(|choice| choice["type"] != "null")
            .cloned()
            .collect::<Vec<_>>();
        if filtered.len() != choices.len() {
            schema.remove("anyOf");
            if filtered.len() == 1 {
                let mut merged = filtered[0]
                    .as_object()
                    .cloned()
                    .ok_or(ModelError::Protocol("Invalid Gemini nullable schema"))?;
                for (key, value) in schema {
                    merged.entry(key).or_insert(value);
                }
                return walk(
                    &Value::Object(merged),
                    definitions,
                    visited,
                    depth + 1,
                    remaining,
                );
            } else if !filtered.is_empty() {
                schema.insert(String::from("anyOf"), json!(filtered));
            }
        }
    }
    if let Some(kind) = schema.get("type").and_then(Value::as_str) {
        schema.insert(
            String::from("type"),
            json!(if kind == "null" {
                String::from("OBJECT")
            } else {
                kind.to_ascii_uppercase()
            }),
        );
    }
    for key in ["properties", "patternProperties"] {
        if let Some(values) = schema.get_mut(key).and_then(Value::as_object_mut) {
            for value in values.values_mut() {
                *value = walk(value, definitions, visited, depth + 1, remaining)?;
            }
        }
    }
    for key in ["items", "not", "if", "then", "else"] {
        if let Some(value) = schema.get_mut(key) {
            *value = walk(value, definitions, visited, depth + 1, remaining)?;
        }
    }
    for key in ["anyOf", "oneOf", "allOf"] {
        if let Some(values) = schema.get_mut(key).and_then(Value::as_array_mut) {
            for value in values {
                *value = walk(value, definitions, visited, depth + 1, remaining)?;
            }
        }
    }
    Ok(Value::Object(schema))
}
