use pretty_assertions::assert_eq;
use serde_json::json;

use super::*;

fn config(name: &str) -> ModelConfig {
    ModelConfig {
        api_key: Some(format!("private-key-{name}")),
        base_url: format!("http://{name}.test/v1"),
        default_model: name.to_owned(),
    }
}

fn options(name: &str) -> ModelRequestOptions {
    ModelRequestOptions {
        custom_headers: BTreeMap::from([(
            String::from("x-private"),
            format!("private-header-{name}"),
        )]),
        generate_kwargs: json!({"temperature": 0.3}).as_object().unwrap().clone(),
        ..ModelRequestOptions::default()
    }
}

#[tokio::test]
async fn invalid_turn_model_settings_do_not_change_history_or_global_settings() {
    let directory = tempfile::tempdir().unwrap();
    let core = Core::new(config("global"));
    let thread = core
        .start_thread(ThreadStartParams {
            model: None,
            workspace_root: Some(directory.path().to_string_lossy().into_owned()),
        })
        .await
        .unwrap()
        .thread;
    let before = core.backup_snapshot(1024 * 1024).unwrap();
    let mut invalid = options("agent");
    invalid
        .generate_kwargs
        .insert(String::from("messages"), json!([]));
    let result = core
        .start_turn_with_model(
            TurnStartParams {
                thread_id: thread.id,
                input: vec![qwenpaw_protocol::UserInput::Text {
                    text: String::from("hello"),
                }],
            },
            Some((config("agent"), invalid)),
        )
        .await;
    assert!(matches!(result, Err(CoreError::Config(_))));
    assert_eq!(core.backup_snapshot(1024 * 1024).unwrap(), before);
    assert_eq!(core.inner.model.config_snapshot(), config("global"));
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn model_options_are_atomic_on_failure_and_survive_restore_and_rollback() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("core.sqlite3");
    let core = Core::persistent(config("first"), &path).unwrap();
    core.configure_model_runtime(config("first"), options("first"))
        .unwrap();
    let before = core.backup_snapshot(1024 * 1024).unwrap();
    let retained = core.prepare_restore(&before).unwrap();
    assert_eq!(
        retained.inner.model.runtime_snapshot().options,
        options("first")
    );
    let mut changed_endpoint = before.clone();
    changed_endpoint.settings.insert(
        String::from(BASE_URL_SETTING),
        String::from("http://elsewhere.test/v1"),
    );
    let isolated = core.prepare_restore(&changed_endpoint).unwrap();
    assert_eq!(
        isolated.inner.model.runtime_snapshot().options,
        ModelRequestOptions::default()
    );
    let serialized = serde_json::to_string(&before).unwrap();
    assert!(!serialized.contains("private-header"));
    assert!(!serialized.contains("private-key"));
    assert!(
        !serde_json::to_string(&core.read_config())
            .unwrap()
            .contains("private-")
    );
    let connection = rusqlite::Connection::open(&path).unwrap();
    connection.execute_batch("CREATE TRIGGER fail_settings BEFORE INSERT ON core_settings BEGIN SELECT RAISE(FAIL, 'fixture failure'); END;").unwrap();
    assert!(
        core.configure_model_runtime(config("next"), options("next"))
            .is_err()
    );
    assert_eq!(core.backup_snapshot(1024 * 1024).unwrap(), before);
    let runtime = core.inner.model.runtime_snapshot();
    assert_eq!(runtime.config, config("first"));
    assert_eq!(runtime.options, options("first"));
    connection
        .execute_batch("DROP TRIGGER fail_settings;")
        .unwrap();

    let mut invalid = options("invalid");
    invalid.custom_headers.insert(
        String::from("x-private"),
        String::from("private\ninjection"),
    );
    let error = core
        .configure_model_runtime(config("next"), invalid)
        .unwrap_err();
    assert!(!error.to_string().contains("private"));
    assert_eq!(core.backup_snapshot(1024 * 1024).unwrap(), before);
    assert_eq!(
        core.inner.model.runtime_snapshot().options,
        options("first")
    );

    let candidate = Core::new(config("restored"));
    candidate
        .configure_model_runtime(config("restored"), options("restored"))
        .unwrap();
    let mut guard = core.begin_restore(Duration::from_secs(2)).await.unwrap();
    assert_eq!(
        core.configure_model_runtime(config("busy"), options("busy")),
        Err(CoreError::RestoreBusy)
    );
    let rollback = guard.capture_rollback(1024 * 1024).unwrap();
    guard.apply(&candidate, 1024 * 1024).await.unwrap();
    assert_eq!(
        core.inner.model.runtime_snapshot().options,
        options("restored")
    );
    assert_eq!(core.backup_model_config(), config("restored"));
    guard.apply(&rollback, 1024 * 1024).await.unwrap();
    assert_eq!(
        core.inner.model.runtime_snapshot().options,
        options("first")
    );
    assert_eq!(core.backup_model_config(), config("first"));
    drop(guard);

    core.write_config(ConfigWriteParams {
        base_url: None,
        default_model: Some(String::from("other-model")),
    })
    .unwrap();
    assert_eq!(
        core.inner.model.runtime_snapshot().options,
        options("first")
    );
    core.set_runtime_api_key(None).unwrap();
    assert_eq!(
        core.inner.model.runtime_snapshot().options,
        options("first")
    );
    core.write_config(ConfigWriteParams {
        base_url: Some(String::from("http://other.test/v1")),
        default_model: None,
    })
    .unwrap();
    assert_eq!(
        core.inner.model.runtime_snapshot().options,
        ModelRequestOptions::default()
    );
}

#[test]
fn concurrent_model_updates_never_mix_provider_headers_and_credentials() {
    let core = Core::new(config("first"));
    core.configure_model_runtime(config("first"), options("first"))
        .unwrap();
    std::thread::scope(|scope| {
        scope.spawn(|| {
            for index in 0..1000 {
                let name = if index % 2 == 0 { "next" } else { "first" };
                core.configure_model_runtime(config(name), options(name))
                    .unwrap();
            }
        });
        for _ in 0..1000 {
            let runtime = core.inner.model.runtime_snapshot();
            let name = &runtime.config.default_model;
            assert_eq!(&runtime.config, &config(name));
            assert_eq!(runtime.options, options(name));
        }
    });
}
