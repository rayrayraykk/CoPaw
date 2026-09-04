use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;

use axum::Router;
use axum::body::Body;
use axum::http::StatusCode;
use axum::http::header::CONTENT_TYPE;
use axum::response::Response;
use axum::routing::post;
use pretty_assertions::assert_eq;
use qwenpaw_protocol::ApprovalDecision;
use qwenpaw_protocol::ConfigWriteParams;
use qwenpaw_protocol::CoreEvent;
use qwenpaw_protocol::Item;
use qwenpaw_protocol::ThreadArchiveParams;
use qwenpaw_protocol::ThreadListParams;
use qwenpaw_protocol::ThreadResumeParams;
use qwenpaw_protocol::ThreadStartParams;
use qwenpaw_protocol::ThreadStatus;
use qwenpaw_protocol::ToolApprovalRespondParams;
use qwenpaw_protocol::TurnInterruptParams;
use qwenpaw_protocol::TurnStartParams;
use qwenpaw_protocol::TurnStatus;
use qwenpaw_protocol::UserInput;
use qwenpaw_protocol::WorkspaceInfo;
use tokio::net::TcpListener;
use tokio::sync::Mutex;

use super::*;
use crate::runtime::compose_user_input;

#[tokio::test]
async fn completes_a_streaming_turn_and_persists_history() {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let base_url = start_model_server(Arc::clone(&requests)).await;
    let directory = tempfile::tempdir().expect("temporary directory should be created");
    let database_path = directory.path().join("threads.sqlite3");
    let model_config = ModelConfig {
        api_key: Some(String::from("test-key")),
        base_url,
        default_model: String::from("qwen-test"),
    };
    let core = Core::persistent(model_config.clone(), &database_path)
        .expect("persistent core should open");
    let started = core
        .start_thread(ThreadStartParams {
            model: None,
            workspace_root: Some(directory.path().to_string_lossy().into_owned()),
        })
        .await
        .expect("thread should start");
    let thread_id = started.thread.id.clone();
    let (turn_response, mut events) = core
        .start_turn(TurnStartParams {
            thread_id: thread_id.clone(),
            input: vec![UserInput::Text {
                text: String::from("Say hello"),
            }],
        })
        .await
        .expect("turn should start");

    let mut received = Vec::new();
    while let Some(event) = events.recv().await {
        let completed = matches!(event, CoreEvent::TurnCompleted(_));
        received.push(event);
        if completed {
            break;
        }
    }

    let read = core
        .read_thread(&thread_id)
        .await
        .expect("thread should be readable");
    let agent_item_id = received
        .iter()
        .find_map(|event| match event {
            CoreEvent::ItemStarted(notification) => Some(notification.item.id().to_owned()),
            _ => None,
        })
        .expect("agent item should start");
    assert!(read.thread.updated_at >= started.thread.updated_at);
    assert_eq!(
        read.thread,
        qwenpaw_protocol::Thread {
            status: ThreadStatus::Idle,
            updated_at: read.thread.updated_at,
            ..started.thread
        }
    );
    assert_eq!(
        read.turns,
        vec![qwenpaw_protocol::Turn {
            id: turn_response.turn.id,
            thread_id,
            status: TurnStatus::Completed,
            items: vec![
                turn_response.turn.items[0].clone(),
                Item::AgentMessage {
                    id: agent_item_id,
                    text: String::from("Hello from QwenPaw"),
                },
            ],
            error: None,
        }]
    );
    assert_eq!(requests.lock().await.len(), 1);

    drop(core);
    let reopened =
        Core::persistent(model_config, &database_path).expect("persistent core should reopen");
    assert_eq!(
        reopened
            .read_thread(&read.thread.id)
            .await
            .expect("reopened thread should be readable"),
        read
    );
}

#[tokio::test]
async fn rejects_turn_input_over_the_runtime_limit() {
    let directory = tempfile::tempdir().expect("temporary directory should be created");
    let core = Core::new(ModelConfig {
        api_key: None,
        base_url: String::from("http://127.0.0.1:1"),
        default_model: String::from("qwen-test"),
    });
    let started = core
        .start_thread(ThreadStartParams {
            model: None,
            workspace_root: Some(directory.path().to_string_lossy().into_owned()),
        })
        .await
        .expect("thread should start");

    let error = core
        .start_turn(TurnStartParams {
            thread_id: started.thread.id,
            input: vec![UserInput::Text {
                text: "x".repeat(262_145),
            }],
        })
        .await
        .expect_err("oversized input should be rejected");

    assert_eq!(
        error,
        CoreError::InputTooLarge {
            actual_bytes: 262_145,
            max_bytes: 262_144,
        }
    );
}

#[test]
fn composes_workspace_file_references_without_reading_contents() {
    let directory = tempfile::tempdir().expect("temporary directory should be created");
    std::fs::create_dir(directory.path().join("src")).expect("src should be created");
    std::fs::write(directory.path().join("src/lib.rs"), [0xff, 0xfe])
        .expect("binary fixture should be written");

    assert_eq!(
        compose_user_input(
            &[
                UserInput::Text {
                    text: String::from("Review this code"),
                },
                UserInput::FileReference {
                    path: directory
                        .path()
                        .join("src/lib.rs")
                        .to_string_lossy()
                        .into_owned(),
                    start_line: Some(10),
                    end_line: Some(20),
                },
            ],
            Some(&directory.path().to_string_lossy()),
        )
        .expect("reference should compose"),
        concat!(
            "Review this code\n\n",
            "Workspace file references (contents are not included; use read_file when needed):\n",
            "[{\"endLine\":20,\"path\":\"src/lib.rs\",\"startLine\":10}]"
        )
    );
}

#[test]
fn accepts_a_file_reference_as_the_only_user_input() {
    let directory = tempfile::tempdir().expect("temporary directory should be created");
    std::fs::write(directory.path().join("README.md"), "readme")
        .expect("fixture should be written");

    assert_eq!(
        compose_user_input(
            &[UserInput::FileReference {
                path: String::from("README.md"),
                start_line: None,
                end_line: None,
            }],
            Some(&directory.path().to_string_lossy()),
        )
        .expect("reference-only input should compose"),
        concat!(
            "Workspace file references (contents are not included; use read_file when needed):\n",
            "[{\"path\":\"README.md\"}]"
        )
    );
}

#[test]
fn rejects_invalid_file_reference_ranges_and_missing_workspaces() {
    let directory = tempfile::tempdir().expect("temporary directory should be created");
    std::fs::write(directory.path().join("lib.rs"), "source").expect("fixture should be written");
    assert_eq!(
        compose_user_input(
            &[UserInput::FileReference {
                path: String::from("lib.rs"),
                start_line: Some(20),
                end_line: Some(10),
            }],
            Some(&directory.path().to_string_lossy()),
        ),
        Err(CoreError::FileReference(String::from(
            "line range must contain 1-based startLine and endLine with startLine <= endLine"
        )))
    );
    assert_eq!(
        compose_user_input(
            &[UserInput::FileReference {
                path: String::from("src/lib.rs"),
                start_line: None,
                end_line: None,
            }],
            None,
        ),
        Err(CoreError::FileReference(String::from(
            "file references require a Thread with a Workspace Root"
        )))
    );
}

#[test]
fn rejects_outside_and_excessive_file_references() {
    let parent = tempfile::tempdir().expect("temporary directory should be created");
    let workspace = parent.path().join("workspace");
    std::fs::create_dir(&workspace).expect("workspace should be created");
    let outside = parent.path().join("outside.rs");
    std::fs::write(&outside, "secret").expect("outside fixture should be written");
    assert_eq!(
        compose_user_input(
            &[UserInput::FileReference {
                path: outside.to_string_lossy().into_owned(),
                start_line: None,
                end_line: None,
            }],
            Some(&workspace.to_string_lossy()),
        ),
        Err(CoreError::FileReference(format!(
            "path is outside the workspace: {}",
            outside
                .canonicalize()
                .expect("outside fixture should canonicalize")
                .display()
        )))
    );
    assert_eq!(
        compose_user_input(
            &vec![
                UserInput::FileReference {
                    path: String::from("unused"),
                    start_line: None,
                    end_line: None,
                };
                33
            ],
            Some(&workspace.to_string_lossy()),
        ),
        Err(CoreError::FileReference(String::from(
            "received 33 references, exceeding the 32-reference limit"
        )))
    );
}

#[tokio::test]
async fn archives_persists_and_resumes_a_thread() {
    let directory = tempfile::tempdir().expect("temporary directory should be created");
    let database_path = directory.path().join("threads.sqlite3");
    let model_config = ModelConfig {
        api_key: None,
        base_url: String::from("http://127.0.0.1:1"),
        default_model: String::from("qwen-test"),
    };
    let core = Core::persistent(model_config.clone(), &database_path)
        .expect("persistent core should open");
    let started = core
        .start_thread(ThreadStartParams {
            model: None,
            workspace_root: Some(directory.path().to_string_lossy().into_owned()),
        })
        .await
        .expect("thread should start");
    let archived = core
        .archive_thread(&ThreadArchiveParams {
            thread_id: started.thread.id.clone(),
        })
        .await
        .expect("thread should archive");
    assert!(archived.thread.archived);
    assert_eq!(
        core.list_threads(ThreadListParams::default()).await.data,
        Vec::new()
    );
    assert_eq!(
        core.list_threads(ThreadListParams {
            include_archived: true,
            ..ThreadListParams::default()
        })
        .await
        .data,
        vec![archived.thread.clone()]
    );
    assert_eq!(
        core.start_turn(TurnStartParams {
            thread_id: started.thread.id.clone(),
            input: vec![UserInput::Text {
                text: String::from("hello"),
            }],
        })
        .await
        .expect_err("archived thread should reject turns"),
        CoreError::ThreadArchived(started.thread.id.clone())
    );

    drop(core);
    let reopened =
        Core::persistent(model_config, &database_path).expect("persistent core should reopen");
    assert!(
        reopened
            .read_thread(&started.thread.id)
            .await
            .expect("archived thread should remain readable")
            .thread
            .archived
    );
    let resumed = reopened
        .resume_thread(&ThreadResumeParams {
            thread_id: started.thread.id,
        })
        .await
        .expect("thread should resume");
    assert!(!resumed.thread.archived);
    assert_eq!(
        reopened
            .list_threads(ThreadListParams::default())
            .await
            .data,
        vec![resumed.thread]
    );
}

#[tokio::test]
async fn deletes_a_thread_from_runtime_and_persistent_storage() {
    let directory = tempfile::tempdir().expect("temporary directory should be created");
    let database_path = directory.path().join("threads.sqlite3");
    let model_config = ModelConfig {
        api_key: None,
        base_url: String::from("http://127.0.0.1:1"),
        default_model: String::from("qwen-test"),
    };
    let core = Core::persistent(model_config.clone(), &database_path)
        .expect("persistent core should open");
    let started = core
        .start_thread(ThreadStartParams {
            model: None,
            workspace_root: Some(directory.path().to_string_lossy().into_owned()),
        })
        .await
        .expect("thread should start");

    assert_eq!(
        core.delete_thread(&started.thread.id)
            .await
            .expect("thread should delete"),
        started.thread
    );
    assert_eq!(
        core.read_thread(&started.thread.id)
            .await
            .expect_err("deleted thread should be absent"),
        CoreError::ThreadNotFound(started.thread.id.clone())
    );

    drop(core);
    let reopened =
        Core::persistent(model_config, &database_path).expect("persistent core should reopen");
    assert_eq!(
        reopened
            .list_threads(ThreadListParams {
                include_archived: true,
                ..ThreadListParams::default()
            })
            .await
            .data,
        Vec::new()
    );
}

#[test]
fn persists_and_hot_reloads_non_secret_model_configuration() {
    let directory = tempfile::tempdir().expect("temporary directory should be created");
    let database_path = directory.path().join("threads.sqlite3");
    let core = Core::persistent(
        ModelConfig {
            api_key: Some(String::from("secret-key")),
            base_url: String::from("https://bootstrap.test/v1"),
            default_model: String::from("qwen-bootstrap"),
        },
        &database_path,
    )
    .expect("persistent core should open");

    let written = core
        .write_config(ConfigWriteParams {
            base_url: Some(String::from(" https://configured.test/v1/ ")),
            default_model: Some(String::from(" qwen-configured ")),
        })
        .expect("configuration should update");
    assert_eq!(
        written.config,
        qwenpaw_protocol::CoreConfig {
            base_url: String::from("https://configured.test/v1"),
            default_model: String::from("qwen-configured"),
            api_key_configured: true,
        }
    );
    assert_eq!(core.read_config().config, written.config);
    assert_eq!(core.list_models().data[0].id, "qwen-configured");
    core.set_runtime_api_key(None)
        .expect("runtime API key should clear");
    assert!(!core.read_config().config.api_key_configured);
    core.set_runtime_api_key(Some(String::from("replacement-secret")))
        .expect("runtime API key should update");
    assert!(core.read_config().config.api_key_configured);
    assert_eq!(
        core.set_runtime_api_key(Some(String::from("invalid\nsecret")))
            .expect_err("control characters should be rejected"),
        CoreError::Config(String::from(
            "API key must contain 1 through 8192 bytes without control characters"
        ))
    );
    assert!(core.read_config().config.api_key_configured);

    drop(core);
    let reopened = Core::persistent(
        ModelConfig {
            api_key: None,
            base_url: String::from("https://ignored.test/v1"),
            default_model: String::from("qwen-ignored"),
        },
        &database_path,
    )
    .expect("persistent Core should reload settings");
    assert_eq!(
        reopened.read_config().config,
        qwenpaw_protocol::CoreConfig {
            base_url: String::from("https://configured.test/v1"),
            default_model: String::from("qwen-configured"),
            api_key_configured: false,
        }
    );
    assert_eq!(
        reopened
            .write_config(ConfigWriteParams {
                base_url: Some(String::from("https://user:secret@example.test/v1")),
                default_model: None,
            })
            .expect_err("embedded credentials should be rejected"),
        CoreError::Config(String::from(
            "base URL must not contain embedded credentials"
        ))
    );
}

#[test]
fn persists_desktop_coding_mode_in_the_core_store() {
    let directory = tempfile::tempdir().expect("temporary directory should be created");
    let database_path = directory.path().join("threads.sqlite3");
    let model_config = ModelConfig {
        api_key: None,
        base_url: String::from("http://127.0.0.1:1"),
        default_model: String::from("qwen-test"),
    };
    let core = Core::persistent(model_config.clone(), &database_path)
        .expect("persistent Core should open");
    assert!(!core.read_coding_mode().expect("Coding Mode should read"));
    assert!(
        core.write_coding_mode(true)
            .expect("Coding Mode should persist")
    );
    drop(core);

    let reopened =
        Core::persistent(model_config, &database_path).expect("persistent Core should reopen");
    assert!(
        reopened
            .read_coding_mode()
            .expect("Coding Mode should reload")
    );
    assert!(
        !reopened
            .write_coding_mode(false)
            .expect("Coding Mode should update")
    );
    assert!(
        !reopened
            .read_coding_mode()
            .expect("updated Coding Mode should read")
    );
}

#[test]
fn persists_and_validates_the_desktop_ui_language() {
    let directory = tempfile::tempdir().expect("temporary directory should be created");
    let database_path = directory.path().join("threads.sqlite3");
    let model_config = ModelConfig {
        api_key: None,
        base_url: String::from("http://127.0.0.1:1"),
        default_model: String::from("qwen-test"),
    };
    let core = Core::persistent(model_config.clone(), &database_path)
        .expect("persistent Core should open");
    assert_eq!(
        core.read_ui_language()
            .expect("default UI language should read"),
        "en"
    );
    assert_eq!(
        core.write_ui_language(" pt-BR ")
            .expect("supported UI language should persist"),
        "pt-BR"
    );
    assert_eq!(
        core.write_ui_language("vi")
            .expect("Console Vietnamese option should persist"),
        "vi"
    );
    assert_eq!(
        core.write_ui_language("en-US")
            .expect_err("unsupported UI language should be rejected"),
        CoreError::Config(String::from(
            "UI language must be one of: en, zh, ja, ru, pt-BR, id, vi"
        ))
    );
    drop(core);

    let reopened =
        Core::persistent(model_config, &database_path).expect("persistent Core should reopen");
    assert_eq!(
        reopened
            .read_ui_language()
            .expect("persisted UI language should reload"),
        "vi"
    );
}

#[test]
fn persists_environment_names_without_persisting_values() {
    let directory = tempfile::tempdir().expect("temporary directory should be created");
    let database_path = directory.path().join("threads.sqlite3");
    let model_config = ModelConfig {
        api_key: None,
        base_url: String::from("http://127.0.0.1:1"),
        default_model: String::from("qwen-test"),
    };
    let core = Core::persistent(model_config.clone(), &database_path)
        .expect("persistent Core should open");

    assert_eq!(
        core.write_environment_keys(&[String::from("SECOND_VALUE"), String::from("FIRST_VALUE"),])
            .expect("environment keys should persist"),
        vec![String::from("FIRST_VALUE"), String::from("SECOND_VALUE")]
    );
    core.replace_runtime_environment(BTreeMap::from([(
        String::from("FIRST_VALUE"),
        String::from("secret-value"),
    )]))
    .expect("runtime environment should update");
    drop(core);

    let database = std::fs::read(&database_path).expect("database should be readable");
    assert!(!String::from_utf8_lossy(&database).contains("secret-value"));
    let reopened =
        Core::persistent(model_config, &database_path).expect("persistent Core should reopen");
    assert_eq!(
        reopened
            .read_environment_keys()
            .expect("environment keys should reload"),
        vec![String::from("FIRST_VALUE"), String::from("SECOND_VALUE")]
    );
    assert_eq!(
        reopened
            .write_environment_keys(&[String::from("INVALID-NAME")])
            .expect_err("invalid environment name should be rejected"),
        CoreError::Config(String::from(
            "environment variable name is invalid: INVALID-NAME"
        ))
    );
}

#[tokio::test]
async fn lists_and_reads_only_registered_workspaces() {
    let directory = tempfile::tempdir().expect("temporary directory should be created");
    let core = Core::new(ModelConfig {
        api_key: None,
        base_url: String::from("http://127.0.0.1:1"),
        default_model: String::from("qwen-test"),
    });
    let first = core
        .start_thread(ThreadStartParams {
            model: None,
            workspace_root: Some(directory.path().to_string_lossy().into_owned()),
        })
        .await
        .expect("first thread should start");
    let second = core
        .start_thread(ThreadStartParams {
            model: None,
            workspace_root: Some(directory.path().to_string_lossy().into_owned()),
        })
        .await
        .expect("second thread should start");
    let archived = core
        .archive_thread(&ThreadArchiveParams {
            thread_id: second.thread.id,
        })
        .await
        .expect("second thread should archive");
    let root = first
        .thread
        .workspace_root
        .expect("thread should have a workspace");
    let expected = WorkspaceInfo {
        root: root.clone(),
        thread_count: 2,
        archived_thread_count: 1,
        updated_at: archived.thread.updated_at.max(first.thread.updated_at),
    };

    assert_eq!(core.list_workspaces().await.data, vec![expected.clone()]);
    assert_eq!(
        core.read_workspace(&root)
            .await
            .expect("registered workspace should read")
            .workspace,
        expected
    );
    assert_eq!(
        core.read_workspace("/not/registered")
            .await
            .expect_err("unknown workspace should fail"),
        CoreError::WorkspaceNotFound(String::from("/not/registered"))
    );
}

#[tokio::test]
async fn rebinds_and_persists_an_idle_thread_workspace() {
    let directory = tempfile::tempdir().expect("temporary directory should be created");
    let first_workspace = directory.path().join("first");
    let second_workspace = directory.path().join("second");
    std::fs::create_dir_all(&first_workspace).expect("first workspace should be created");
    std::fs::create_dir_all(&second_workspace).expect("second workspace should be created");
    let database_path = directory.path().join("threads.sqlite3");
    let config = ModelConfig {
        api_key: None,
        base_url: String::from("http://127.0.0.1:1"),
        default_model: String::from("qwen-test"),
    };
    let core =
        Core::persistent(config.clone(), &database_path).expect("persistent Core should open");
    let started = core
        .start_thread(ThreadStartParams {
            model: None,
            workspace_root: Some(first_workspace.to_string_lossy().into_owned()),
        })
        .await
        .expect("thread should start");
    let rebound = core
        .set_thread_workspace(&started.thread.id, &second_workspace)
        .await
        .expect("idle thread should rebind");
    assert_eq!(
        rebound.workspace_root,
        Some(
            second_workspace
                .canonicalize()
                .expect("second workspace should resolve")
                .to_string_lossy()
                .into_owned()
        )
    );
    assert_eq!(
        core.write_preferred_workspace(&second_workspace)
            .expect("preferred Workspace should persist"),
        second_workspace
            .canonicalize()
            .expect("preferred Workspace should resolve")
            .to_string_lossy()
    );
    drop(core);

    let reopened = Core::persistent(config, &database_path).expect("persistent Core should reopen");
    assert_eq!(
        reopened
            .read_thread(&started.thread.id)
            .await
            .expect("rebound thread should persist")
            .thread,
        rebound
    );
    assert_eq!(
        reopened
            .read_preferred_workspace()
            .expect("preferred Workspace should reload"),
        rebound.workspace_root
    );
}

#[tokio::test]
async fn reads_a_workspace_file_through_the_agent_loop() {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let base_url = start_tool_model_server(Arc::clone(&requests), "read_file").await;
    let directory = tempfile::tempdir().expect("temporary directory should be created");
    std::fs::write(directory.path().join("notes.txt"), "workspace secret")
        .expect("workspace fixture should be written");
    let core = Core::new(ModelConfig {
        api_key: None,
        base_url,
        default_model: String::from("qwen-test"),
    });
    let started = core
        .start_thread(ThreadStartParams {
            model: None,
            workspace_root: Some(directory.path().to_string_lossy().into_owned()),
        })
        .await
        .expect("thread should start");
    let (turn, mut events) = core
        .start_turn(TurnStartParams {
            thread_id: started.thread.id.clone(),
            input: vec![UserInput::Text {
                text: String::from("Read notes.txt"),
            }],
        })
        .await
        .expect("turn should start");

    while let Some(event) = events.recv().await {
        if matches!(event, CoreEvent::TurnCompleted(_)) {
            break;
        }
    }

    let read = core
        .read_thread(&started.thread.id)
        .await
        .expect("thread should be readable");
    assert_eq!(read.turns[0].status, TurnStatus::Completed);
    assert_eq!(read.turns[0].items.len(), 4);
    assert!(matches!(
        &read.turns[0].items[1],
        Item::ToolCall { name, arguments, .. }
            if name == "read_file" && arguments == "{\"path\":\"notes.txt\"}"
    ));
    assert!(matches!(
        &read.turns[0].items[2],
        Item::ToolResult { content, is_error: false, .. }
            if content == "workspace secret"
    ));
    assert!(matches!(
        &read.turns[0].items[3],
        Item::AgentMessage { text, .. } if text == "File says workspace secret"
    ));
    assert_eq!(read.turns[0].id, turn.turn.id);
    let requests = requests.lock().await;
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0]["tool_choice"], serde_json::json!("auto"));
    assert_eq!(requests[0]["tools"].as_array().map(Vec::len), Some(6));
    assert_eq!(
        requests[1]["messages"][3],
        serde_json::json!({
            "role": "tool",
            "content": "workspace secret",
            "tool_call_id": "call_read"
        })
    );
}

#[tokio::test]
async fn waits_for_shell_approval_before_execution() {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let base_url = start_tool_model_server(Arc::clone(&requests), "shell").await;
    let directory = tempfile::tempdir().expect("temporary directory should be created");
    let core = Core::new(ModelConfig {
        api_key: None,
        base_url,
        default_model: String::from("qwen-test"),
    });
    let started = core
        .start_thread(ThreadStartParams {
            model: None,
            workspace_root: Some(directory.path().to_string_lossy().into_owned()),
        })
        .await
        .expect("thread should start");
    let (_, mut events) = core
        .start_turn(TurnStartParams {
            thread_id: started.thread.id.clone(),
            input: vec![UserInput::Text {
                text: String::from("Run a command"),
            }],
        })
        .await
        .expect("turn should start");
    let mut requested = false;
    let mut resolved = false;
    while let Some(event) = events.recv().await {
        match event {
            CoreEvent::ToolApprovalRequested(notification) => {
                requested = true;
                assert_eq!(notification.tool_name, "shell");
                assert_eq!(notification.arguments, "{\"command\":\"echo approved\"}");
                assert!(
                    core.respond_tool_approval(ToolApprovalRespondParams {
                        approval_id: notification.approval_id,
                        decision: ApprovalDecision::Approved,
                    })
                    .await
                    .accepted
                );
            }
            CoreEvent::ToolApprovalResolved(notification) => {
                resolved = notification.decision == ApprovalDecision::Approved;
            }
            CoreEvent::TurnCompleted(_) => break,
            _ => {}
        }
    }

    let read = core
        .read_thread(&started.thread.id)
        .await
        .expect("thread should be readable");
    assert!(requested);
    assert!(resolved);
    assert_eq!(read.turns[0].status, TurnStatus::Completed);
    assert!(matches!(
        &read.turns[0].items[2],
        Item::ToolResult { content, is_error: false, .. }
            if content.contains("approved")
    ));
}

#[tokio::test]
async fn strict_agent_runtime_requires_approval_for_read_tools() {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let base_url = start_tool_model_server(Arc::clone(&requests), "read_file").await;
    let directory = tempfile::tempdir().expect("temporary directory should be created");
    std::fs::write(directory.path().join("notes.txt"), "strict read")
        .expect("fixture should be written");
    let core = Core::new(ModelConfig {
        api_key: None,
        base_url,
        default_model: String::from("qwen-test"),
    });
    core.replace_agent_runtime_config(AgentRuntimeConfig {
        approval_level: ToolApprovalLevel::Strict,
        ..AgentRuntimeConfig::default()
    })
    .expect("strict runtime config should apply");
    let started = core
        .start_thread(ThreadStartParams {
            model: None,
            workspace_root: Some(directory.path().to_string_lossy().into_owned()),
        })
        .await
        .expect("thread should start");
    let (_, mut events) = core
        .start_turn(TurnStartParams {
            thread_id: started.thread.id.clone(),
            input: vec![UserInput::Text {
                text: String::from("Read the file"),
            }],
        })
        .await
        .expect("turn should start");
    let mut requested = false;
    while let Some(event) = events.recv().await {
        match event {
            CoreEvent::ToolApprovalRequested(notification) => {
                requested = true;
                assert_eq!(notification.tool_name, "read_file");
                assert!(
                    core.respond_tool_approval(ToolApprovalRespondParams {
                        approval_id: notification.approval_id,
                        decision: ApprovalDecision::Approved,
                    })
                    .await
                    .accepted
                );
            }
            CoreEvent::TurnCompleted(notification) => {
                assert_eq!(notification.turn.status, TurnStatus::Completed);
                break;
            }
            _ => {}
        }
    }
    assert!(requested);
}

#[tokio::test]
async fn off_agent_runtime_executes_guarded_tools_without_approval() {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let base_url = start_tool_model_server(Arc::clone(&requests), "shell").await;
    let directory = tempfile::tempdir().expect("temporary directory should be created");
    let core = Core::new(ModelConfig {
        api_key: None,
        base_url,
        default_model: String::from("qwen-test"),
    });
    core.replace_agent_runtime_config(AgentRuntimeConfig {
        approval_level: ToolApprovalLevel::Off,
        ..AgentRuntimeConfig::default()
    })
    .expect("off runtime config should apply");
    let started = core
        .start_thread(ThreadStartParams {
            model: None,
            workspace_root: Some(directory.path().to_string_lossy().into_owned()),
        })
        .await
        .expect("thread should start");
    let (_, mut events) = core
        .start_turn(TurnStartParams {
            thread_id: started.thread.id.clone(),
            input: vec![UserInput::Text {
                text: String::from("Run without approval"),
            }],
        })
        .await
        .expect("turn should start");
    let mut requested = false;
    while let Some(event) = events.recv().await {
        match event {
            CoreEvent::ToolApprovalRequested(_) => requested = true,
            CoreEvent::TurnCompleted(notification) => {
                assert_eq!(notification.turn.status, TurnStatus::Completed);
                break;
            }
            _ => {}
        }
    }
    assert!(!requested);
    let read = core
        .read_thread(&started.thread.id)
        .await
        .expect("thread should be readable");
    assert!(matches!(
        &read.turns[0].items[2],
        Item::ToolResult { content, is_error: false, .. }
            if content.contains("approved")
    ));
}

#[tokio::test]
async fn returns_a_denied_shell_result_to_the_model() {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let base_url = start_tool_model_server(Arc::clone(&requests), "shell").await;
    let directory = tempfile::tempdir().expect("temporary directory should be created");
    let core = Core::new(ModelConfig {
        api_key: None,
        base_url,
        default_model: String::from("qwen-test"),
    });
    let started = core
        .start_thread(ThreadStartParams {
            model: None,
            workspace_root: Some(directory.path().to_string_lossy().into_owned()),
        })
        .await
        .expect("thread should start");
    let (_, mut events) = core
        .start_turn(TurnStartParams {
            thread_id: started.thread.id.clone(),
            input: vec![UserInput::Text {
                text: String::from("Do not run the command"),
            }],
        })
        .await
        .expect("turn should start");
    while let Some(event) = events.recv().await {
        match event {
            CoreEvent::ToolApprovalRequested(notification) => {
                assert!(
                    core.respond_tool_approval(ToolApprovalRespondParams {
                        approval_id: notification.approval_id,
                        decision: ApprovalDecision::Denied,
                    })
                    .await
                    .accepted
                );
            }
            CoreEvent::TurnCompleted(_) => break,
            _ => {}
        }
    }

    let read = core
        .read_thread(&started.thread.id)
        .await
        .expect("thread should be readable");
    assert_eq!(read.turns[0].status, TurnStatus::Completed);
    assert!(matches!(
        &read.turns[0].items[2],
        Item::ToolResult { content, is_error: true, .. }
            if content == "Tool execution was denied by the user."
    ));
    assert_eq!(
        requests.lock().await[1]["messages"][3]["content"],
        serde_json::json!("Tool execution was denied by the user.")
    );
}

#[tokio::test]
async fn interrupts_a_turn_waiting_for_shell_approval() {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let base_url = start_tool_model_server(Arc::clone(&requests), "shell").await;
    let directory = tempfile::tempdir().expect("temporary directory should be created");
    let core = Core::new(ModelConfig {
        api_key: None,
        base_url,
        default_model: String::from("qwen-test"),
    });
    let started = core
        .start_thread(ThreadStartParams {
            model: None,
            workspace_root: Some(directory.path().to_string_lossy().into_owned()),
        })
        .await
        .expect("thread should start");
    let (turn, mut events) = core
        .start_turn(TurnStartParams {
            thread_id: started.thread.id.clone(),
            input: vec![UserInput::Text {
                text: String::from("Cancel the command"),
            }],
        })
        .await
        .expect("turn should start");
    let completed = loop {
        match events.recv().await.expect("turn should emit events") {
            CoreEvent::ToolApprovalRequested(_) => {
                assert!(
                    core.interrupt_turn(&TurnInterruptParams {
                        thread_id: started.thread.id.clone(),
                        turn_id: turn.turn.id.clone(),
                    })
                    .await
                    .expect("turn should be interruptible")
                    .accepted
                );
            }
            CoreEvent::TurnCompleted(notification) => break notification.turn,
            _ => {}
        }
    };

    assert_eq!(completed.status, TurnStatus::Interrupted);
    assert_eq!(requests.lock().await.len(), 1);
}

#[tokio::test]
async fn interrupts_a_running_shell_tool_without_waiting_for_exit() {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let base_url = start_tool_model_server(Arc::clone(&requests), "long_shell").await;
    let directory = tempfile::tempdir().expect("temporary directory should be created");
    let core = Core::new(ModelConfig {
        api_key: None,
        base_url,
        default_model: String::from("qwen-test"),
    });
    let started = core
        .start_thread(ThreadStartParams {
            model: None,
            workspace_root: Some(directory.path().to_string_lossy().into_owned()),
        })
        .await
        .expect("thread should start");
    let (turn, mut events) = core
        .start_turn(TurnStartParams {
            thread_id: started.thread.id.clone(),
            input: vec![UserInput::Text {
                text: String::from("Run a long command"),
            }],
        })
        .await
        .expect("turn should start");
    let started_at = Instant::now();
    let completed = tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            match events.recv().await.expect("turn should emit events") {
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
                    tokio::time::sleep(Duration::from_millis(150)).await;
                    assert!(
                        core.interrupt_turn(&TurnInterruptParams {
                            thread_id: started.thread.id.clone(),
                            turn_id: turn.turn.id.clone(),
                        })
                        .await
                        .expect("running turn should be interruptible")
                        .accepted
                    );
                }
                CoreEvent::TurnCompleted(notification) => break notification.turn,
                _ => {}
            }
        }
    })
    .await
    .expect("interrupted shell should not wait for its natural exit");

    assert_eq!(completed.status, TurnStatus::Interrupted);
    assert!(started_at.elapsed() < Duration::from_secs(3));
    assert_eq!(requests.lock().await.len(), 1);
}

#[tokio::test]
async fn interrupts_a_model_request_waiting_for_response_headers() {
    let requests = Arc::new(Mutex::new(0_usize));
    let base_url = start_delayed_model_server(Arc::clone(&requests)).await;
    let directory = tempfile::tempdir().expect("temporary directory should be created");
    let core = Core::new(ModelConfig {
        api_key: None,
        base_url,
        default_model: String::from("qwen-test"),
    });
    let started = core
        .start_thread(ThreadStartParams {
            model: None,
            workspace_root: Some(directory.path().to_string_lossy().into_owned()),
        })
        .await
        .expect("thread should start");
    let (turn, mut events) = core
        .start_turn(TurnStartParams {
            thread_id: started.thread.id.clone(),
            input: vec![UserInput::Text {
                text: String::from("Wait for the model"),
            }],
        })
        .await
        .expect("turn should start");
    while *requests.lock().await == 0 {
        tokio::task::yield_now().await;
    }
    assert!(
        core.interrupt_turn(&TurnInterruptParams {
            thread_id: started.thread.id.clone(),
            turn_id: turn.turn.id.clone(),
        })
        .await
        .expect("waiting turn should be interruptible")
        .accepted
    );
    let completed = tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if let CoreEvent::TurnCompleted(notification) =
                events.recv().await.expect("turn should emit completion")
            {
                break notification.turn;
            }
        }
    })
    .await
    .expect("interrupted request should not wait for model response");

    assert_eq!(completed.status, TurnStatus::Interrupted);
}

#[tokio::test]
async fn persists_a_rate_limited_model_turn_as_failed() {
    let router = Router::new().route(
        "/chat/completions",
        post(|| async { (StatusCode::TOO_MANY_REQUESTS, "rate limited") }),
    );
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("model listener should bind");
    let address = listener
        .local_addr()
        .expect("model listener should have an address");
    tokio::spawn(async move {
        axum::serve(listener, router)
            .await
            .expect("model server should run");
    });
    let directory = tempfile::tempdir().expect("temporary directory should be created");
    let core = Core::new(ModelConfig {
        api_key: None,
        base_url: format!("http://{address}"),
        default_model: String::from("qwen-test"),
    });
    let started = core
        .start_thread(ThreadStartParams {
            model: None,
            workspace_root: Some(directory.path().to_string_lossy().into_owned()),
        })
        .await
        .expect("thread should start");
    let (_, mut events) = core
        .start_turn(TurnStartParams {
            thread_id: started.thread.id.clone(),
            input: vec![UserInput::Text {
                text: String::from("Trigger rate limiting"),
            }],
        })
        .await
        .expect("turn should start");
    let completed = loop {
        if let CoreEvent::TurnCompleted(notification) =
            events.recv().await.expect("turn should complete")
        {
            break notification.turn;
        }
    };

    assert_eq!(completed.status, TurnStatus::Failed);
    assert_eq!(
        completed.error,
        Some(qwenpaw_protocol::ErrorInfo {
            message: String::from("model returned HTTP 429: rate limited"),
        })
    );
    assert_eq!(
        core.read_thread(&started.thread.id)
            .await
            .expect("failed turn should persist"),
        qwenpaw_protocol::ThreadReadResponse {
            thread: qwenpaw_protocol::Thread {
                status: ThreadStatus::Error,
                ..started.thread
            },
            turns: vec![completed],
        }
    );
}

#[tokio::test]
async fn discovers_searches_and_edits_through_the_agent_loop() {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let base_url = start_coding_model_server(Arc::clone(&requests)).await;
    let directory = tempfile::tempdir().expect("temporary directory should be created");
    std::fs::create_dir(directory.path().join("src")).expect("src should be created");
    std::fs::write(
        directory.path().join("src/lib.rs"),
        "pub const MODE: &str = \"old\";\n",
    )
    .expect("source should be written");
    let core = Core::new(ModelConfig {
        api_key: None,
        base_url,
        default_model: String::from("qwen-test"),
    });
    let started = core
        .start_thread(ThreadStartParams {
            model: None,
            workspace_root: Some(directory.path().to_string_lossy().into_owned()),
        })
        .await
        .expect("thread should start");
    let (_, mut events) = core
        .start_turn(TurnStartParams {
            thread_id: started.thread.id.clone(),
            input: vec![UserInput::Text {
                text: String::from("Change MODE from old to new"),
            }],
        })
        .await
        .expect("turn should start");
    let mut approved_replace = false;
    while let Some(event) = events.recv().await {
        match event {
            CoreEvent::ToolApprovalRequested(notification) => {
                assert_eq!(notification.tool_name, "replace_text");
                approved_replace = core
                    .respond_tool_approval(ToolApprovalRespondParams {
                        approval_id: notification.approval_id,
                        decision: ApprovalDecision::Approved,
                    })
                    .await
                    .accepted;
            }
            CoreEvent::TurnCompleted(notification) => {
                assert_eq!(notification.turn.status, TurnStatus::Completed);
                break;
            }
            _ => {}
        }
    }

    assert!(approved_replace);
    assert_eq!(
        std::fs::read_to_string(directory.path().join("src/lib.rs"))
            .expect("edited source should be readable"),
        "pub const MODE: &str = \"new\";\n"
    );
    let read = core
        .read_thread(&started.thread.id)
        .await
        .expect("thread should be readable");
    assert_eq!(read.turns[0].items.len(), 8);
    assert!(matches!(
        &read.turns[0].items[1],
        Item::ToolCall { name, .. } if name == "list_files"
    ));
    assert!(matches!(
        &read.turns[0].items[3],
        Item::ToolCall { name, .. } if name == "search_text"
    ));
    assert!(matches!(
        &read.turns[0].items[5],
        Item::ToolCall { name, .. } if name == "replace_text"
    ));
    assert!(matches!(
        &read.turns[0].items[7],
        Item::AgentMessage { text, .. } if text == "Updated MODE to new"
    ));
    assert_eq!(requests.lock().await.len(), 4);
}

#[tokio::test]
async fn composes_persists_and_hot_reloads_workspace_system_prompt_files() {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let base_url = start_model_server(Arc::clone(&requests)).await;
    let directory = tempfile::tempdir().expect("temporary directory should be created");
    let database_path = directory.path().join("threads.sqlite3");
    std::fs::write(
        directory.path().join("AGENTS.md"),
        concat!(
            "---\nname: test\n---\nAgent body\n",
            "<!-- heartbeat:start -->heartbeat hidden<!-- heartbeat:end -->\n",
            "<!-- memory:start -->memory hidden<!-- memory:end -->\n",
        ),
    )
    .expect("AGENTS.md should be written");
    std::fs::write(directory.path().join("SOUL.md"), "Original soul")
        .expect("SOUL.md should be written");
    std::fs::write(directory.path().join("PROFILE.md"), "Profile body")
        .expect("PROFILE.md should be written");
    let config = ModelConfig {
        api_key: Some(String::from("test-key")),
        base_url,
        default_model: String::from("qwen-test"),
    };
    let core =
        Core::persistent(config.clone(), &database_path).expect("persistent Core should open");
    assert_eq!(
        core.system_prompt_files()
            .expect("system prompt files should load"),
        vec!["AGENTS.md", "SOUL.md", "PROFILE.md"]
    );
    core.replace_system_prompt_files(vec![String::from("SOUL.md"), String::from("AGENTS.md")])
        .expect("system prompt files should update");
    let started = core
        .start_thread(ThreadStartParams {
            model: None,
            workspace_root: Some(directory.path().to_string_lossy().into_owned()),
        })
        .await
        .expect("thread should start");
    let (_, mut first_events) = core
        .start_turn(TurnStartParams {
            thread_id: started.thread.id.clone(),
            input: vec![UserInput::Text {
                text: String::from("first"),
            }],
        })
        .await
        .expect("first turn should start");
    while let Some(event) = first_events.recv().await {
        if matches!(event, CoreEvent::TurnCompleted(_)) {
            break;
        }
    }
    let expected_first = concat!(
        "# SOUL.md\n\nOriginal soul\n\n",
        "# AGENTS.md\n\nAgent body"
    );
    assert_eq!(
        requests.lock().await[0]["messages"][0],
        serde_json::json!({"role": "system", "content": expected_first})
    );

    std::fs::write(directory.path().join("PROFILE.md"), "Updated profile")
        .expect("PROFILE.md should update");
    core.replace_system_prompt_files(vec![String::from("PROFILE.md")])
        .expect("system prompt files should hot update");
    let (_, mut second_events) = core
        .start_turn(TurnStartParams {
            thread_id: started.thread.id,
            input: vec![UserInput::Text {
                text: String::from("second"),
            }],
        })
        .await
        .expect("second turn should start");
    while let Some(event) = second_events.recv().await {
        if matches!(event, CoreEvent::TurnCompleted(_)) {
            break;
        }
    }
    assert_eq!(
        requests.lock().await[1]["messages"][0],
        serde_json::json!({
            "role": "system",
            "content": "# PROFILE.md\n\nUpdated profile"
        })
    );
    drop(core);

    let reopened = Core::persistent(config, &database_path).expect("persistent Core should reopen");
    assert_eq!(
        reopened
            .system_prompt_files()
            .expect("system prompt files should reload"),
        vec!["PROFILE.md"]
    );
}

#[test]
fn rejects_unsafe_or_duplicate_system_prompt_files() {
    let core = Core::new(ModelConfig {
        api_key: None,
        base_url: String::from("http://127.0.0.1:1"),
        default_model: String::from("qwen-test"),
    });
    for files in [
        vec![String::from("../SOUL.md")],
        vec![String::from("nested/SOUL.md")],
        vec![String::from("SOUL.txt")],
        vec![String::from("SOUL.md"), String::from("SOUL.md")],
    ] {
        assert!(core.replace_system_prompt_files(files).is_err());
    }
    assert!(
        core.replace_system_prompt_files(
            (0..=64)
                .map(|index| format!("PROFILE-{index}.md"))
                .collect(),
        )
        .is_err()
    );
    assert_eq!(
        core.system_prompt_files()
            .expect("failed updates must preserve defaults"),
        vec!["AGENTS.md", "SOUL.md", "PROFILE.md"]
    );
}

async fn start_model_server(requests: Arc<Mutex<Vec<serde_json::Value>>>) -> String {
    let app = Router::new().route(
        "/chat/completions",
        post(move |body: axum::Json<serde_json::Value>| {
            let requests = Arc::clone(&requests);
            async move {
                requests.lock().await.push(body.0);
                Response::builder()
                    .status(StatusCode::OK)
                    .header(CONTENT_TYPE, "text/event-stream")
                    .body(Body::from(concat!(
                        "data: {\"choices\":[{\"delta\":{\"content\":\"Hello \"}}]}\n\n",
                        "data: {\"choices\":[{\"delta\":{\"content\":\"from QwenPaw\"}}]}\n\n",
                        "data: [DONE]\n\n"
                    )))
                    .expect("mock response should build")
            }
        }),
    );
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("mock listener should bind");
    let address = listener
        .local_addr()
        .expect("mock listener should have an address");
    tokio::spawn(async move {
        axum::serve(listener, app)
            .await
            .expect("mock server should run");
    });
    format!("http://{address}")
}

async fn start_delayed_model_server(requests: Arc<Mutex<usize>>) -> String {
    let app = Router::new().route(
        "/chat/completions",
        post(move || {
            let requests = Arc::clone(&requests);
            async move {
                *requests.lock().await += 1;
                tokio::time::sleep(Duration::from_secs(30)).await;
                Response::builder()
                    .status(StatusCode::OK)
                    .header(CONTENT_TYPE, "text/event-stream")
                    .body(Body::from("data: [DONE]\n\n"))
                    .expect("mock response should build")
            }
        }),
    );
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("mock listener should bind");
    let address = listener
        .local_addr()
        .expect("mock listener should have an address");
    tokio::spawn(async move {
        axum::serve(listener, app)
            .await
            .expect("mock server should run");
    });
    format!("http://{address}")
}

async fn start_tool_model_server(
    requests: Arc<Mutex<Vec<serde_json::Value>>>,
    tool: &'static str,
) -> String {
    let app = Router::new().route(
        "/chat/completions",
        post(move |body: axum::Json<serde_json::Value>| {
            let requests = Arc::clone(&requests);
            async move {
                let mut requests = requests.lock().await;
                requests.push(body.0);
                let response = if requests.len() == 1 {
                    match tool {
                        "read_file" => String::from(concat!(
                            "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_read\",\"function\":{\"name\":\"read_file\",\"arguments\":\"{\\\"path\\\":\"}}]}}]}\n\n",
                            "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"\\\"notes.txt\\\"}\"}}]}}]}\n\n",
                            "data: [DONE]\n\n"
                        )),
                        "shell" => String::from(concat!(
                            "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_shell\",\"function\":{\"name\":\"shell\",\"arguments\":\"{\\\"command\\\":\\\"echo approved\\\"}\"}}]}}]}\n\n",
                            "data: [DONE]\n\n"
                        )),
                        "long_shell" => tool_call_response(
                            "call_long_shell",
                            "shell",
                            &serde_json::json!({"command": LONG_SHELL_COMMAND}),
                        ),
                        _ => unreachable!("test tool should be known"),
                    }
                } else if tool == "read_file" {
                    String::from(concat!(
                        "data: {\"choices\":[{\"delta\":{\"content\":\"File says workspace secret\"}}]}\n\n",
                        "data: [DONE]\n\n"
                    ))
                } else {
                    String::from(concat!(
                        "data: {\"choices\":[{\"delta\":{\"content\":\"Command completed\"}}]}\n\n",
                        "data: [DONE]\n\n"
                    ))
                };
                Response::builder()
                    .status(StatusCode::OK)
                    .header(CONTENT_TYPE, "text/event-stream")
                    .body(Body::from(response))
                    .expect("mock response should build")
            }
        }),
    );
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("mock listener should bind");
    let address = listener
        .local_addr()
        .expect("mock listener should have an address");
    tokio::spawn(async move {
        axum::serve(listener, app)
            .await
            .expect("mock server should run");
    });
    format!("http://{address}")
}

async fn start_coding_model_server(requests: Arc<Mutex<Vec<serde_json::Value>>>) -> String {
    let app = Router::new().route(
        "/chat/completions",
        post(move |body: axum::Json<serde_json::Value>| {
            let requests = Arc::clone(&requests);
            async move {
                let mut requests = requests.lock().await;
                requests.push(body.0);
                let response = match requests.len() {
                    1 => tool_call_response("call_list", "list_files", &serde_json::json!({})),
                    2 => tool_call_response(
                        "call_search",
                        "search_text",
                        &serde_json::json!({"query": "MODE", "path": "src"}),
                    ),
                    3 => tool_call_response(
                        "call_replace",
                        "replace_text",
                        &serde_json::json!({
                            "path": "src/lib.rs",
                            "oldText": "pub const MODE: &str = \"old\";",
                            "newText": "pub const MODE: &str = \"new\";"
                        }),
                    ),
                    4 => text_response("Updated MODE to new"),
                    _ => panic!("coding model should finish in four requests"),
                };
                Response::builder()
                    .status(StatusCode::OK)
                    .header(CONTENT_TYPE, "text/event-stream")
                    .body(Body::from(response))
                    .expect("mock response should build")
            }
        }),
    );
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("mock listener should bind");
    let address = listener
        .local_addr()
        .expect("mock listener should have an address");
    tokio::spawn(async move {
        axum::serve(listener, app)
            .await
            .expect("mock server should run");
    });
    format!("http://{address}")
}

fn tool_call_response(id: &str, name: &str, arguments: &serde_json::Value) -> String {
    let arguments = serde_json::to_string(arguments).expect("arguments should serialize");
    let chunk = serde_json::json!({
        "choices": [{
            "delta": {
                "tool_calls": [{
                    "index": 0,
                    "id": id,
                    "function": {"name": name, "arguments": arguments}
                }]
            }
        }]
    });
    format!("data: {chunk}\n\ndata: [DONE]\n\n")
}

fn text_response(content: &str) -> String {
    let chunk = serde_json::json!({"choices": [{"delta": {"content": content}}]});
    format!("data: {chunk}\n\ndata: [DONE]\n\n")
}

#[cfg(windows)]
const LONG_SHELL_COMMAND: &str = "ping -n 30 127.0.0.1 >NUL";

#[cfg(not(windows))]
const LONG_SHELL_COMMAND: &str = "sleep 30";
