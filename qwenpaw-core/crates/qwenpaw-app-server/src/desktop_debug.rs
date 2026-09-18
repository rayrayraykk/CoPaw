//! Actual backend log snapshots for the unchanged Console Debug page.

use axum::Json;
use axum::Router;
use axum::extract::{RawQuery, State};
use axum::http::StatusCode;
use axum::routing::get;
use serde_json::Value;
use serde_json::json;

use super::AppServer;

pub(super) fn router() -> Router<AppServer> {
    Router::new().route("/api/console/debug/backend-logs", get(backend_logs))
}

async fn backend_logs(
    State(server): State<AppServer>,
    RawQuery(query): RawQuery,
) -> Result<Json<Value>, super::desktop_files::ApiError> {
    let raw = url::form_urlencoded::parse(query.as_deref().unwrap_or_default().as_bytes())
        .filter(|(key, _)| key == "lines")
        .map(|(_, value)| value.into_owned())
        .last();
    let lines = match raw {
        None => 200,
        Some(value) => match parse_integer(&value) {
            Some(lines) if (20..=1000).contains(&lines) => usize::try_from(lines).unwrap(),
            parsed => return Err(validation_error(&value, parsed)),
        },
    };
    let root = server
        .inner
        .desktop_workspace
        .as_ref()
        .ok_or_else(|| {
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({"detail":"Workspace is unavailable"})),
            )
        })?
        .data_dir
        .clone();
    tokio::task::spawn_blocking(move || super::backend_log::snapshot(&root, lines))
        .await
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"detail":"Backend log lookup failed"})),
            )
        })?
        .map(Json)
        .map_err(|error| {
            (
                if error.kind() == std::io::ErrorKind::PermissionDenied {
                    StatusCode::FORBIDDEN
                } else {
                    StatusCode::INTERNAL_SERVER_ERROR
                },
                Json(json!({"detail":"Backend log file could not be read"})),
            )
        })
}

// Match the original query coercion without floating-point rounding. Saturating
// arithmetic preserves the appropriate range error for integers beyond i64.
fn parse_integer(raw: &str) -> Option<i64> {
    let raw = raw.trim();
    let negative = raw.starts_with('-');
    let unsigned = raw.strip_prefix(['-', '+']).unwrap_or(raw);
    let integer = if let Some((integer, fraction)) = unsigned.split_once('.') {
        if fraction.is_empty() || !fraction.bytes().all(|byte| byte == b'0') {
            return None;
        }
        integer
    } else {
        unsigned
    };
    let mut number = 0_i64;
    let mut previous_digit = false;
    for byte in integer.bytes() {
        if byte.is_ascii_digit() {
            number = number
                .saturating_mul(10)
                .saturating_add(i64::from(byte - b'0'));
            previous_digit = true;
        } else if byte == b'_' && previous_digit {
            previous_digit = false;
        } else {
            return None;
        }
    }
    previous_digit.then_some(if negative { -number } else { number })
}

fn validation_error(raw: &str, value: Option<i64>) -> super::desktop_files::ApiError {
    let mut detail = match value {
        Some(value) if value < 20 => json!({"type":"greater_than_equal",
            "msg":"Input should be greater than or equal to 20","ctx":{"ge":20}}),
        Some(_) => json!({"type":"less_than_equal",
            "msg":"Input should be less than or equal to 1000","ctx":{"le":1000}}),
        None => json!({"type":"int_parsing",
            "msg":"Input should be a valid integer, unable to parse string as an integer"}),
    };
    detail["loc"] = json!(["query", "lines"]);
    detail["input"] = json!(raw);
    (
        StatusCode::UNPROCESSABLE_ENTITY,
        Json(json!({"detail":[detail]})),
    )
}
