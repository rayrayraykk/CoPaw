//! Trusted per-turn runtime snapshots must not change another turn's policy.

use pretty_assertions::assert_eq;

use super::*;

#[path = "runtime_quiescence_tests.rs"]
mod quiescence;

#[path = "runtime_active_checkpoint_tests.rs"]
mod active_checkpoint;

#[path = "runtime_persistence_tests.rs"]
mod persistence;

#[path = "runtime_thread_model_tests.rs"]
mod thread_model;

fn runtime(level: ToolApprovalLevel) -> AgentRuntimeConfig {
    AgentRuntimeConfig {
        approval_level: level,
        ..AgentRuntimeConfig::default()
    }
}

#[tokio::test]
async fn invalid_usage_identity_cannot_record_input_or_change_global_policy() {
    let (core, directory) = fixture("read_file").await;
    let id = thread(&core, &directory).await;
    let before = core.backup_snapshot(1024 * 1024).unwrap();
    let global = core.agent_runtime_config().unwrap();
    for owner in [
        UsageOwner {
            agent_id: String::from("writer"),
            data_key: WorkspaceDataKey::Workspace(uuid::Uuid::nil()),
        },
        UsageOwner {
            agent_id: String::from("../writer"),
            data_key: WorkspaceDataKey::LegacyAgent(String::from("default")),
        },
    ] {
        assert_eq!(
            core.start_turn_with_owner(
                input(&id),
                None,
                runtime(ToolApprovalLevel::Off),
                Some(owner)
            )
            .await
            .unwrap_err(),
            CoreError::Config(String::from("Invalid usage ownership"))
        );
        assert_eq!(core.backup_snapshot(1024 * 1024).unwrap(), before);
        assert_eq!(core.agent_runtime_config().unwrap(), global);
    }
}

async fn thread(core: &Core, directory: &tempfile::TempDir) -> String {
    core.start_thread(ThreadStartParams {
        model: None,
        workspace_root: Some(directory.path().to_string_lossy().into_owned()),
    })
    .await
    .unwrap()
    .thread
    .id
}

fn input(thread_id: &str) -> TurnStartParams {
    TurnStartParams {
        thread_id: thread_id.to_owned(),
        input: vec![UserInput::Text {
            text: String::from("Run the fixture tool"),
        }],
    }
}

async fn fixture(tool: &'static str) -> (Core, tempfile::TempDir) {
    let base_url = start_tool_model_server(Arc::new(Mutex::new(Vec::new())), tool).await;
    let core = Core::new(ModelConfig {
        api_key: None,
        base_url,
        default_model: String::from("fixture"),
    });
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("notes.txt"), "runtime fixture").unwrap();
    (core, directory)
}

async fn next(events: &mut TurnEventStream) -> CoreEvent {
    tokio::time::timeout(Duration::from_secs(5), events.recv())
        .await
        .expect("turn event must arrive before timeout")
        .expect("turn must emit completion before closing")
}

async fn finish(events: &mut TurnEventStream) -> qwenpaw_protocol::Turn {
    loop {
        match next(events).await {
            CoreEvent::ToolApprovalRequested(_) => panic!("unexpected approval"),
            CoreEvent::TurnCompleted(notification) => return notification.turn,
            _ => {}
        }
    }
}

async fn approval(
    events: &mut TurnEventStream,
) -> qwenpaw_protocol::ToolApprovalRequestedNotification {
    loop {
        match next(events).await {
            CoreEvent::ToolApprovalRequested(notification) => return notification,
            CoreEvent::TurnCompleted(turn) => panic!("turn finished without approval: {turn:?}"),
            _ => {}
        }
    }
}

#[tokio::test]
async fn ordinary_turns_snapshot_runtime_before_returning_to_the_caller() {
    for use_model_api in [false, true] {
        let (core, directory) = fixture("read_file").await;
        let id = thread(&core, &directory).await;
        core.replace_agent_runtime_config(runtime(ToolApprovalLevel::Strict))
            .unwrap();
        let (_, mut events) = if use_model_api {
            core.start_turn_with_model(input(&id), None).await.unwrap()
        } else {
            core.start_turn(input(&id)).await.unwrap()
        };
        // A current-thread runtime does not poll the spawned turn before this
        // synchronous update. Reading settings inside run_turn is too late.
        core.replace_agent_runtime_config(runtime(ToolApprovalLevel::Off))
            .unwrap();
        let request = approval(&mut events).await;
        assert_eq!(request.tool_name, "read_file");
        assert!(
            core.respond_tool_approval(ToolApprovalRespondParams {
                approval_id: request.approval_id,
                decision: ApprovalDecision::Approved,
            })
            .await
            .accepted
        );
        let turn = finish(&mut events).await;
        assert_eq!(turn.status, TurnStatus::Completed);
        assert_eq!(
            core.agent_runtime_config().unwrap(),
            runtime(ToolApprovalLevel::Off)
        );
    }
}

#[tokio::test]
async fn unattended_turn_does_not_approve_a_parallel_interactive_turn() {
    let (core, directory) = fixture("shell").await;
    let interactive = thread(&core, &directory).await;
    let background = thread(&core, &directory).await;
    let original = runtime(ToolApprovalLevel::Auto);
    let (_, mut normal_events) = core.start_turn(input(&interactive)).await.unwrap();
    let request = approval(&mut normal_events).await;
    assert_eq!(request.tool_name, "shell");
    let background_requests = Arc::new(Mutex::new(Vec::new()));
    let model = ModelConfig {
        api_key: None,
        base_url: start_tool_model_server(Arc::clone(&background_requests), "shell").await,
        default_model: String::from("background-fixture"),
    };
    let (_, mut background_events) = core
        .start_turn_with_runtime(
            input(&background),
            Some((model, ModelRequestOptions::default())),
            runtime(ToolApprovalLevel::Off),
        )
        .await
        .unwrap();
    assert_eq!(core.agent_runtime_config().unwrap(), original);
    let completed = finish(&mut background_events).await;
    assert_eq!(completed.status, TurnStatus::Completed);
    assert_eq!(completed.items.len(), 4);
    assert!(matches!(&completed.items[2], Item::ToolResult {
        is_error: false, content, ..
    } if content.contains("approved")));
    assert_eq!(background_requests.lock().await.len(), 2);
    let waiting = core.read_thread(&interactive).await.unwrap();
    assert_eq!(waiting.turns[0].status, TurnStatus::InProgress);
    assert_eq!(waiting.turns[0].items.len(), 2);
    assert_eq!(core.agent_runtime_config().unwrap(), original);
    assert!(
        core.respond_tool_approval(ToolApprovalRespondParams {
            approval_id: request.approval_id,
            decision: ApprovalDecision::Denied,
        })
        .await
        .accepted
    );
    let denied = finish(&mut normal_events).await;
    assert_eq!(denied.status, TurnStatus::Completed);
    assert!(matches!(
        &denied.items[2],
        Item::ToolResult { is_error: true, .. }
    ));
}

#[tokio::test]
async fn wire_input_cannot_select_the_trusted_host_approval_override() {
    let (core, directory) = fixture("read_file").await;
    let id = thread(&core, &directory).await;
    core.replace_agent_runtime_config(runtime(ToolApprovalLevel::Strict))
        .unwrap();
    let original = input(&id);
    let mut wire = serde_json::to_value(&original).unwrap();
    wire["runtime"] = serde_json::json!({"approval_level":"off"});
    wire["runtimeConfig"] = serde_json::json!({"approvalLevel":"off"});
    wire["approvalLevel"] = serde_json::json!("off");
    let parsed: TurnStartParams = serde_json::from_value(wire).unwrap();
    assert_eq!(parsed, original);
    let (_, mut events) = core.start_turn(parsed).await.unwrap();
    let request = approval(&mut events).await;
    assert_eq!(request.tool_name, "read_file");
    assert!(
        core.respond_tool_approval(ToolApprovalRespondParams {
            approval_id: request.approval_id,
            decision: ApprovalDecision::Denied,
        })
        .await
        .accepted
    );
    let turn = finish(&mut events).await;
    assert_eq!(turn.status, TurnStatus::Completed);
    assert!(matches!(
        &turn.items[2],
        Item::ToolResult { is_error: true, .. }
    ));
    assert_eq!(
        core.agent_runtime_config().unwrap(),
        runtime(ToolApprovalLevel::Strict)
    );
}

#[tokio::test]
async fn local_step_limit_and_approval_remain_fixed_after_global_hot_reload() {
    let (core, directory) = fixture("read_file").await;
    let id = thread(&core, &directory).await;
    let config = AgentRuntimeConfig {
        max_agent_steps: 1,
        ..runtime(ToolApprovalLevel::Off)
    };
    let (_, mut events) = core
        .start_turn_with_runtime(input(&id), None, config)
        .await
        .unwrap();
    core.replace_agent_runtime_config(runtime(ToolApprovalLevel::Strict))
        .unwrap();
    let turn = finish(&mut events).await;
    assert_eq!(turn.status, TurnStatus::Failed);
    assert_eq!(
        turn.error,
        Some(qwenpaw_protocol::ErrorInfo {
            message: String::from("agent exceeded maximum steps"),
        })
    );
    assert_eq!(turn.items.len(), 3);
    assert!(matches!(&turn.items[2], Item::ToolResult {
        is_error: false, content, ..
    } if content == "runtime fixture"));
    assert_eq!(
        core.agent_runtime_config().unwrap(),
        runtime(ToolApprovalLevel::Strict)
    );
}

#[tokio::test]
async fn off_override_cannot_enable_a_disabled_builtin() {
    let (core, directory) = fixture("read_file").await;
    let id = thread(&core, &directory).await;
    core.set_builtin_tool_enabled("read_file", false).unwrap();
    let (_, mut events) = core
        .start_turn_with_runtime(input(&id), None, runtime(ToolApprovalLevel::Off))
        .await
        .unwrap();
    let turn = finish(&mut events).await;
    assert_eq!(turn.status, TurnStatus::Completed);
    assert!(matches!(&turn.items[2], Item::ToolResult {
        is_error: true, content, ..
    } if content == "Tool 'read_file' is disabled"));
    assert_eq!(
        core.agent_runtime_config().unwrap(),
        AgentRuntimeConfig::default()
    );
}

#[tokio::test]
async fn invalid_runtime_overrides_do_not_mutate_history_or_global_settings() {
    let (core, directory) = fixture("read_file").await;
    let id = thread(&core, &directory).await;
    let before = core.backup_snapshot(1024 * 1024).unwrap();
    let mut invalid = Vec::new();
    for max_agent_steps in [0, 501, usize::MAX] {
        invalid.push(AgentRuntimeConfig {
            max_agent_steps,
            ..AgentRuntimeConfig::default()
        });
    }
    for shell_timeout_ms in [0, 999, 600_001, u64::MAX] {
        invalid.push(AgentRuntimeConfig {
            shell_timeout_ms,
            ..AgentRuntimeConfig::default()
        });
    }
    for shell_executable in [
        String::from("shell\nunsafe"),
        String::from("shell\0"),
        "x".repeat(4097),
    ] {
        invalid.push(AgentRuntimeConfig {
            shell_executable,
            ..AgentRuntimeConfig::default()
        });
    }
    for config in invalid {
        let result = core
            .start_turn_with_runtime(input(&id), None, config.clone())
            .await;
        let Err(error) = result else {
            panic!("invalid runtime was accepted");
        };
        assert!(matches!(&error, CoreError::Config(_)));
        assert_eq!(core.replace_agent_runtime_config(config), Err(error));
        assert_eq!(core.backup_snapshot(1024 * 1024).unwrap(), before);
        assert_eq!(
            core.agent_runtime_config().unwrap(),
            AgentRuntimeConfig::default()
        );
    }
}

#[tokio::test]
async fn local_shell_deadline_cancels_the_real_tool_without_changing_global_timeout() {
    let (core, directory) = fixture("long_shell").await;
    let id = thread(&core, &directory).await;
    let original = core.agent_runtime_config().unwrap();
    let config = AgentRuntimeConfig {
        shell_timeout_ms: 1_000,
        ..runtime(ToolApprovalLevel::Off)
    };
    let (_, mut events) = core
        .start_turn_with_runtime(input(&id), None, config)
        .await
        .unwrap();
    let turn = finish(&mut events).await;
    assert_eq!(turn.status, TurnStatus::Completed);
    assert_eq!(turn.items.len(), 4);
    assert!(matches!(&turn.items[2], Item::ToolResult {
        is_error: true, content, ..
    } if content == "Tool execution was cancelled due to timeout."));
    assert_eq!(core.agent_runtime_config().unwrap(), original);
}

#[tokio::test]
async fn cancelling_a_safe_local_turn_clears_approval_and_releases_the_restore_lease() {
    let (core, directory) = fixture("read_file").await;
    let id = thread(&core, &directory).await;
    let original = runtime(ToolApprovalLevel::Off);
    core.replace_agent_runtime_config(original.clone()).unwrap();
    let (started, mut events) = core
        .start_turn_with_runtime(input(&id), None, runtime(ToolApprovalLevel::Strict))
        .await
        .unwrap();
    let request = approval(&mut events).await;
    assert_eq!(request.tool_name, "read_file");
    core.interrupt_turn(&TurnInterruptParams {
        thread_id: id.clone(),
        turn_id: started.turn.id,
    })
    .await
    .unwrap();
    let turn = finish(&mut events).await;
    assert_eq!(turn.status, TurnStatus::Interrupted);
    assert_eq!(turn.items.len(), 2);
    assert!(
        !core
            .respond_tool_approval(ToolApprovalRespondParams {
                approval_id: request.approval_id,
                decision: ApprovalDecision::Approved,
            })
            .await
            .accepted
    );
    let guard = core.begin_restore(Duration::from_secs(2)).await.unwrap();
    assert_eq!(core.agent_runtime_config().unwrap(), original);
    drop(guard);
    let (_, mut resumed) = core.start_turn(input(&id)).await.unwrap();
    assert_eq!(finish(&mut resumed).await.status, TurnStatus::Completed);
}
