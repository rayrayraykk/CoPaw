//! Trusted SDK settings preserve explicit Thread model selection.

use super::*;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn thread_model_host_settings_snapshot_provider_options_and_runtime_together() {
    for selected in [false, true] {
        let requests = Arc::new(Mutex::new(Vec::new()));
        let config = ModelConfig {
            api_key: None,
            base_url: start_tool_model_server(Arc::clone(&requests), "read_file").await,
            default_model: String::from("provider-default"),
        };
        let core = Core::new(config.clone());
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("notes.txt"), "snapshot fixture").unwrap();
        let id = core
            .start_thread(ThreadStartParams {
                model: Some(String::from("caller-choice")),
                workspace_root: Some(directory.path().to_string_lossy().into_owned()),
            })
            .await
            .unwrap()
            .thread
            .id;
        let options = ModelRequestOptions {
            generate_kwargs: serde_json::from_value(serde_json::json!({"temperature":0.1}))
                .unwrap(),
            model_generate_kwargs: serde_json::from_value(serde_json::json!({
                "caller-choice":{"top_p":0.2},"provider-default":{"top_p":0.9}
            }))
            .unwrap(),
            ..Default::default()
        };
        core.configure_model_runtime(config.clone(), options.clone())
            .unwrap();
        let selection = selected.then_some((config, options));
        let (_, mut events) = core
            .start_turn_with_thread_model(
                input(&id),
                selection,
                runtime(ToolApprovalLevel::Off),
                Some(UsageOwner {
                    agent_id: String::from("writer"),
                    data_key: WorkspaceDataKey::LegacyAgent(String::from("writer")),
                }),
            )
            .await
            .unwrap();
        let replacement = ModelConfig {
            api_key: None,
            base_url: String::from("http://127.0.0.1:1"),
            default_model: String::from("later-global"),
        };
        core.configure_model_runtime(replacement.clone(), ModelRequestOptions::default())
            .unwrap();
        core.replace_agent_runtime_config(runtime(ToolApprovalLevel::Strict))
            .unwrap();
        assert_eq!(finish(&mut events).await.status, TurnStatus::Completed);
        let requests = requests.lock().await;
        let bodies = requests
            .iter()
            .map(|body| {
                serde_json::json!({
                    "model":body["model"],"temperature":body["temperature"],"top_p":body["top_p"]
                })
            })
            .collect::<Vec<_>>();
        assert_eq!(
            bodies,
            vec![serde_json::json!({"model":"caller-choice","temperature":0.1,"top_p":0.2}); 2]
        );
        assert_eq!(
            core.read_thread(&id).await.unwrap().thread.model,
            "caller-choice"
        );
        assert_eq!(core.backup_model_config(), replacement);
        assert_eq!(
            core.agent_runtime_config().unwrap(),
            runtime(ToolApprovalLevel::Strict)
        );
    }
}

#[tokio::test]
async fn thread_model_invalid_host_settings_cannot_record_input() {
    let (core, directory) = fixture("read_file").await;
    let id = thread(&core, &directory).await;
    let before = core.backup_snapshot(1024 * 1024).unwrap();
    let mut invalid_runtime = runtime(ToolApprovalLevel::Off);
    invalid_runtime.max_agent_steps = 0;
    let invalid_owner = UsageOwner {
        agent_id: String::from("../writer"),
        data_key: WorkspaceDataKey::LegacyAgent(String::from("writer")),
    };
    let invalid_options = ModelRequestOptions {
        custom_headers: std::collections::BTreeMap::from([(
            String::from("invalid\nheader"),
            String::from("x"),
        )]),
        ..Default::default()
    };
    for (selection, runtime, owner) in [
        (None, invalid_runtime, None),
        (None, runtime(ToolApprovalLevel::Off), Some(invalid_owner)),
        (
            Some((core.backup_model_config(), invalid_options)),
            runtime(ToolApprovalLevel::Off),
            None,
        ),
    ] {
        assert!(
            core.start_turn_with_thread_model(input(&id), selection, runtime, owner)
                .await
                .is_err()
        );
        assert_eq!(core.backup_snapshot(1024 * 1024).unwrap(), before);
        assert_eq!(
            core.export_thread_checkpoint(&id).await.unwrap(),
            before.threads[0]
        );
    }
    let (_, mut events) = core
        .start_turn_with_thread_model(input(&id), None, runtime(ToolApprovalLevel::Off), None)
        .await
        .unwrap();
    assert_eq!(finish(&mut events).await.status, TurnStatus::Completed);
}
