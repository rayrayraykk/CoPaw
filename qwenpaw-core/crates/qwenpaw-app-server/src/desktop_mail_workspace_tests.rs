//! Mail permissions keep both Workspace ownership and the original Agent namespace.

use super::*;
use pretty_assertions::assert_eq;

#[path = "desktop_mail_browser_tests.rs"]
mod browser;

fn mail(mode: &str) -> Value {
    json!({"push":{"mode":mode,"access_control_enabled":true}})
}

fn acl(address: &str) -> Value {
    let whitelist = if address.is_empty() {
        json!({})
    } else {
        json!({address:{"remark":"","display_name":""}})
    };
    json!({"whitelist":whitelist,"blacklist":{},"pending":[],"approved_replay":[]})
}

async fn actor(fixture: &Fixture, id: &str, mode: &str) {
    fixture
        .request(
            "POST",
            "/api/agents",
            json!({"id":id,"name":id,"mail":mail(mode)}),
        )
        .await;
}

async fn action(fixture: &Fixture, kind: &str, entries: Value) -> Value {
    fixture
        .request(
            "POST",
            &format!("/api/mail-access-control/{kind}"),
            json!({"entries":entries}),
        )
        .await
}

fn stored(fixture: &Fixture) -> Value {
    serde_json::from_str(
        &fixture
            .server
            .inner
            .core
            .read_mail_access_control_data()
            .unwrap()
            .unwrap(),
    )
    .unwrap()
}

fn pending(agent: &str, timestamp: u32) -> Value {
    json!({"sender_address":"same@example.com","agent_id":agent,
        "display_name":agent,"subject":"Review","body_preview":"Preview",
        "timestamp":f64::from(timestamp),"remark":"original","uid":1,"date":"2026-09-09",
        "messages":[{"uid":1,"subject":"Review"}]})
}

#[tokio::test]
async fn mail_pending_actions_preserve_other_subjects_and_durable_replay_after_restart() {
    let mut fixture = Fixture::new().await;
    let mut expected = json!({"version":2,"workspaces":[]});
    for (index, id) in ["writer", "reader", "off"].into_iter().enumerate() {
        actor(&fixture, id, if id == "off" { "off" } else { "agent_all" }).await;
        let mut state = acl("");
        state["pending"] = json!([pending(id, u32::try_from(index).unwrap() + 1)]);
        expected["workspaces"]
            .as_array_mut()
            .unwrap()
            .push(json!({"data_key":fixture.data_key(id),"agents":{id:state}}));
    }
    fixture
        .server
        .inner
        .core
        .write_mail_access_control_data(&expected.to_string())
        .unwrap();
    assert_eq!(
        fixture
            .request("GET", "/api/mail-access-control/pending/all", Value::Null)
            .await,
        json!([pending("reader", 2), pending("writer", 1)])
    );
    assert_eq!(
        fixture
            .request("GET", "/api/mail-access-control/pending/count", Value::Null)
            .await,
        json!({"count":2})
    );
    assert_eq!(
        fixture
            .request(
                "POST",
                "/api/mail-access-control/pending/remark",
                json!({"agent_id":"writer","address":"same@example.com","remark":"reviewed"})
            )
            .await,
        json!({"status":"ok"})
    );
    expected["workspaces"][0]["agents"]["writer"]["pending"][0]["remark"] = json!("reviewed");
    assert_eq!(stored(&fixture), expected);
    for (index, id, kind) in [
        (0, "writer", "approve"),
        (1, "reader", "deny"),
        (2, "off", "dismiss"),
    ] {
        assert_eq!(
            action(
                &fixture,
                &format!("pending/{kind}"),
                json!([{"agent_id":id,"address":" SAME@EXAMPLE.COM ","remark":null}])
            )
            .await,
            json!({"status":"ok","count":1})
        );
        let state = &mut expected["workspaces"][index]["agents"][id];
        if kind == "approve" {
            state["approved_replay"] = state["pending"].clone();
            state["whitelist"] =
                json!({"same@example.com":{"remark":"reviewed","display_name":id}});
        } else if kind == "deny" {
            state["blacklist"] =
                json!({"same@example.com":{"remark":"original","display_name":id}});
        }
        state["pending"] = json!([]);
        assert_eq!(stored(&fixture), expected);
    }
    assert_eq!(
        fixture
            .request("GET", "/api/mail-access-control/pending/count", Value::Null)
            .await,
        json!({"count":0})
    );
    fixture.reopen().await;
    assert_eq!(stored(&fixture), expected);
}

#[tokio::test]
async fn mail_pending_equal_timestamps_retain_original_registration_order() {
    let fixture = Fixture::new().await;
    let mut workspaces = Vec::new();
    for id in ["writer", "reader"] {
        actor(&fixture, id, "agent_all").await;
        let mut state = acl("");
        state["pending"] = json!([pending(id, 1)]);
        workspaces.push(json!({"data_key":fixture.data_key(id),"agents":{id:state}}));
    }
    fixture
        .server
        .inner
        .core
        .write_mail_access_control_data(&json!({"version":2,"workspaces":workspaces}).to_string())
        .unwrap();
    assert_eq!(
        fixture
            .request("GET", "/api/mail-access-control/pending/all", Value::Null)
            .await,
        json!([pending("writer", 1), pending("reader", 1)])
    );
}

#[tokio::test]
async fn mail_batch_rejects_a_replaced_workspace_before_any_target_is_written() {
    let fixture = Fixture::new().await;
    for id in ["writer", "reader"] {
        actor(&fixture, id, "agent_all").await;
        action(
            &fixture,
            "whitelist/add",
            json!([{"agent_id":id,"address":"old@example.com"}]),
        )
        .await;
    }
    let before = stored(&fixture);
    let marker = fixture
        .directory
        .path()
        .join("data/workspaces/reader/.qwenpaw-workspace.json");
    let bytes = std::fs::read(&marker).unwrap();
    let replacement = serde_json::to_vec(&fixture.data_key("default")).unwrap();
    std::fs::write(&marker, &replacement).unwrap();
    for entries in [
        json!([{"agent_id":"writer","address":"new@example.com"},{"agent_id":"reader","address":"new@example.com"}]),
        json!([{"agent_id":"","address":"new@example.com"}]),
    ] {
        let (status, _) = raw(
            &fixture,
            "POST",
            "/api/mail-access-control/whitelist/add",
            json!({"entries":entries}),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(stored(&fixture), before);
        assert_eq!(std::fs::read(&marker).unwrap(), replacement);
    }
    std::fs::write(&marker, bytes).unwrap();
    assert_eq!(
        fixture
            .request("GET", "/api/mail-access-control", Value::Null)
            .await,
        json!({"writer":acl("old@example.com"),"reader":acl("old@example.com")})
    );
}

#[tokio::test]
async fn mail_write_finishes_before_deletion_and_cannot_retarget_a_reused_name() {
    let mut fixture = Fixture::new().await;
    actor(&fixture, "writer", "agent_all").await;
    let original_key = fixture.data_key("writer");
    {
        let guard = fixture
            .server
            .inner
            .desktop_mail_access_control_lock
            .lock()
            .await;
        let mutation = action(
            &fixture,
            "whitelist/add",
            json!([{"agent_id":"writer","address":"old@example.com"}]),
        );
        tokio::pin!(mutation);
        assert!(
            tokio::time::timeout(Duration::from_millis(20), &mut mutation)
                .await
                .is_err()
        );
        assert!(
            fixture
                .server
                .inner
                .desktop_agent_lifecycle_lock
                .try_lock()
                .is_err()
        );
        let deletion = fixture.request("DELETE", "/api/agents/writer", Value::Null);
        tokio::pin!(deletion);
        assert!(
            tokio::time::timeout(Duration::from_millis(20), &mut deletion)
                .await
                .is_err()
        );
        drop(guard);
        assert_eq!(mutation.await, json!({"status":"ok","count":1}));
        deletion.await;
    }
    let expected = json!({"version":2,"workspaces":[{"data_key":original_key,"agents":{"writer":acl("old@example.com")}}]});
    assert_eq!(stored(&fixture), expected);
    fixture
        .request(
            "POST",
            "/api/agents",
            json!({"id":"writer","name":"New Writer",
        "workspace_dir":fixture.directory.path().join("new-writer"),"mail":mail("agent_all")}),
        )
        .await;
    assert_ne!(fixture.data_key("writer"), original_key);
    fixture.reopen().await;
    assert_eq!(stored(&fixture), expected);
    assert_eq!(
        fixture
            .request("GET", "/api/mail-access-control", Value::Null)
            .await,
        json!({"writer":acl("")})
    );
}

#[tokio::test]
async fn mail_unknown_targets_do_not_create_or_upgrade_persistent_state() {
    let fixture = Fixture::new().await;
    for original in [
        None,
        Some(json!({"version":1,"agents":{"default":acl("")}}).to_string()),
    ] {
        if let Some(value) = &original {
            fixture
                .server
                .inner
                .core
                .write_mail_access_control_data(value)
                .unwrap();
        }
        for kind in [
            "whitelist/add",
            "blacklist/add",
            "whitelist/remove",
            "pending/approve",
            "pending/deny",
            "pending/dismiss",
        ] {
            assert_eq!(
                action(
                    &fixture,
                    kind,
                    json!([{"agent_id":"missing","address":"same@example.com"},
                    {"agent_id":"","address":"same@example.com"}])
                )
                .await,
                json!({"status":"ok","count":0})
            );
            assert_eq!(
                fixture
                    .server
                    .inner
                    .core
                    .read_mail_access_control_data()
                    .unwrap(),
                original
            );
        }
    }
}

async fn raw(fixture: &Fixture, method: &str, path: &str, body: Value) -> (StatusCode, Value) {
    let response = fixture
        .server
        .clone()
        .router()
        .oneshot(
            Request::builder()
                .method(method)
                .uri(path)
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 2 * 1024 * 1024)
        .await
        .unwrap();
    (status, serde_json::from_slice(&bytes).unwrap())
}

#[tokio::test]
async fn mail_visibility_broadcast_and_direct_targets_match_original_switches() {
    let fixture = Fixture::new().await;
    assert_eq!(
        fixture
            .request("GET", "/api/mail-access-control/agents", Value::Null)
            .await,
        json!({"agents":[]})
    );
    assert_eq!(
        fixture
            .request("GET", "/api/mail-access-control", Value::Null)
            .await,
        json!({})
    );
    for (id, mode) in [
        ("writer", "agent_all"),
        ("reader", "rules_only"),
        ("off", "off"),
        ("disabled", "agent_all"),
    ] {
        actor(&fixture, id, mode).await;
    }
    fixture
        .request(
            "PATCH",
            "/api/agents/disabled/toggle",
            json!({"enabled":false}),
        )
        .await;
    assert_eq!(
        fixture
            .request("GET", "/api/mail-access-control/agents", Value::Null)
            .await,
        json!({"agents":["writer","reader"]})
    );
    assert_eq!(
        action(
            &fixture,
            "whitelist/add",
            json!([
                {"agent_id":"","address":" ALICE@EXAMPLE.COM ","remark":null,"display_name":null},
                {"agent_id":"off","address":"off@example.com"},
                {"agent_id":"disabled","address":"disabled@example.com"},
                {"agent_id":"missing","address":"missing@example.com"}
            ])
        )
        .await,
        json!({"status":"ok","count":4})
    );
    assert_eq!(
        fixture
            .request("GET", "/api/mail-access-control", Value::Null)
            .await,
        json!({"writer":acl("alice@example.com"),"reader":acl("alice@example.com")})
    );
    assert_eq!(
        action(
            &fixture,
            "whitelist/remove",
            json!([{"agent_id":"","address":"alice@example.com"}])
        )
        .await,
        json!({"status":"ok","count":0})
    );
    assert_eq!(
        action(
            &fixture,
            "whitelist/add",
            json!([{"agent_id":"default","address":"default@example.com"}])
        )
        .await,
        json!({"status":"ok","count":1})
    );
    fixture
        .request(
            "PATCH",
            "/api/agents/disabled/toggle",
            json!({"enabled":true}),
        )
        .await;
    assert_eq!(
        fixture
            .request("GET", "/api/mail-access-control", Value::Null)
            .await,
        json!({"writer":acl("alice@example.com"),"reader":acl("alice@example.com"),"disabled":acl("disabled@example.com")})
    );
    assert_invalid_mail_batch_is_unchanged(&fixture).await;
}

async fn assert_invalid_mail_batch_is_unchanged(fixture: &Fixture) {
    let before = stored(fixture);
    assert_eq!(raw(fixture,"POST","/api/mail-access-control/whitelist/add",json!({"entries":[
        {"agent_id":"writer","address":"valid@example.com"},{"agent_id":"reader","address":"invalid"}
    ]})).await.0,StatusCode::BAD_REQUEST);
    assert_eq!(stored(fixture), before);
    assert_eq!(
        raw(
            fixture,
            "POST",
            "/api/mail-access-control/remark",
            json!({"agent_id":"missing","address":"alice@example.com","remark":"x"})
        )
        .await,
        (
            StatusCode::NOT_FOUND,
            json!({"detail":"Address not found in any list"})
        )
    );
}

#[tokio::test]
async fn mail_permissions_do_not_follow_a_new_agent_name_or_a_reused_name_in_a_new_root() {
    let mut fixture = Fixture::new().await;
    actor(&fixture, "writer", "agent_all").await;
    action(
        &fixture,
        "whitelist/add",
        json!([{"agent_id":"writer","address":"alice@example.com"}]),
    )
    .await;
    let original = stored(&fixture);
    let root = fixture.directory.path().join("data/workspaces/writer");
    fixture
        .request("DELETE", "/api/agents/writer", Value::Null)
        .await;
    fixture
        .request(
            "POST",
            "/api/agents",
            json!({"id":"editor","name":"Editor","workspace_dir":root,"mail":mail("agent_all")}),
        )
        .await;
    fixture.request("POST","/api/agents",json!({"id":"writer","name":"New Writer","workspace_dir":fixture.directory.path().join("new-writer"),"mail":mail("agent_all")})).await;
    fixture.reopen().await;
    assert_eq!(stored(&fixture), original);
    assert_eq!(
        fixture
            .request("GET", "/api/mail-access-control", Value::Null)
            .await,
        json!({"editor":acl(""),"writer":acl("")})
    );
    action(
        &fixture,
        "whitelist/add",
        json!([{"agent_id":"editor","address":"editor@example.com"}]),
    )
    .await;
    let with_editor = stored(&fixture);
    assert_eq!(
        with_editor["workspaces"][0]["agents"],
        json!({"writer":acl("alice@example.com"),"editor":acl("editor@example.com")})
    );
    fixture
        .request("DELETE", "/api/agents/editor", Value::Null)
        .await;
    fixture
        .request("DELETE", "/api/agents/writer", Value::Null)
        .await;
    fixture.request("POST","/api/agents",json!({"id":"writer","name":"Original Writer","workspace_dir":root,"mail":mail("agent_all")})).await;
    fixture.reopen().await;
    assert_eq!(stored(&fixture), with_editor);
    assert_eq!(
        fixture
            .request("GET", "/api/mail-access-control", Value::Null)
            .await,
        json!({"writer":acl("alice@example.com")})
    );
}

#[tokio::test]
async fn mail_old_native_state_cannot_be_claimed_by_a_new_uuid_registration() {
    let fixture = Fixture::new().await;
    actor(&fixture, "writer", "agent_all").await;
    let original=json!({"version":1,"agents":{"writer":acl("old@example.com"),"default":acl("default@example.com")}}).to_string();
    fixture
        .server
        .inner
        .core
        .write_mail_access_control_data(&original)
        .unwrap();
    assert_eq!(
        fixture
            .request("GET", "/api/mail-access-control", Value::Null)
            .await,
        json!({"writer":acl("")})
    );
    assert_eq!(
        fixture
            .server
            .inner
            .core
            .read_mail_access_control_data()
            .unwrap(),
        Some(original)
    );
    action(
        &fixture,
        "whitelist/add",
        json!([{"agent_id":"writer","address":"new@example.com"}]),
    )
    .await;
    let expected = json!({"version":2,"workspaces":[
        {"data_key":{"kind":"legacy_agent","id":"writer"},"agents":{"writer":acl("old@example.com")}},
        {"data_key":fixture.data_key("default"),"agents":{"default":acl("default@example.com")}},
        {"data_key":fixture.data_key("writer"),"agents":{"writer":acl("new@example.com")}}
    ]});
    assert_eq!(stored(&fixture), expected);
}
