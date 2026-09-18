//! Original plugin market query contract, without installation side effects.

use std::collections::BTreeMap;

use axum::Json;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use serde_json::{Value, json};

use super::{ApiError, AppServer};

pub(super) async fn search(
    State(server): State<AppServer>,
    Query(query): Query<BTreeMap<String, String>>,
) -> Result<Json<Value>, ApiError> {
    let parameters = parameters(&query)?;
    let failed = |message: String| {
        (
            StatusCode::BAD_GATEWAY,
            Json(json!({"detail":format!("Failed to fetch from plugin market: {message}")})),
        )
    };
    let mut url = super::endpoint(&server.inner.desktop_market.qwenpaw, "/openapi/v1/plugins")
        .map_err(failed)?;
    url.query_pairs_mut().extend_pairs(parameters);
    super::fetch(url, HeaderMap::new())
        .await
        .map(Json)
        .map_err(failed)
}

fn parameters(query: &BTreeMap<String, String>) -> Result<Vec<(String, String)>, ApiError> {
    let mut values = Vec::new();
    let mut errors = Vec::new();
    for (key, fallback) in [("page_number", "1"), ("page_size", "20")] {
        let raw = query.get(key).map_or(fallback, String::as_str);
        if let Some(value) = integer(raw) {
            values.push((key.to_owned(), value));
        } else {
            errors.push(json!({"type":"int_parsing","loc":["query",key],
                "msg":"Input should be a valid integer, unable to parse string as an integer",
                "input":raw}));
        }
    }
    for key in ["search", "category", "sort_by"] {
        if let Some(value) = query.get(key).filter(|value| !value.is_empty()) {
            values.push((key.to_owned(), value.clone()));
        }
    }
    for key in ["is_featured", "is_trending"] {
        if let Some(raw) = query.get(key) {
            let value = match raw.to_ascii_lowercase().as_str() {
                "1" | "true" | "t" | "yes" | "y" | "on" => Some("true"),
                "0" | "false" | "f" | "no" | "n" | "off" => Some("false"),
                _ => None,
            };
            if let Some(value) = value {
                values.push((key.to_owned(), value.to_owned()));
            } else {
                errors.push(json!({"type":"bool_parsing","loc":["query",key],
                    "msg":"Input should be a valid boolean, unable to interpret input",
                    "input":raw}));
            }
        }
    }
    if errors.is_empty() {
        Ok(values)
    } else {
        Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({"detail":errors})),
        ))
    }
}

// Keep integer text exact: Python query integers are not limited to i64.
fn integer(raw: &str) -> Option<String> {
    let raw = raw.trim();
    let negative = raw.starts_with('-');
    let unsigned = raw.strip_prefix(['-', '+']).unwrap_or(raw);
    let digits = if let Some((digits, fraction)) = unsigned.split_once('.') {
        if fraction.is_empty() || !fraction.bytes().all(|byte| byte == b'0') {
            return None;
        }
        digits
    } else {
        unsigned
    };
    let mut value = String::new();
    let mut previous_digit = false;
    for byte in digits.bytes() {
        if byte.is_ascii_digit() {
            value.push(char::from(byte));
            previous_digit = true;
        } else if byte == b'_' && previous_digit {
            previous_digit = false;
        } else {
            return None;
        }
    }
    if !previous_digit {
        return None;
    }
    let value = value.trim_start_matches('0');
    Some(if value.is_empty() {
        String::from("0")
    } else if negative {
        format!("-{value}")
    } else {
        value.to_owned()
    })
}
