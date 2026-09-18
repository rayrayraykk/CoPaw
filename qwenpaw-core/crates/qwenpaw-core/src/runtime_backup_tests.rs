use pretty_assertions::assert_eq;

use super::*;

const BACKUP_LIMIT: u64 = 1024 * 1024;

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn restored_mcp_environment_rebinds_real_http_connections_and_rolls_back() {
    use axum::Json;
    use axum::http::HeaderMap;
    use axum::http::StatusCode;
    use axum::http::Uri;
    use axum::response::IntoResponse;
    use serde_json::json;

    async fn handler(
        uri: Uri,
        headers: HeaderMap,
        Json(request): Json<serde_json::Value>,
    ) -> axum::response::Response {
        let Some(id) = request.get("id") else {
            return StatusCode::ACCEPTED.into_response();
        };
        let result = match request["method"].as_str().unwrap() {
            "initialize" => {
                json!({"protocolVersion": "2025-03-26", "capabilities": {"tools": {}}, "serverInfo": {"name": "fixture", "version": "1"}})
            }
            "tools/list" => {
                json!({"tools": [{"name": "echo", "description": format!("{}|{}", uri.path(), headers["authorization"].to_str().unwrap()), "inputSchema": {"type": "object"}}]})
            }
            method => panic!("unexpected fixture method: {method}"),
        };
        Json(json!({"jsonrpc": "2.0", "id": id, "result": result})).into_response()
    }

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let stop = tokio_util::sync::CancellationToken::new();
    let shutdown = stop.clone();
    let server = tokio::spawn(async move {
        let router = axum::Router::new()
            .route("/original", axum::routing::post(handler))
            .route("/restored", axum::routing::post(handler));
        axum::serve(listener, router)
            .with_graceful_shutdown(shutdown.cancelled_owned())
            .await
            .unwrap();
    });
    let original = BTreeMap::from([
        (
            String::from("QWENPAW_MCP_RESTORE_URL"),
            format!("{base}/original"),
        ),
        (
            String::from("QWENPAW_MCP_RESTORE_TOKEN"),
            String::from("original-token"),
        ),
    ]);
    let restored = BTreeMap::from([
        (
            String::from("QWENPAW_MCP_RESTORE_URL"),
            format!("{base}/restored"),
        ),
        (
            String::from("QWENPAW_MCP_RESTORE_TOKEN"),
            String::from("restored-token"),
        ),
    ]);
    let core = Core::new(model_config());
    core.replace_runtime_environment(original.clone()).unwrap();
    let settings = serde_json::from_value(json!([{
        "key": "remote", "enabled": true, "transport": "streamable_http",
        "url": "${QWENPAW_MCP_RESTORE_URL}", "headers": {"Authorization": "Bearer ${QWENPAW_MCP_RESTORE_TOKEN}"}
    }])).unwrap();
    core.replace_mcp_client_settings(settings).unwrap();
    assert_eq!(
        core.list_mcp_tools("remote").await.unwrap()[0].description,
        "/original|Bearer original-token"
    );
    let snapshot = core.backup_snapshot(BACKUP_LIMIT).unwrap();
    let mut lease = core.begin_restore(Duration::from_secs(2)).await.unwrap();
    let rollback = lease.capture_rollback(BACKUP_LIMIT).unwrap();
    let candidate = lease.prepare_restore(&snapshot).unwrap();
    candidate
        .replace_runtime_environment(restored.clone())
        .unwrap();
    candidate
        .validate_mcp_client_bindings(candidate.mcp_client_settings())
        .unwrap();
    assert_eq!(core.runtime_environment().unwrap(), original);
    lease.apply(&candidate, BACKUP_LIMIT).await.unwrap();
    drop(lease);
    assert_eq!(
        core.list_mcp_tools("remote").await.unwrap()[0].description,
        "/restored|Bearer restored-token"
    );
    // Reconfiguring clients must not restore an older environment snapshot.
    core.replace_mcp_client_settings(core.mcp_client_settings())
        .unwrap();
    assert_eq!(core.runtime_environment().unwrap(), restored);
    assert_eq!(
        core.list_mcp_tools("remote").await.unwrap()[0].description,
        "/restored|Bearer restored-token"
    );
    let mut lease = core.begin_restore(Duration::from_secs(2)).await.unwrap();
    lease.apply(&rollback, BACKUP_LIMIT).await.unwrap();
    drop(lease);
    assert_eq!(core.runtime_environment().unwrap(), original);
    assert_eq!(
        core.list_mcp_tools("remote").await.unwrap()[0].description,
        "/original|Bearer original-token"
    );
    stop.cancel();
    tokio::time::timeout(Duration::from_secs(2), server)
        .await
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn exclusive_lease_can_rebuild_a_candidate_from_the_latest_quiescent_snapshot() {
    let core = Core::new(model_config());
    let operation = core.operation_guard().unwrap();
    let source = core.clone();
    let waiting = tokio::spawn(async move { source.begin_restore(Duration::from_secs(2)).await });
    core.write_ui_language("zh").unwrap();
    drop(operation);
    let mut guard = waiting.await.unwrap().unwrap();
    let latest = core.backup_snapshot(BACKUP_LIMIT).unwrap();
    assert!(matches!(
        core.prepare_restore(&latest),
        Err(CoreError::RestoreBusy)
    ));
    let candidate = guard.prepare_restore(&latest).unwrap();
    assert_eq!(candidate.read_ui_language().unwrap(), "zh");
    assert_eq!(candidate.backup_snapshot(BACKUP_LIMIT).unwrap(), latest);
    let mut invalid = latest.clone();
    invalid
        .settings
        .insert(String::from("tool_offload_policy"), String::from("invalid"));
    assert!(guard.prepare_restore(&invalid).is_err());
    assert_eq!(core.backup_snapshot(BACKUP_LIMIT).unwrap(), latest);
    guard.apply(&candidate, BACKUP_LIMIT).await.unwrap();
    assert_eq!(core.backup_snapshot(BACKUP_LIMIT).unwrap(), latest);
}

fn model_config() -> ModelConfig {
    ModelConfig {
        api_key: None,
        base_url: String::from("http://127.0.0.1:1/v1"),
        default_model: String::from("original-model"),
    }
}

#[tokio::test]
async fn restores_durable_and_live_state_and_can_roll_back_before_releasing_lease() {
    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("core.sqlite3");
    let core = Core::persistent(model_config(), &database).unwrap();
    let original = core.backup_snapshot(BACKUP_LIMIT).unwrap();
    let rollback = core.prepare_restore(&original).unwrap();
    let candidate = core.prepare_restore(&original).unwrap();
    candidate
        .write_config(ConfigWriteParams {
            base_url: Some(String::from("http://127.0.0.1:2/v1")),
            default_model: Some(String::from("restored-model")),
        })
        .unwrap();
    candidate.write_ui_language("zh").unwrap();
    candidate
        .set_runtime_api_key(Some(String::from("restored-key")))
        .unwrap();
    candidate.set_tool_offload_policy("offload").unwrap();
    candidate.set_builtin_tool_enabled("shell", false).unwrap();
    candidate
        .replace_system_prompt_files(vec![String::from("PROFILE.md")])
        .unwrap();
    let mut security = candidate.security_settings().unwrap();
    security.tool_guard.denied_tools = vec![String::from("write_file")];
    candidate
        .replace_security_settings(security.clone())
        .unwrap();
    let agent = AgentRuntimeConfig {
        max_agent_steps: 12,
        ..AgentRuntimeConfig::default()
    };
    candidate
        .replace_agent_runtime_config(agent.clone())
        .unwrap();
    candidate
        .replace_runtime_environment(BTreeMap::from([(
            String::from("RESTORED_VALUE"),
            String::from("value"),
        )]))
        .unwrap();
    let thread = candidate
        .start_thread(ThreadStartParams {
            model: None,
            workspace_root: Some(directory.path().to_string_lossy().into_owned()),
        })
        .await
        .unwrap()
        .thread;
    let expected = candidate.backup_snapshot(BACKUP_LIMIT).unwrap();
    let mut guard = core.begin_restore(Duration::from_secs(1)).await.unwrap();
    guard.apply(&candidate, BACKUP_LIMIT).await.unwrap();
    assert_eq!(core.backup_snapshot(BACKUP_LIMIT).unwrap(), expected);
    assert_eq!(core.read_thread(&thread.id).await.unwrap().thread, thread);
    assert_eq!(core.read_config(), candidate.read_config());
    assert_eq!(core.security_settings().unwrap(), security);
    assert_eq!(core.agent_runtime_config().unwrap(), agent);
    assert_eq!(
        core.builtin_tools().unwrap(),
        candidate.builtin_tools().unwrap()
    );
    assert_eq!(
        core.system_prompt_files().unwrap(),
        vec![String::from("PROFILE.md")]
    );
    assert_eq!(core.tool_offload_policy(), "offload");
    assert_eq!(core.write_ui_language("en"), Err(CoreError::RestoreBusy));

    guard.apply(&rollback, BACKUP_LIMIT).await.unwrap();
    assert_eq!(core.backup_snapshot(BACKUP_LIMIT).unwrap(), original);
    assert_eq!(
        core.agent_runtime_config().unwrap(),
        AgentRuntimeConfig::default()
    );
    assert_eq!(core.read_config(), rollback.read_config());
    guard.apply(&candidate, BACKUP_LIMIT).await.unwrap();
    drop(guard);
    let reopened = Core::persistent(model_config(), &database).unwrap();
    assert_eq!(reopened.backup_snapshot(BACKUP_LIMIT).unwrap(), expected);
    assert_eq!(reopened.security_settings().unwrap(), security);
    assert_eq!(
        reopened.builtin_tools().unwrap(),
        candidate.builtin_tools().unwrap()
    );
    assert_eq!(reopened.tool_offload_policy(), "offload");
    assert_eq!(reopened.read_ui_language().unwrap(), "zh");
}

#[tokio::test]
async fn invalid_or_busy_candidates_leave_original_data_unchanged() {
    let core = Core::new(model_config());
    core.write_ui_language("zh").unwrap();
    let original = core.backup_snapshot(BACKUP_LIMIT).unwrap();
    let mut invalid = original.clone();
    invalid
        .settings
        .insert(String::from("tool_offload_policy"), String::from("invalid"));
    assert!(matches!(
        core.prepare_restore(&invalid),
        Err(CoreError::Config(_))
    ));
    assert_eq!(core.backup_snapshot(BACKUP_LIMIT).unwrap(), original);
    let candidate = core.prepare_restore(&original).unwrap();
    let operation = candidate.operation_guard().unwrap();
    let mut guard = core.begin_restore(Duration::from_secs(1)).await.unwrap();
    assert_eq!(
        guard.apply(&candidate, BACKUP_LIMIT).await,
        Err(CoreError::RestoreBusy)
    );
    assert_eq!(core.backup_snapshot(BACKUP_LIMIT).unwrap(), original);
    drop(operation);
    assert!(matches!(
        guard.apply(&candidate, 1).await,
        Err(CoreError::Storage(_))
    ));
    assert_eq!(core.backup_snapshot(BACKUP_LIMIT).unwrap(), original);
    assert!(matches!(
        guard.apply(&core, BACKUP_LIMIT).await,
        Err(CoreError::Config(_))
    ));
    assert_eq!(core.backup_snapshot(BACKUP_LIMIT).unwrap(), original);
    drop(guard);
    assert_eq!(core.write_ui_language("en").unwrap(), "en");
}

#[tokio::test]
async fn draining_allows_nested_writes_and_timeout_or_cancellation_releases_the_gate() {
    let core = Core::new(model_config());
    let operation = core.operation_guard().unwrap();
    let detached_operation = operation.clone();
    drop(operation);
    let waiting_core = core.clone();
    let waiting =
        tokio::spawn(async move { waiting_core.begin_restore(Duration::from_millis(40)).await });
    tokio::task::yield_now().await;
    assert_eq!(core.write_ui_language("zh").unwrap(), "zh");
    assert!(matches!(
        waiting.await.unwrap(),
        Err(CoreError::RestoreTimeout)
    ));
    let original = core.backup_snapshot(BACKUP_LIMIT).unwrap();
    assert!(
        tokio::time::timeout(
            Duration::from_millis(20),
            core.begin_restore(Duration::from_secs(10)),
        )
        .await
        .is_err()
    );
    assert_eq!(core.backup_snapshot(BACKUP_LIMIT).unwrap(), original);
    drop(detached_operation);
    let guard = core.begin_restore(Duration::from_secs(1)).await.unwrap();
    assert!(matches!(
        core.operation_guard(),
        Err(CoreError::RestoreBusy)
    ));
    assert_eq!(core.write_coding_mode(true), Err(CoreError::RestoreBusy));
    assert_eq!(core.set_runtime_api_key(None), Err(CoreError::RestoreBusy));
    drop(guard);
    assert_eq!(core.write_ui_language("en").unwrap(), "en");
}

#[tokio::test]
async fn restore_cancels_active_turn_and_prevents_old_history_from_reappearing() {
    let requests = Arc::new(Mutex::new(0));
    let base_url = start_delayed_model_server(Arc::clone(&requests)).await;
    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("core.sqlite3");
    let config = ModelConfig {
        base_url,
        ..model_config()
    };
    let core = Core::persistent(config.clone(), &database).unwrap();
    let original = core.backup_snapshot(BACKUP_LIMIT).unwrap();
    let candidate = core.prepare_restore(&original).unwrap();
    let thread = core
        .start_thread(ThreadStartParams {
            model: None,
            workspace_root: Some(directory.path().to_string_lossy().into_owned()),
        })
        .await
        .unwrap()
        .thread;
    let (_, mut events) = core
        .start_turn(TurnStartParams {
            thread_id: thread.id.clone(),
            input: vec![UserInput::Text {
                text: String::from("wait for cancellation"),
            }],
        })
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(2), async {
        while *requests.lock().await == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let interrupted_snapshot = core.backup_snapshot(BACKUP_LIMIT).unwrap();
    let recovered = core.prepare_restore(&interrupted_snapshot).unwrap();
    assert_eq!(
        recovered.read_thread(&thread.id).await.unwrap().turns[0].status,
        TurnStatus::Interrupted
    );
    let mut guard = core.begin_restore(Duration::from_secs(2)).await.unwrap();
    let mut completed = None;
    while let Some(event) = events.recv().await {
        if let CoreEvent::TurnCompleted(notification) = event {
            completed = Some(notification.turn.status);
        }
    }
    assert_eq!(completed, Some(TurnStatus::Interrupted));
    guard.apply(&candidate, BACKUP_LIMIT).await.unwrap();
    drop(guard);
    assert_eq!(
        core.read_thread(&thread.id).await,
        Err(CoreError::ThreadNotFound(thread.id))
    );
    assert_eq!(core.backup_snapshot(BACKUP_LIMIT).unwrap(), original);
    let reopened = Core::persistent(config, &database).unwrap();
    assert_eq!(reopened.backup_snapshot(BACKUP_LIMIT).unwrap(), original);
}

#[tokio::test]
async fn restore_waits_for_offloaded_shell_after_its_turn_has_completed() {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let base_url = start_tool_model_server(requests, "long_shell").await;
    let directory = tempfile::tempdir().unwrap();
    let core = Core::new(ModelConfig {
        base_url,
        ..model_config()
    });
    let original = core.backup_snapshot(BACKUP_LIMIT).unwrap();
    let candidate = core.prepare_restore(&original).unwrap();
    let thread = core
        .start_thread(ThreadStartParams {
            model: None,
            workspace_root: Some(directory.path().to_string_lossy().into_owned()),
        })
        .await
        .unwrap()
        .thread;
    let (_, mut events) = core
        .start_turn(TurnStartParams {
            thread_id: thread.id.clone(),
            input: vec![UserInput::Text {
                text: String::from("run in background"),
            }],
        })
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        while let Some(event) = events.recv().await {
            match event {
                CoreEvent::ToolApprovalRequested(notification) => {
                    assert!(
                        core.respond_tool_approval(ToolApprovalRespondParams {
                            approval_id: notification.approval_id,
                            decision: ApprovalDecision::Approved,
                        })
                        .await
                        .accepted
                    );
                }
                CoreEvent::ToolApprovalResolved(_) => {
                    while core.tool_call(&thread.id, "call_long_shell").await.is_err() {
                        tokio::task::yield_now().await;
                    }
                    core.offload_tool_call(&thread.id, "call_long_shell")
                        .await
                        .unwrap();
                }
                CoreEvent::TurnCompleted(notification) => {
                    assert_eq!(notification.turn.status, TurnStatus::Completed);
                    break;
                }
                _ => {}
            }
        }
    })
    .await
    .unwrap();
    let mut subscription = core
        .subscribe_tool_call(&thread.id, "call_long_shell")
        .await
        .unwrap();
    assert_eq!(subscription.snapshot.status, "offloaded");
    let mut guard = core.begin_restore(Duration::from_secs(3)).await.unwrap();
    assert_eq!(
        core.tool_call(&thread.id, "call_long_shell")
            .await
            .unwrap()
            .end_state
            .as_deref(),
        Some("interrupted")
    );
    assert_eq!(
        subscription.events.recv().await,
        Some(ToolCallStreamEvent::Chunk(serde_json::json!({
            "type": "text", "text": "Tool execution was cancelled by the user."
        })))
    );
    assert_eq!(
        subscription.events.recv().await,
        Some(ToolCallStreamEvent::Done)
    );
    guard.apply(&candidate, BACKUP_LIMIT).await.unwrap();
    assert_eq!(
        core.tool_call(&thread.id, "call_long_shell").await,
        Err(ToolCallControlError::NotFound)
    );
    assert_eq!(core.backup_snapshot(BACKUP_LIMIT).unwrap(), original);
}
