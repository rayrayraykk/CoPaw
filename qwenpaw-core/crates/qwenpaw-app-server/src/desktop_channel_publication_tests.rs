//! Canonical channel views and multi-file publication must agree.

use super::*;
use pretty_assertions::assert_eq;

#[derive(Default)]
struct Secrets {
    recovery: crate::desktop_publication_test_support::RecoverySecrets,
    value: std::sync::Mutex<Option<String>>,
    writes: std::sync::Mutex<Vec<Option<String>>>,
    fail_after_write: std::sync::atomic::AtomicBool,
    fail_rollback: std::sync::atomic::AtomicBool,
    fail_prepare_after_write: std::sync::atomic::AtomicBool,
    fail_cleanup_after_delete: std::sync::atomic::AtomicBool,
}

impl crate::DesktopCredentialStore for Secrets {
    fn load_agent_publication_secret(
        &self,
        scope: &crate::AgentPublicationSecretScope,
    ) -> anyhow::Result<Option<crate::AgentPublicationSecret>> {
        self.recovery.load(scope)
    }
    fn prepare_agent_publication_secret(
        &self,
        scope: &crate::AgentPublicationSecretScope,
        secret: &crate::AgentPublicationSecret,
    ) -> anyhow::Result<()> {
        self.recovery.prepare(scope, secret)?;
        anyhow::ensure!(
            !self
                .fail_prepare_after_write
                .swap(false, std::sync::atomic::Ordering::SeqCst),
            "fixture private recovery preparation failed after write"
        );
        Ok(())
    }
    fn finish_agent_publication_secret(
        &self,
        scope: &crate::AgentPublicationSecretScope,
        expected: &crate::AgentPublicationSecret,
    ) -> anyhow::Result<()> {
        self.recovery.finish(scope, expected)?;
        anyhow::ensure!(
            !self
                .fail_cleanup_after_delete
                .swap(false, std::sync::atomic::Ordering::SeqCst),
            "fixture private recovery cleanup failed after delete"
        );
        Ok(())
    }
    fn load_api_key(&self) -> anyhow::Result<Option<String>> {
        Ok(None)
    }
    fn save_api_key(&self, _: Option<&str>) -> anyhow::Result<()> {
        panic!("no model secrets")
    }
    fn load_agent_setting_secret(&self, key: &str) -> anyhow::Result<Option<String>> {
        Ok(if key == "agent.writer.mail-auth-code" {
            self.value.lock().unwrap().clone()
        } else {
            None
        })
    }
    fn save_agent_setting_secret(&self, key: &str, value: Option<&str>) -> anyhow::Result<()> {
        assert_eq!(key, "agent.writer.mail-auth-code");
        anyhow::ensure!(
            value == Some("after-fixture")
                || !self.fail_rollback.load(std::sync::atomic::Ordering::SeqCst),
            "fixture credential rollback unavailable"
        );
        let value = value.map(str::to_owned);
        self.value.lock().unwrap().clone_from(&value);
        self.writes.lock().unwrap().push(value);
        anyhow::ensure!(
            !self
                .fail_after_write
                .swap(false, std::sync::atomic::Ordering::SeqCst),
            "fixture credential write failed after mutation"
        );
        Ok(())
    }
}

async fn profile(fixture: &Fixture) -> Value {
    let (status, value) = scoped(fixture, "writer", "GET", PROFILE, Value::Null).await;
    assert_eq!(status, StatusCode::OK, "{value}");
    value
}

#[path = "desktop_publication_recovery_tests.rs"]
mod recovery;

#[tokio::test]
async fn channel_publication_both_apis_contexts_and_reopen_share_one_value() {
    let (mut fixture, root) = setup().await;
    let defaults = profile(&fixture).await["channels"].clone();
    let mut console = defaults["console"].clone();
    console["bot_prefix"] = json!("dedicated");
    assert_eq!(
        scoped(&fixture, "writer", "PUT", SINGLE, console.clone()).await,
        (StatusCode::OK, console.clone())
    );
    let mut expected = profile(&fixture).await;
    assert_eq!(expected["channels"]["console"], console);
    expected["name"] = json!("Published");
    expected["channels"]["console"]["bot_prefix"] = json!("profile");
    assert_eq!(
        scoped(&fixture, "writer", "PUT", PROFILE, expected.clone()).await,
        (StatusCode::OK, expected.clone())
    );
    for reopen in [false, true] {
        if reopen {
            fixture.reopen().await;
        }
        assert_eq!(profile(&fixture).await, expected);
        assert_eq!(
            scoped(&fixture, "writer", "GET", SINGLE, Value::Null).await,
            (StatusCode::OK, expected["channels"]["console"].clone())
        );
        let context = crate::desktop_agents::context_for_agent(&fixture.server, "writer")
            .await
            .unwrap();
        assert_eq!(context.config, expected);
        assert_eq!(
            crate::desktop_agents::context_for_data_key(&fixture.server, &context.data_key)
                .await
                .unwrap()
                .config,
            expected
        );
        assert_eq!(
            crate::desktop_agents::mail_context(&fixture.server, "writer")
                .await
                .unwrap()
                .unwrap()
                .config,
            expected
        );
        assert_eq!(
            scoped(&fixture, "default", "GET", SINGLE, Value::Null).await,
            (StatusCode::OK, defaults["console"].clone())
        );
    }
    let disk: Value =
        serde_json::from_slice(&std::fs::read(root.join("agent.json")).unwrap()).unwrap();
    assert_eq!(disk["channels"], json!({}));
    assert_eq!(
        registry(&fixture)["agents"]["writer"]["config"]["channels"],
        json!({})
    );
}

#[tokio::test]
async fn channel_publication_null_is_distinct_from_defaults_and_survives_reopen() {
    let (mut fixture, _) = setup().await;
    let defaults = profile(&fixture).await["channels"].clone();
    let mut expected = profile(&fixture).await;
    expected["channels"] = Value::Null;
    assert_eq!(
        scoped(&fixture, "writer", "PUT", PROFILE, expected.clone()).await,
        (StatusCode::OK, expected.clone())
    );
    for reopen in [false, true] {
        if reopen {
            fixture.reopen().await;
        }
        assert_eq!(profile(&fixture).await, expected);
        for channel in ["console", "telegram"] {
            assert_eq!(
                scoped(
                    &fixture,
                    "writer",
                    "GET",
                    &format!("/api/config/channels/{channel}"),
                    Value::Null
                )
                .await,
                (
                    StatusCode::NOT_FOUND,
                    json!({"detail":format!("Channel '{channel}' not configured")})
                )
            );
        }
        let (_, list) = scoped(
            &fixture,
            "writer",
            "GET",
            "/api/config/channels",
            Value::Null,
        )
        .await;
        assert_eq!(list.as_object().unwrap().len(), 18);
        assert!(
            list.as_object()
                .unwrap()
                .values()
                .all(|value| *value == json!({"enabled":false,"bot_prefix":"","isBuiltin":true}))
        );
    }
    assert_eq!(
        scoped(
            &fixture,
            "writer",
            "PUT",
            SINGLE,
            defaults["console"].clone()
        )
        .await,
        (StatusCode::OK, defaults["console"].clone())
    );
    expected["channels"] = defaults;
    assert_eq!(profile(&fixture).await, expected);
}

#[tokio::test]
async fn channel_publication_sqlite_failure_restores_exact_files_and_absent_or_present_setting() {
    for existing in [false, true] {
        let (mut fixture, root) = setup().await;
        if existing {
            scoped(
                &fixture,
                "writer",
                "PUT",
                SINGLE,
                json!({"bot_prefix":"original"}),
            )
            .await;
        }
        let expected = profile(&fixture).await;
        let before = snapshot(&fixture, &root);
        let connection =
            rusqlite::Connection::open(fixture.directory.path().join("core.sqlite")).unwrap();
        connection.execute_batch("CREATE TRIGGER fail_channels BEFORE INSERT ON core_settings WHEN NEW.key = 'desktop_channel_config_data' BEGIN SELECT RAISE(ABORT, 'fixture channel publication failure'); END;").unwrap();
        let result = scoped(&fixture, "writer", "PUT", PROFILE,
            json!({"id":"writer","name":"must roll back","channels":{"console":{"bot_prefix":"changed"}}})).await;
        assert_eq!(result.0, StatusCode::INTERNAL_SERVER_ERROR, "{result:?}");
        assert_eq!(snapshot(&fixture, &root), before);
        fixture.reopen().await;
        assert_eq!(profile(&fixture).await, expected);
        assert_eq!(snapshot(&fixture, &root), before);
    }
}

#[tokio::test]
async fn channel_publication_conflicting_shadow_is_preserved_and_never_imported() {
    let (fixture, root) = setup().await;
    let expected = profile(&fixture).await;
    let mut catalog = registry(&fixture);
    let raw = &mut catalog["agents"]["writer"]["config"];
    raw["channels"] = json!({"console":{"bot_prefix":"old shadow"}});
    std::fs::write(
        root.join("agent.json"),
        serde_json::to_vec_pretty(raw).unwrap(),
    )
    .unwrap();
    std::fs::write(
        fixture.directory.path().join("data/agents/catalog.json"),
        serde_json::to_vec_pretty(&catalog).unwrap(),
    )
    .unwrap();
    let before = snapshot(&fixture, &root);
    assert_eq!(profile(&fixture).await, expected);
    assert_eq!(
        scoped(&fixture, "writer", "PUT", PROFILE, expected).await,
        (
            StatusCode::CONFLICT,
            json!({"detail":"Stored Agent Channels conflict with Workspace configuration; original data was preserved"})
        )
    );
    assert_eq!(snapshot(&fixture, &root), before);
}

#[tokio::test]
async fn channel_publication_credentials_restore_after_mutating_failure_or_sqlite_failure() {
    for secret_failure in [false, true] {
        let (mut fixture, root) = setup().await;
        let secrets = Arc::new(Secrets::default());
        *secrets.value.lock().unwrap() = Some(String::from("before-fixture"));
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
        let before = snapshot(&fixture, &root);
        if secret_failure {
            secrets
                .fail_after_write
                .store(true, std::sync::atomic::Ordering::SeqCst);
        } else {
            rusqlite::Connection::open(fixture.directory.path().join("core.sqlite")).unwrap()
                .execute_batch("CREATE TRIGGER fail_channels BEFORE INSERT ON core_settings WHEN NEW.key = 'desktop_channel_config_data' BEGIN SELECT RAISE(ABORT, 'fixture channel publication failure'); END;").unwrap();
        }
        let result = scoped(&fixture, "writer", "PUT", PROFILE,
            json!({"id":"writer","name":"Do not publish","channels":{"console":{"bot_prefix":"changed"}},
                "mail":{"credential":{"auth_code":"after-fixture"}}})).await;
        assert_eq!(result.0, StatusCode::INTERNAL_SERVER_ERROR, "{result:?}");
        assert_eq!(snapshot(&fixture, &root), before);
        assert_eq!(
            *secrets.value.lock().unwrap(),
            Some(String::from("before-fixture"))
        );
        assert_eq!(
            *secrets.writes.lock().unwrap(),
            vec![
                Some(String::from("after-fixture")),
                Some(String::from("before-fixture"))
            ]
        );
    }
}

#[tokio::test]
async fn channel_publication_failed_first_file_creation_restores_absence_and_rejects_directories() {
    let (fixture, root) = setup().await;
    std::fs::rename(root.join("agent.json"), root.join("retained-agent.json")).unwrap();
    let before = registry(&fixture);
    rusqlite::Connection::open(fixture.directory.path().join("core.sqlite")).unwrap()
        .execute_batch("CREATE TRIGGER fail_channels BEFORE INSERT ON core_settings WHEN NEW.key = 'desktop_channel_config_data' BEGIN SELECT RAISE(ABORT, 'fixture channel publication failure'); END;").unwrap();
    let body = json!({"id":"writer","name":"Do not publish","channels":{"console":{"bot_prefix":"changed"}}});
    assert_eq!(
        scoped(&fixture, "writer", "PUT", PROFILE, body.clone())
            .await
            .0,
        StatusCode::INTERNAL_SERVER_ERROR
    );
    assert!(!root.join("agent.json").exists());
    assert_eq!(registry(&fixture), before);
    assert_eq!(
        fixture
            .server
            .inner
            .core
            .read_channel_config_data()
            .unwrap(),
        None
    );
    std::fs::create_dir(root.join("agent.json")).unwrap();
    std::fs::write(root.join("agent.json/keep"), "independent data").unwrap();
    assert_eq!(
        scoped(&fixture, "writer", "PUT", PROFILE, body).await,
        (
            StatusCode::BAD_REQUEST,
            json!({"detail":"Agent config is not a regular file"})
        )
    );
    assert_eq!(
        std::fs::read_to_string(root.join("agent.json/keep")).unwrap(),
        "independent data"
    );
    assert_eq!(registry(&fixture), before);
}

#[tokio::test]
async fn channel_publication_disabled_management_and_concurrent_updates_preserve_channels() {
    let (fixture, _) = setup().await;
    scoped(
        &fixture,
        "writer",
        "PATCH",
        "/api/agents/writer/toggle",
        json!({"enabled":false}),
    )
    .await;
    let (_, saved) = scoped(&fixture, "writer", "PUT", PROFILE,
        json!({"id":"writer","name":"Disabled edit","channels":{"console":{"bot_prefix":"disabled"}}})).await;
    assert_eq!(profile(&fixture).await, saved);
    assert_eq!(saved["channels"]["console"]["bot_prefix"], "disabled");
    scoped(
        &fixture,
        "writer",
        "PATCH",
        "/api/agents/writer/toggle",
        json!({"enabled":true}),
    )
    .await;
    for i in 0..10 {
        let (metadata, channel) = tokio::join!(
            scoped(
                &fixture,
                "writer",
                "PUT",
                PROFILE,
                json!({"name":format!("Edit {i}")})
            ),
            scoped(
                &fixture,
                "writer",
                "PUT",
                SINGLE,
                json!({"bot_prefix":format!("Prefix {i}")})
            ),
        );
        assert_eq!(metadata.0, StatusCode::OK);
        assert_eq!(channel.0, StatusCode::OK);
        let final_profile = profile(&fixture).await;
        assert_eq!(final_profile["name"], format!("Edit {i}"));
        assert_eq!(final_profile["channels"]["console"], channel.1);
    }
}
