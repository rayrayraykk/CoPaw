//! Failed publication must retain inverse operations until recovery succeeds.

use super::*;
use axum::body::Body;
use axum::http::Request;
use pretty_assertions::assert_eq;
use std::sync::atomic::Ordering;

fn reopen_with_secrets(fixture: &Fixture, secrets: Arc<Secrets>) -> anyhow::Result<AppServer> {
    AppServer::new_desktop_with_stores_and_workspace(
        Core::persistent(
            fixture.model.clone(),
            &fixture.directory.path().join("core.sqlite"),
        )?,
        fixture.directory.path(),
        String::from("fixture-shutdown"),
        secrets,
        &fixture.directory.path().join("data"),
        &fixture.directory.path().join("workspace"),
    )
}

#[tokio::test]
async fn publication_host_startup_refuses_pending_inverse_then_recovers_from_reopened_database() {
    for old in [None, Some("before-fixture")] {
        let (mut fixture, root, secrets, expected) = pending(old).await;
        let before = snapshot(&fixture, &root);
        fixture.server.inner.shutdown.cancel();
        assert!(reopen_with_secrets(&fixture, secrets.clone()).is_err());
        assert_eq!(snapshot(&fixture, &root), before);
        assert!(
            fixture
                .server
                .inner
                .core
                .read_agent_publication()
                .unwrap()
                .is_some()
        );
        secrets.fail_rollback.store(false, Ordering::SeqCst);
        fixture.server = reopen_with_secrets(&fixture, secrets.clone()).unwrap();
        assert_eq!(snapshot(&fixture, &root), before);
        assert_eq!(profile(&fixture).await, expected);
        assert_eq!(*secrets.value.lock().unwrap(), old.map(str::to_owned));
        assert_eq!(
            *secrets.writes.lock().unwrap(),
            vec![Some("after-fixture".into()), old.map(str::to_owned)]
        );
        assert_eq!(
            fixture.server.inner.core.read_agent_publication().unwrap(),
            None
        );
    }
}

#[tokio::test]
async fn publication_host_cleaning_reopens_after_private_snapshot_deletion_without_repeating_inverse()
 {
    for committed in [false, true] {
        let (mut fixture, root) = setup().await;
        let secrets = Arc::new(Secrets::default());
        *secrets.value.lock().unwrap() = Some("before-fixture".into());
        fixture.server.inner.shutdown.cancel();
        fixture.server = reopen_with_secrets(&fixture, secrets.clone()).unwrap();
        let before = snapshot(&fixture, &root);
        let previous_profile = profile(&fixture).await;
        secrets
            .fail_prepare_after_write
            .store(!committed, Ordering::SeqCst);
        secrets
            .fail_cleanup_after_delete
            .store(true, Ordering::SeqCst);
        let response = scoped(
            &fixture,
            "writer",
            "PUT",
            PROFILE,
            json!({
                "name":"Published durably", "channels":{"console":{"bot_prefix":"durable"}},
                "mail":{"credential":{"auth_code":"after-fixture"}}
            }),
        )
        .await;
        assert_eq!(response.0, StatusCode::INTERNAL_SERVER_ERROR);
        let core = &fixture.server.inner.core;
        let decision = core.read_agent_publication().unwrap().unwrap();
        assert_eq!(
            decision.state,
            if committed {
                qwenpaw_core::AgentPublicationState::Committed
            } else {
                qwenpaw_core::AgentPublicationState::Prepared
            }
        );
        assert_eq!(
            core.read_agent_publication_recovery(decision.id)
                .unwrap()
                .unwrap()
                .phase,
            qwenpaw_core::AgentPublicationPhase::Cleaning
        );
        let after = snapshot(&fixture, &root);
        if !committed {
            assert_eq!(after, before);
        }
        fixture.server.inner.shutdown.cancel();
        fixture.server = reopen_with_secrets(&fixture, secrets.clone()).unwrap();
        assert_eq!(snapshot(&fixture, &root), after);
        let restored = profile(&fixture).await;
        if committed {
            assert_eq!(restored["name"], json!("Published durably"));
            assert_eq!(
                restored["channels"]["console"]["bot_prefix"],
                json!("durable")
            );
        } else {
            assert_eq!(restored, previous_profile);
        }
        assert_eq!(
            *secrets.value.lock().unwrap(),
            Some(
                if committed {
                    "after-fixture"
                } else {
                    "before-fixture"
                }
                .into()
            )
        );
        assert_eq!(
            *secrets.writes.lock().unwrap(),
            if committed {
                vec![Some("after-fixture".into())]
            } else {
                vec![]
            }
        );
        assert_eq!(
            fixture.server.inner.core.read_agent_publication().unwrap(),
            None
        );
    }
}

async fn pending(old: Option<&str>) -> (Fixture, std::path::PathBuf, Arc<Secrets>, Value) {
    let (mut fixture, root) = setup().await;
    let secrets = Arc::new(Secrets::default());
    *secrets.value.lock().unwrap() = old.map(str::to_owned);
    fixture.server.inner.shutdown.cancel();
    fixture.server = AppServer::new_desktop_with_stores_and_workspace(
        fixture.server.inner.core.clone(),
        fixture.directory.path(),
        String::from("fixture-shutdown"),
        secrets.clone(),
        &fixture.directory.path().join("data"),
        &fixture.directory.path().join("workspace"),
    )
    .unwrap();
    let expected = profile(&fixture).await;
    let before = snapshot(&fixture, &root);
    secrets.fail_after_write.store(true, Ordering::SeqCst);
    secrets.fail_rollback.store(true, Ordering::SeqCst);
    let result = scoped(&fixture, "writer", "PUT", PROFILE,
        json!({"id":"writer","name":"Do not publish","channels":{"console":{"bot_prefix":"changed"}},
            "mail":{"credential":{"auth_code":"after-fixture"}}})).await;
    assert_eq!(result.0, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(snapshot(&fixture, &root), before);
    assert_eq!(
        *secrets.value.lock().unwrap(),
        Some(String::from("after-fixture"))
    );
    (fixture, root, secrets, expected)
}

#[tokio::test]
async fn publication_recovery_blocks_new_writes_and_retries_original_absent_or_present_secret() {
    for old in [None, Some("before-fixture")] {
        let (fixture, root, secrets, mut expected) = pending(old).await;
        let before = snapshot(&fixture, &root);
        for (agent, method, uri, body) in [
            (
                "writer",
                "PUT",
                PROFILE,
                json!({"name":"must not replace inverse"}),
            ),
            (
                "default",
                "PUT",
                "/api/agents/default",
                json!({"name":"must not publish"}),
            ),
            (
                "writer",
                "PUT",
                SINGLE,
                json!({"bot_prefix":"must not publish"}),
            ),
            ("writer", "GET", PROFILE, Value::Null),
            ("writer", "DELETE", PROFILE, Value::Null),
        ] {
            let response = scoped(&fixture, agent, method, uri, body).await;
            assert_eq!(response.0, StatusCode::CONFLICT, "{response:?}");
            assert_eq!(snapshot(&fixture, &root), before);
            assert_eq!(
                *secrets.value.lock().unwrap(),
                Some(String::from("after-fixture"))
            );
        }
        secrets.fail_rollback.store(false, Ordering::SeqCst);
        expected["name"] = json!("Recovered");
        assert_eq!(
            scoped(
                &fixture,
                "writer",
                "PUT",
                PROFILE,
                json!({"name":"Recovered"})
            )
            .await,
            (StatusCode::OK, expected.clone())
        );
        assert_eq!(profile(&fixture).await, expected);
        assert_eq!(*secrets.value.lock().unwrap(), old.map(str::to_owned));
        assert_eq!(
            *secrets.writes.lock().unwrap(),
            vec![Some(String::from("after-fixture")), old.map(str::to_owned)]
        );
    }
}

#[tokio::test]
async fn publication_recovery_http_protocol_and_backup_gates_preserve_health_and_retry() {
    let (fixture, root, secrets, _) = pending(Some("before-fixture")).await;
    let before = snapshot(&fixture, &root);
    let router = fixture.server.clone().router();
    for (method, uri, status) in [
        ("GET", "/healthz", StatusCode::OK),
        ("GET", "/api/healthz", StatusCode::OK),
        ("GET", "/readyz", StatusCode::SERVICE_UNAVAILABLE),
        ("GET", "/api/agents", StatusCode::CONFLICT),
        ("GET", "/api/models", StatusCode::CONFLICT),
        ("POST", "/api/backups/fixture/restore", StatusCode::CONFLICT),
        ("PUT", PROFILE, StatusCode::CONFLICT),
        ("POST", "/api/desktop/shutdown", StatusCode::NOT_FOUND),
    ] {
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(uri)
                    .header("content-type", "application/json")
                    .body(Body::from("{\"name\":\"must not publish\"}"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), status, "{method} {uri}");
        assert_eq!(snapshot(&fixture, &root), before);
    }
    assert!(!fixture.server.inner.shutdown.is_cancelled());
    let error = fixture
        .server
        .dispatch("initialize", json!({}))
        .await
        .err()
        .unwrap();
    assert_eq!(error.code, -32000);
    assert_eq!(
        error.message,
        "Agent publication recovery is pending; retry Agent configuration save"
    );
    // This direct router deliberately omits the outer HTTP gate, proving that
    // restore itself rejects the request before archive or credential work.
    let response = crate::desktop_backups::router()
        .with_state(fixture.server.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/backups/fixture/restore")
                .header("content-type", "application/json")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CONFLICT);
    secrets.fail_rollback.store(false, Ordering::SeqCst);
    let response = router
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(PROFILE)
                .header("content-type", "application/json")
                .body(Body::from("{\"name\":\"Recovered via HTTP\"}"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(profile(&fixture).await["name"], "Recovered via HTTP");
    assert_eq!(
        *secrets.value.lock().unwrap(),
        Some(String::from("before-fixture"))
    );
}

#[tokio::test]
async fn publication_recovery_shutdown_retries_without_dropping_failed_inverse_or_reopening_admission()
 {
    for old in [None, Some("before-fixture")] {
        let (fixture, root, secrets, _) = pending(old).await;
        let before = snapshot(&fixture, &root);
        fixture.server.shutdown_services().await;
        assert!(fixture.server.inner.shutdown.is_cancelled());
        assert!(fixture.server.ensure_agent_publication_available().is_err());
        assert_eq!(
            *secrets.value.lock().unwrap(),
            Some(String::from("after-fixture"))
        );
        secrets.fail_rollback.store(false, Ordering::SeqCst);
        fixture.server.shutdown_services().await;
        assert!(fixture.server.ensure_agent_publication_available().is_ok());
        assert_eq!(*secrets.value.lock().unwrap(), old.map(str::to_owned));
        assert_eq!(snapshot(&fixture, &root), before);
        let error = fixture
            .server
            .dispatch("initialize", json!({}))
            .await
            .err()
            .unwrap();
        assert_eq!(error.message, "App Server is shutting down");
        fixture.server.shutdown_services().await;
        assert_eq!(
            *secrets.writes.lock().unwrap(),
            vec![Some(String::from("after-fixture")), old.map(str::to_owned)]
        );
    }
}
