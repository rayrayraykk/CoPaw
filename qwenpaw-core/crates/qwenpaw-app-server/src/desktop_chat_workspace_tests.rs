//! Console lifecycle tests against real native registration and storage.

use super::approvals::{agent_chat, start_chat, wait_pending};
use super::scope::scoped;
use super::*;
use crate::desktop_chats;
use pretty_assertions::assert_eq;

async fn writer(fixture: &Fixture) {
    fixture
        .request(
            "POST",
            "/api/agents",
            json!({"id":"writer","name":"Writer"}),
        )
        .await;
}

#[tokio::test]
#[ignore = "requires console/dist, Node 24+ and Chrome; run explicitly for browser acceptance"]
async fn original_chat_group_controls_preserve_other_workspace_data() {
    let mut fixture = Fixture::new().await;
    writer(&fixture).await;
    agent_chat(&fixture, "default").await;
    agent_chat(&fixture, "writer").await;
    fixture.server.inner.core.write_ui_language("en").unwrap();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    fixture.server.inner.shutdown.cancel();
    shutdown(&fixture.server).await;
    fixture.server = AppServer::new_desktop_with_stores_and_workspace(
        fixture.server.inner.core.clone(),
        &root.join("../console/dist"),
        String::from("chat-groups-fixture"),
        Arc::new(Credentials),
        &fixture.directory.path().join("data"),
        &fixture.directory.path().join("workspace"),
    )
    .unwrap();
    let before: Value = serde_json::from_str(
        &fixture
            .server
            .inner
            .core
            .read_chat_catalog_data()
            .unwrap()
            .unwrap(),
    )
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(fixture.server.clone().run_http(listener));
    let result = tokio::time::timeout(
        Duration::from_secs(90),
        tokio::process::Command::new("node")
            .arg(root.join("scripts/console_browser_smoke.mjs"))
            .args([&origin, "/chat", "--chat-groups-crud"])
            .kill_on_drop(true)
            .output(),
    )
    .await;
    fixture.server.inner.shutdown.cancel();
    tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let output = result.unwrap().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let report: Value =
        serde_json::from_str(&stdout).unwrap_or_else(|_| panic!("{stdout}\n{stderr}"));
    assert!(output.status.success(), "{report:#}\n{stderr}");
    assert_eq!(
        report["pages"][0]["chatGroupsCrud"],
        json!({"created":true,"renamed":true,"pinned":true,"reload":true,"deleted":true})
    );
    assert_eq!(
        serde_json::from_str::<Value>(
            &fixture
                .server
                .inner
                .core
                .read_chat_catalog_data()
                .unwrap()
                .unwrap()
        )
        .unwrap(),
        before
    );
}

async fn list(fixture: &Fixture, actor: &str, path: &str) -> Value {
    let (status, body) = scoped(fixture, actor, "GET", path, Value::Null).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body
}

#[tokio::test]
async fn failed_console_catalog_publication_rolls_back_thread_without_publishing_alias() {
    let fixture = Fixture::new().await;
    writer(&fixture).await;
    agent_chat(&fixture, "default").await;
    let before = fixture
        .server
        .inner
        .core
        .backup_snapshot(8 * 1024 * 1024)
        .unwrap();
    let connection =
        rusqlite::Connection::open(fixture.directory.path().join("core.sqlite")).unwrap();
    connection.execute_batch("CREATE TRIGGER fail_chat_catalog BEFORE INSERT ON core_settings WHEN NEW.key = 'desktop_chat_catalog_data' BEGIN SELECT RAISE(ABORT, 'fixture chat publication failure'); END;").unwrap();
    let error = crate::desktop_api::resolve_console_thread(
        &fixture.server,
        "writer",
        Some("1700000000001-new"),
        None,
    )
    .await
    .unwrap_err();
    assert_eq!(error.0, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(
        fixture
            .server
            .inner
            .core
            .backup_snapshot(8 * 1024 * 1024)
            .unwrap(),
        before
    );
    assert_eq!(
        desktop_chats::resolve_existing_thread(&fixture.server, "writer", "1700000000001-new")
            .await
            .unwrap(),
        None
    );
}

#[tokio::test]
async fn durable_alias_selects_latest_matching_chat_after_reopen() {
    let mut fixture = Fixture::new().await;
    let first = agent_chat(&fixture, "default").await;
    let second = agent_chat(&fixture, "default").await;
    let core = &fixture.server.inner.core;
    let mut catalog: Value =
        serde_json::from_str(&core.read_chat_catalog_data().unwrap().unwrap()).unwrap();
    catalog["chats"][&first]["updated_at"] = json!(200);
    catalog["chats"][&second]["updated_at"] = json!(100);
    core.write_chat_catalog_data(&catalog.to_string()).unwrap();
    // The cache still points at the second chat. Durable ordering wins.
    assert_eq!(
        desktop_chats::resolve_existing_thread(&fixture.server, "default", "same-session")
            .await
            .unwrap(),
        Some(first.clone())
    );
    fixture.reopen().await;
    assert_eq!(
        desktop_chats::resolve_existing_thread(&fixture.server, "default", "same-session")
            .await
            .unwrap(),
        Some(first)
    );
}

async fn assert_sdk_project_does_not_assign_owner(
    fixture: &Fixture,
    root: &std::path::Path,
    chats: &Value,
) {
    let sdk = fixture
        .server
        .inner
        .core
        .start_thread(qwenpaw_protocol::ThreadStartParams {
            workspace_root: Some(root.to_string_lossy().into_owned()),
            model: None,
        })
        .await
        .unwrap()
        .thread;
    assert_eq!(&list(fixture, "editor", "/api/chats").await, chats);
    let default = list(fixture, "default", "/api/chats").await;
    assert_eq!(
        default
            .as_array()
            .unwrap()
            .iter()
            .map(|chat| chat["id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec![sdk.id.as_str()]
    );
}

#[tokio::test]
async fn chats_groups_and_aliases_follow_retained_root_not_reused_actor_or_project() {
    let mut fixture = Fixture::new().await;
    writer(&fixture).await;
    let id = agent_chat(&fixture, "writer").await;
    let (_, group) = scoped(
        &fixture,
        "writer",
        "POST",
        "/api/chats/groups",
        json!({"name":"Keep group"}),
    )
    .await;
    let group_id = group["id"].as_str().unwrap();
    let (status, _) = scoped(
        &fixture,
        "writer",
        "PUT",
        &format!("/api/chats/{id}"),
        json!({"group_id":group_id,"pinned":true}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let chats = list(&fixture, "writer", "/api/chats").await;
    let groups = list(&fixture, "writer", "/api/chats/groups").await;
    let root = fixture.directory.path().join("data/workspaces/writer");
    let key = fixture.data_key("writer");
    fixture
        .request("DELETE", "/api/agents/writer", Value::Null)
        .await;
    fixture
        .request(
            "POST",
            "/api/agents",
            json!({"id":"editor","name":"Editor","workspace_dir":root}),
        )
        .await;
    fixture.request("POST", "/api/agents", json!({"id":"writer","name":"New Writer","workspace_dir":fixture.directory.path().join("new-writer")})).await;
    fixture.reopen().await;
    assert_eq!(fixture.data_key("editor"), key);
    assert_ne!(fixture.data_key("writer"), key);
    assert_eq!(list(&fixture, "editor", "/api/chats").await, chats);
    assert_eq!(list(&fixture, "editor", "/api/chats/groups").await, groups);
    assert_eq!(list(&fixture, "writer", "/api/chats").await, json!([]));
    assert_eq!(
        desktop_chats::resolve_existing_thread(&fixture.server, "editor", "same-session")
            .await
            .unwrap(),
        Some(id.clone())
    );
    assert_eq!(
        desktop_chats::resolve_existing_thread(&fixture.server, "writer", "same-session")
            .await
            .unwrap(),
        None
    );
    let identity = desktop_chats::approval_session_info(&fixture.server, &id)
        .await
        .unwrap();
    assert_eq!(
        (identity.agent, identity.session, identity.root_session),
        (
            String::from("editor"),
            String::from("same-session"),
            String::from("writer-root")
        )
    );
    let cron = desktop_chats::resolve_cron_chat(
        &fixture.server,
        "editor",
        "same-session",
        "admin",
        "Do not rename",
    )
    .await
    .unwrap();
    assert_eq!(cron.id, id);
    assert_eq!(list(&fixture, "editor", "/api/chats").await, chats);
    for method in ["GET", "PUT", "DELETE"] {
        let (status, _) = scoped(
            &fixture,
            "writer",
            method,
            &format!("/api/chats/{id}"),
            json!({"name":"forged"}),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{method}");
    }
    let (status, _) = scoped(
        &fixture,
        "writer",
        "PUT",
        &format!("/api/chats/groups/{group_id}"),
        json!({"name":"forged"}),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(list(&fixture, "editor", "/api/chats/groups").await, groups);
    assert_sdk_project_does_not_assign_owner(&fixture, &root, &chats).await;
}

#[tokio::test]
async fn another_workspace_cannot_stop_or_read_the_project_of_a_live_chat() {
    let fixture = Fixture::new().await;
    writer(&fixture).await;
    let id = agent_chat(&fixture, "writer").await;
    let body = start_chat(&fixture, "writer", &id).await;
    let pending = wait_pending(&fixture, 1).await;
    let snapshot = fixture
        .server
        .inner
        .core
        .backup_snapshot(8 * 1024 * 1024)
        .unwrap();
    for requested in [id.as_str(), "same-session"] {
        assert_eq!(
            scoped(
                &fixture,
                "default",
                "POST",
                &format!("/api/console/chat/stop?chat_id={requested}"),
                Value::Null
            )
            .await,
            (StatusCode::OK, json!({"stopped":false}))
        );
    }
    let headers = axum::http::HeaderMap::from_iter([
        (
            axum::http::HeaderName::from_static("x-agent-id"),
            "default".parse().unwrap(),
        ),
        (
            axum::http::HeaderName::from_static("x-chat-id"),
            id.parse().unwrap(),
        ),
    ]);
    assert_eq!(
        crate::desktop_files::resolve_workspace_root(&fixture.server, &headers, Some("project"))
            .await
            .unwrap_err()
            .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(wait_pending(&fixture, 1).await, pending);
    assert_eq!(
        fixture
            .server
            .inner
            .core
            .backup_snapshot(8 * 1024 * 1024)
            .unwrap(),
        snapshot
    );
    assert_eq!(
        scoped(
            &fixture,
            "writer",
            "POST",
            "/api/console/chat/stop?chat_id=same-session",
            Value::Null
        )
        .await,
        (StatusCode::OK, json!({"stopped":true}))
    );
    let bytes = tokio::time::timeout(
        Duration::from_secs(5),
        axum::body::to_bytes(body, 1024 * 1024),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(
        String::from_utf8(bytes.to_vec())
            .unwrap()
            .contains("\"status\":\"canceled\"")
    );
    wait_pending(&fixture, 0).await;
}

#[tokio::test]
async fn legacy_chat_catalog_does_not_authorize_a_new_same_name_workspace() {
    let mut fixture = Fixture::new().await;
    writer(&fixture).await;
    let writer_id = agent_chat(&fixture, "writer").await;
    let default_id = agent_chat(&fixture, "default").await;
    let core = &fixture.server.inner.core;
    let mut legacy: Value =
        serde_json::from_str(&core.read_chat_catalog_data().unwrap().unwrap()).unwrap();
    legacy["version"] = json!(1);
    for chat in legacy["chats"].as_object_mut().unwrap().values_mut() {
        chat.as_object_mut().unwrap().remove("data_key");
    }
    for group in legacy["groups"].as_array_mut().unwrap() {
        group.as_object_mut().unwrap().remove("data_key");
    }
    let original = legacy.to_string();
    core.write_chat_catalog_data(&original).unwrap();
    fixture.reopen().await;
    assert_eq!(list(&fixture, "writer", "/api/chats").await, json!([]));
    assert_eq!(
        desktop_chats::resolve_existing_thread(&fixture.server, "writer", &writer_id)
            .await
            .unwrap_err()
            .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        fixture.server.inner.core.read_chat_catalog_data().unwrap(),
        Some(original)
    );
    assert_eq!(
        desktop_chats::resolve_existing_thread(&fixture.server, "default", "same-session")
            .await
            .unwrap(),
        Some(default_id)
    );
    let (status, _) = scoped(
        &fixture,
        "default",
        "POST",
        "/api/chats/groups",
        json!({"name":"publish native upgrade"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let stored: Value = serde_json::from_str(
        &fixture
            .server
            .inner
            .core
            .read_chat_catalog_data()
            .unwrap()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(stored["version"], 2);
    assert_eq!(
        stored["chats"][writer_id]["data_key"],
        json!({"kind":"legacy_agent","id":"writer"})
    );
}
