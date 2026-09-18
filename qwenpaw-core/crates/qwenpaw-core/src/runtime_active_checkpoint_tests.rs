//! Active exports preserve the last completed conversation, not partial work.

use super::*;
use base64::Engine as _;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn active_checkpoint_exports_a_complete_boundary_without_interrupting() {
    let (core, directory) = fixture("read_file").await;
    let id = thread(&core, &directory).await;
    for complete_turn in [true, false, true] {
        let before = core.export_thread_checkpoint(&id).await.unwrap();
        std::fs::write(
            directory.path().join("AGENTS.md"),
            format!("Changed prompt: {complete_turn}"),
        )
        .unwrap();
        let (started, mut events) = strict_turn(&core, &id).await;
        let request = approval(&mut events).await;
        let running = core.read_thread(&id).await.unwrap();
        let exported = core.export_thread_checkpoint(&id).await.unwrap();
        assert_eq!(exported, before);
        Core::validate_thread_checkpoint(&exported).unwrap();
        assert_eq!(core.read_thread(&id).await.unwrap(), running);
        assert_eq!(
            core.restore_thread_checkpoint(&id, exported)
                .await
                .unwrap_err(),
            CoreError::ThreadBusy(id.clone())
        );
        if complete_turn {
            assert!(
                core.respond_tool_approval(ToolApprovalRespondParams {
                    approval_id: request.approval_id,
                    decision: ApprovalDecision::Approved,
                })
                .await
                .accepted
            );
        } else {
            core.interrupt_turn(&TurnInterruptParams {
                thread_id: id.clone(),
                turn_id: started.turn.id,
            })
            .await
            .unwrap();
        }
        let finished = finish(&mut events).await;
        assert_eq!(
            finished.status,
            if complete_turn {
                TurnStatus::Completed
            } else {
                TurnStatus::Interrupted
            }
        );
        let after = core.export_thread_checkpoint(&id).await.unwrap();
        Core::validate_thread_checkpoint(&after).unwrap();
        assert_eq!(after.turns.len(), before.turns.len() + 1);
        assert_eq!(after.turns.last(), Some(&finished));
        assert!(after.turn_metadata.last().unwrap().completed_at.is_some());
    }
}

#[tokio::test]
async fn active_checkpoint_after_restore_uses_the_restored_boundary() {
    let (core, directory) = fixture("read_file").await;
    let id = thread(&core, &directory).await;
    let empty = core.export_thread_checkpoint(&id).await.unwrap();
    let (_, mut events) = core.start_turn(input(&id)).await.unwrap();
    assert_eq!(finish(&mut events).await.status, TurnStatus::Completed);
    core.restore_thread_checkpoint(&id, empty).await.unwrap();
    let restored = core.export_thread_checkpoint(&id).await.unwrap();
    assert!(restored.turns.is_empty());
    let (started, mut events) = strict_turn(&core, &id).await;
    approval(&mut events).await;
    assert_eq!(core.export_thread_checkpoint(&id).await.unwrap(), restored);
    core.interrupt_turn(&TurnInterruptParams {
        thread_id: id,
        turn_id: started.turn.id,
    })
    .await
    .unwrap();
    assert_eq!(finish(&mut events).await.status, TurnStatus::Interrupted);
}

#[tokio::test]
async fn active_checkpoint_keeps_a_failed_turn_as_the_next_complete_boundary() {
    let (core, directory) = fixture("read_file").await;
    let id = thread(&core, &directory).await;
    let before = core.export_thread_checkpoint(&id).await.unwrap();
    let router = Router::new().route(
        "/chat/completions",
        post(|| async { (StatusCode::BAD_REQUEST, "fixture rejection") }),
    );
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base_url = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    let (_, mut events) = core
        .start_turn_with_model(
            input(&id),
            Some((
                ModelConfig {
                    api_key: None,
                    base_url,
                    default_model: String::from("rejected"),
                },
                ModelRequestOptions::default(),
            )),
        )
        .await
        .unwrap();
    assert_eq!(core.export_thread_checkpoint(&id).await.unwrap(), before);
    assert_eq!(finish(&mut events).await.status, TurnStatus::Failed);
    let failed = core.export_thread_checkpoint(&id).await.unwrap();
    assert_eq!(failed.turns.len(), 1);
    assert_eq!(failed.turns[0].status, TurnStatus::Failed);
    Core::validate_thread_checkpoint(&failed).unwrap();
    let (started, mut events) = strict_turn(&core, &id).await;
    approval(&mut events).await;
    assert_eq!(core.export_thread_checkpoint(&id).await.unwrap(), failed);
    core.interrupt_turn(&TurnInterruptParams {
        thread_id: id,
        turn_id: started.turn.id,
    })
    .await
    .unwrap();
    assert_eq!(finish(&mut events).await.status, TurnStatus::Interrupted);
}

async fn strict_turn(
    core: &Core,
    id: &str,
) -> (qwenpaw_protocol::TurnStartResponse, TurnEventStream) {
    let base_url = start_tool_model_server(Arc::new(Mutex::new(Vec::new())), "read_file").await;
    core.start_turn_with_runtime(
        input(id),
        Some((
            ModelConfig {
                api_key: None,
                base_url,
                default_model: String::from("fixture"),
            },
            ModelRequestOptions::default(),
        )),
        runtime(ToolApprovalLevel::Strict),
    )
    .await
    .unwrap()
}

#[tokio::test]
async fn active_checkpoint_keeps_images_and_failed_input_does_not_replace_the_boundary() {
    let (core, directory) = fixture("read_file").await;
    let id = thread(&core, &directory).await;
    let image = directory.path().join("image.png");
    let png = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAIAAACQd1PeAAAADElEQVR4nGP4z8AAAAMBAQDJ/pLvAAAAAElFTkSuQmCC";
    std::fs::write(
        &image,
        base64::engine::general_purpose::STANDARD
            .decode(png)
            .unwrap(),
    )
    .unwrap();
    let (_, mut events) = core
        .start_turn(TurnStartParams {
            thread_id: id.clone(),
            input: vec![UserInput::Image {
                path: String::from("image.png"),
            }],
        })
        .await
        .unwrap();
    assert_eq!(finish(&mut events).await.status, TurnStatus::Completed);
    let before = core.export_thread_checkpoint(&id).await.unwrap();
    assert!(serde_json::to_string(&before).unwrap().contains(png));
    std::fs::write(&image, "changed image fixture").unwrap();
    assert!(
        core.start_turn(TurnStartParams {
            thread_id: id.clone(),
            input: vec![UserInput::Image {
                path: String::from("image.png")
            }],
        })
        .await
        .is_err()
    );
    assert_eq!(core.export_thread_checkpoint(&id).await.unwrap(), before);
    let (started, mut events) = strict_turn(&core, &id).await;
    approval(&mut events).await;
    assert_eq!(core.export_thread_checkpoint(&id).await.unwrap(), before);
    Core::validate_thread_checkpoint(&before).unwrap();
    core.interrupt_turn(&TurnInterruptParams {
        thread_id: id.clone(),
        turn_id: started.turn.id,
    })
    .await
    .unwrap();
    assert_eq!(finish(&mut events).await.status, TurnStatus::Interrupted);
    core.restore_thread_checkpoint(&id, before.clone())
        .await
        .unwrap();
    let restored = core.export_thread_checkpoint(&id).await.unwrap();
    assert_eq!(
        (restored.turns, restored.messages, restored.turn_metadata),
        (before.turns, before.messages, before.turn_metadata)
    );
}
