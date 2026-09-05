//! Read-only compatibility contracts for unchanged Console navigation pages.

use axum::Json;
use axum::Router;
use axum::routing::get;
use serde_json::Value;
use serde_json::json;

use super::AppServer;

pub(super) fn router() -> Router<AppServer> {
    Router::new()
        .route("/api/pawapps", get(pawapps))
        .route("/api/config/acp", get(acp))
        .route("/api/console/debug/backend-logs", get(backend_logs))
        .route("/api/backups", get(empty_array))
        .route("/api/backups/jobs/active", get(empty_value))
}

async fn empty_array() -> Json<Value> {
    Json(json!([]))
}

async fn empty_value() -> Json<Value> {
    Json(Value::Null)
}

async fn pawapps() -> Json<Value> {
    Json(json!({"apps": [], "total": 0}))
}

async fn acp() -> Json<Value> {
    Json(json!({"agents": {}}))
}

async fn backend_logs() -> Json<Value> {
    Json(json!({
        "path": "",
        "exists": false,
        "lines": 0,
        "updated_at": null,
        "size": 0,
        "content": ""
    }))
}
