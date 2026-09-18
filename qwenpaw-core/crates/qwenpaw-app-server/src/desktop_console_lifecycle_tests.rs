//! Ordinary Console admission, per-turn settings and shutdown fences.

use super::approvals::{agent_chat, start_chat, wait_pending};
use super::scope::scoped;
use super::*;
use pretty_assertions::assert_eq;

#[path = "desktop_console_lifecycle_browser_tests.rs"]
mod browser;

async fn writer(fixture: &Fixture) {
    fixture
        .request(
            "POST",
            "/api/agents",
            json!({"id":"writer","name":"Writer"}),
        )
        .await;
}

async fn terminal(body: Body, status: &str) {
    let bytes = tokio::time::timeout(
        Duration::from_secs(5),
        axum::body::to_bytes(body, 1024 * 1024),
    )
    .await
    .unwrap()
    .unwrap();
    let text = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(text.contains(&format!("\"status\":\"{status}\"")), "{text}");
}

async fn assert_closed(fixture: &Fixture, id: &str) {
    let thread = fixture.server.inner.core.read_thread(id).await.unwrap();
    assert_eq!(thread.turns.last().unwrap().status, TurnStatus::Interrupted);
    assert_eq!(thread.thread.status, qwenpaw_protocol::ThreadStatus::Idle);
    assert!(
        !fixture
            .directory
            .path()
            .join("data/workspaces/writer/cron-output.txt")
            .exists()
    );
}

async fn stop_pending(delete: bool) {
    let fixture = Fixture::new().await;
    writer(&fixture).await;
    let default = agent_chat(&fixture, "default").await;
    let id = agent_chat(&fixture, "writer").await;
    let default_body = start_chat(&fixture, "default", &default).await;
    let body = start_chat(&fixture, "writer", &id).await;
    let pending = wait_pending(&fixture, 2).await;
    let untouched = fixture
        .server
        .inner
        .core
        .read_thread(&default)
        .await
        .unwrap();
    let expected = pending["pending_approvals"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|approval| approval["agent_id"] == "default")
        .cloned()
        .collect::<Vec<_>>();
    let path = if delete {
        "/api/agents/writer"
    } else {
        "/api/agents/writer/toggle"
    };
    let response = fixture
        .request(
            if delete { "DELETE" } else { "PATCH" },
            path,
            json!({"enabled":false}),
        )
        .await;
    assert_eq!(
        response,
        if delete {
            json!({"success":true,"agent_id":"writer"})
        } else {
            json!({"success":true,"agent_id":"writer","enabled":false})
        }
    );
    // The successful lifecycle response itself is the completion fence.
    assert_closed(&fixture, &id).await;
    assert_eq!(
        fixture
            .server
            .inner
            .core
            .read_thread(&default)
            .await
            .unwrap(),
        untouched
    );
    assert_eq!(
        fixture
            .request("GET", "/api/console/push-messages", Value::Null)
            .await,
        json!({"messages":[],"pending_approvals":expected})
    );
    terminal(body, "canceled").await;
    resume_after_stop(&fixture, &id, &default, default_body, delete).await;
}

async fn resume_after_stop(
    fixture: &Fixture,
    id: &str,
    default: &str,
    default_body: Body,
    delete: bool,
) {
    let before = fixture
        .server
        .inner
        .core
        .backup_snapshot(8 * 1024 * 1024)
        .unwrap();
    assert_eq!(
        scoped(
            fixture,
            "writer",
            "POST",
            "/api/console/chat",
            json!({"session_id":id,"input":[{"role":"user","content":"write fixture"}]})
        )
        .await,
        if delete {
            (
                StatusCode::NOT_FOUND,
                json!({"detail":"Agent 'writer' not found"}),
            )
        } else {
            (
                StatusCode::FORBIDDEN,
                json!({"detail":"Agent 'writer' is disabled"}),
            )
        }
    );
    assert_eq!(
        fixture
            .server
            .inner
            .core
            .backup_snapshot(8 * 1024 * 1024)
            .unwrap(),
        before
    );
    if delete {
        fixture
            .request(
                "POST",
                "/api/agents",
                json!({
                    "id":"writer","name":"New Writer",
                    "workspace_dir":fixture.directory.path().join("new-writer")
                }),
            )
            .await;
        assert_eq!(
            scoped(
                fixture,
                "writer",
                "GET",
                &format!("/api/chats/{id}"),
                Value::Null
            )
            .await
            .0,
            StatusCode::NOT_FOUND
        );
    } else {
        fixture
            .request(
                "PATCH",
                "/api/agents/writer/toggle",
                json!({"enabled":true}),
            )
            .await;
    }
    let next = agent_chat(fixture, "writer").await;
    let next_body = start_chat(fixture, "writer", &next).await;
    wait_pending(fixture, 2).await;
    assert_eq!(
        scoped(
            fixture,
            "writer",
            "POST",
            &format!("/api/console/chat/stop?chat_id={next}"),
            Value::Null
        )
        .await,
        (StatusCode::OK, json!({"stopped":true}))
    );
    terminal(next_body, "canceled").await;
    fixture
        .request(
            "POST",
            &format!("/api/console/chat/stop?chat_id={default}"),
            Value::Null,
        )
        .await;
    terminal(default_body, "canceled").await;
    assert_eq!(
        wait_pending(fixture, 0).await,
        json!({"messages":[],"pending_approvals":[]})
    );
}

#[tokio::test]
async fn disabling_console_agent_drains_approval_without_touching_default() {
    stop_pending(false).await;
}

#[tokio::test]
async fn deleting_console_agent_drains_before_reusing_its_name() {
    stop_pending(true).await;
}

async fn held(fixture: &Fixture, id: &str) -> Body {
    let response = fixture
        .server
        .clone()
        .router()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/console/chat")
                .header("x-agent-id", "writer")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "session_id":id,"input":[{"role":"user","content":"hold"}]
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    tokio::time::timeout(Duration::from_secs(5), async {
        while fixture.remote.requests.lock().unwrap().is_empty() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    response.into_body()
}

#[tokio::test]
async fn deleting_console_agent_interrupts_pending_model_headers() {
    let fixture = Fixture::new().await;
    writer(&fixture).await;
    let id = agent_chat(&fixture, "writer").await;
    let body = held(&fixture, &id).await;
    fixture
        .request("DELETE", "/api/agents/writer", Value::Null)
        .await;
    assert_closed(&fixture, &id).await;
    terminal(body, "canceled").await;
    assert_eq!(
        wait_pending(&fixture, 0).await,
        json!({"messages":[],"pending_approvals":[]})
    );
}

#[tokio::test]
async fn disconnected_console_peer_cancels_model_without_another_event() {
    let fixture = Fixture::new().await;
    writer(&fixture).await;
    let id = agent_chat(&fixture, "writer").await;
    drop(held(&fixture, &id).await);
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let thread = fixture.server.inner.core.read_thread(&id).await.unwrap();
            if thread.thread.status == qwenpaw_protocol::ThreadStatus::Idle {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_closed(&fixture, &id).await;
    let completed = crate::desktop_console_runs::cancel_agent(&fixture.server, "writer");
    crate::desktop_console_runs::drain_runs(completed)
        .await
        .unwrap();
    assert_eq!(
        wait_pending(&fixture, 0).await,
        json!({"messages":[],"pending_approvals":[]})
    );
}

#[tokio::test]
async fn console_running_policy_is_private_to_the_selected_agent() {
    let fixture = Fixture::new().await;
    writer(&fixture).await;
    assert_eq!(
        scoped(
            &fixture,
            "writer",
            "PUT",
            "/api/workspace/running-config",
            json!({"approval_level":"OFF"})
        )
        .await
        .0,
        StatusCode::OK
    );
    let default = agent_chat(&fixture, "default").await;
    let id = agent_chat(&fixture, "writer").await;
    let default_body = start_chat(&fixture, "default", &default).await;
    let writer_body = start_chat(&fixture, "writer", &id).await;
    terminal(writer_body, "completed").await;
    let pending = wait_pending(&fixture, 1).await;
    assert_eq!(pending["pending_approvals"][0]["agent_id"], "default");
    assert_eq!(
        std::fs::read_to_string(
            fixture
                .directory
                .path()
                .join("data/workspaces/writer/cron-output.txt")
        )
        .unwrap(),
        "created by Cron"
    );
    assert!(
        !fixture
            .directory
            .path()
            .join("workspace/cron-output.txt")
            .exists()
    );
    fixture
        .request(
            "POST",
            &format!("/api/console/chat/stop?chat_id={default}"),
            Value::Null,
        )
        .await;
    terminal(default_body, "canceled").await;
}

#[tokio::test]
async fn console_without_running_profile_preserves_effective_global_runtime() {
    let fixture = Fixture::new().await;
    writer(&fixture).await;
    let catalog_path = fixture.directory.path().join("data/agents/catalog.json");
    let mut catalog: Value =
        serde_json::from_slice(&std::fs::read(&catalog_path).unwrap()).unwrap();
    catalog["agents"]["writer"]["config"]
        .as_object_mut()
        .unwrap()
        .remove("running");
    std::fs::write(&catalog_path, serde_json::to_vec(&catalog).unwrap()).unwrap();
    let mut runtime = fixture.server.inner.core.agent_runtime_config().unwrap();
    runtime.approval_level = qwenpaw_core::ToolApprovalLevel::Off;
    fixture
        .server
        .inner
        .core
        .replace_agent_runtime_config(runtime.clone())
        .unwrap();
    let id = agent_chat(&fixture, "writer").await;
    terminal(start_chat(&fixture, "writer", &id).await, "completed").await;
    assert_eq!(
        fixture.server.inner.core.agent_runtime_config().unwrap(),
        runtime
    );
    assert_eq!(
        std::fs::read_to_string(
            fixture
                .directory
                .path()
                .join("data/workspaces/writer/cron-output.txt")
        )
        .unwrap(),
        "created by Cron"
    );
    assert_eq!(
        wait_pending(&fixture, 0).await,
        json!({"messages":[],"pending_approvals":[]})
    );
}
