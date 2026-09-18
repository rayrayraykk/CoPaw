//! Checkpoint execution fences must not cancel work or stop other Threads.

use super::*;
use pretty_assertions::assert_eq;

async fn complete(core: &Core, id: &str) {
    let (_, mut events) = core.start_turn(input(id)).await.unwrap();
    assert_eq!(finish(&mut events).await.status, TurnStatus::Completed);
}

#[tokio::test]
async fn quiescence_freezes_only_selected_threads_and_survives_checkpoint_replace() {
    let (core, directory) = fixture("read_file").await;
    let selected = thread(&core, &directory).await;
    let other = thread(&core, &directory).await;
    let checkpoint = core.export_thread_checkpoint(&selected).await.unwrap();
    let guard = core
        .quiesce_threads(
            &[selected.clone(), selected.clone()],
            Duration::from_secs(1),
        )
        .await
        .unwrap();
    for replace in [false, true] {
        if replace {
            core.restore_thread_checkpoint(&selected, checkpoint.clone())
                .await
                .unwrap();
        }
        let before = core.read_thread(&selected).await.unwrap();
        assert_eq!(
            core.start_turn(input(&selected)).await.unwrap_err(),
            CoreError::ThreadBusy(selected.clone())
        );
        assert_eq!(core.read_thread(&selected).await.unwrap(), before);
        complete(&core, &other).await;
    }
    drop(guard);
    complete(&core, &selected).await;
}

#[tokio::test]
async fn quiescence_waits_for_approval_without_interrupting_the_turn() {
    let (core, directory) = fixture("read_file").await;
    let id = thread(&core, &directory).await;
    let (_, mut events) = core
        .start_turn_with_runtime(input(&id), None, runtime(ToolApprovalLevel::Strict))
        .await
        .unwrap();
    let request = approval(&mut events).await;
    let before = core.read_thread(&id).await.unwrap();
    let ids = [id.clone()];
    let waiting = core.quiesce_threads(&ids, Duration::from_secs(5));
    tokio::pin!(waiting);
    assert!(
        tokio::time::timeout(Duration::from_millis(20), &mut waiting)
            .await
            .is_err()
    );
    assert_eq!(core.read_thread(&id).await.unwrap(), before);
    assert!(
        core.respond_tool_approval(ToolApprovalRespondParams {
            approval_id: request.approval_id,
            decision: ApprovalDecision::Approved,
        })
        .await
        .accepted
    );
    assert_eq!(finish(&mut events).await.status, TurnStatus::Completed);
    let guard = waiting.await.unwrap();
    assert_eq!(
        core.start_turn(input(&id)).await.unwrap_err(),
        CoreError::ThreadBusy(id.clone())
    );
    drop(guard);
    complete(&core, &id).await;
}

#[tokio::test]
async fn quiescence_timeout_and_cancellation_release_partial_fences() {
    for cancel in [false, true] {
        let (core, directory) = fixture("read_file").await;
        let idle = thread(&core, &directory).await;
        let active = thread(&core, &directory).await;
        assert!(
            idle < active,
            "fixture IDs must acquire the idle gate first"
        );
        let (_, mut events) = core
            .start_turn_with_runtime(input(&active), None, runtime(ToolApprovalLevel::Strict))
            .await
            .unwrap();
        let request = approval(&mut events).await;
        let before = core.read_thread(&active).await.unwrap();
        let ids = [active.clone(), idle.clone()];
        {
            let waiting = core.quiesce_threads(&ids, Duration::from_millis(60));
            tokio::pin!(waiting);
            assert!(
                tokio::time::timeout(Duration::from_millis(10), &mut waiting)
                    .await
                    .is_err()
            );
            assert_eq!(
                core.start_turn(input(&idle)).await.unwrap_err(),
                CoreError::ThreadBusy(idle.clone())
            );
            if !cancel {
                assert!(matches!(waiting.await, Err(CoreError::RestoreTimeout)));
            }
        }
        assert_eq!(core.read_thread(&active).await.unwrap(), before);
        complete(&core, &idle).await;
        assert!(
            core.respond_tool_approval(ToolApprovalRespondParams {
                approval_id: request.approval_id,
                decision: ApprovalDecision::Approved,
            })
            .await
            .accepted
        );
        assert_eq!(finish(&mut events).await.status, TurnStatus::Completed);
    }
}

#[tokio::test]
async fn quiescence_validates_all_ids_and_excludes_global_backup_restore() {
    let (core, directory) = fixture("read_file").await;
    let id = thread(&core, &directory).await;
    assert!(matches!(
        core.quiesce_threads(&[id.clone(), String::from("unknown")], Duration::from_secs(1)).await,
        Err(CoreError::ThreadNotFound(value)) if value == "unknown"
    ));
    complete(&core, &id).await;
    let guard = core
        .quiesce_threads(std::slice::from_ref(&id), Duration::from_secs(1))
        .await
        .unwrap();
    assert!(matches!(
        core.begin_restore(Duration::from_millis(20)).await,
        Err(CoreError::RestoreTimeout)
    ));
    drop(guard);
    let backup = core.begin_restore(Duration::from_secs(1)).await.unwrap();
    assert!(matches!(
        core.quiesce_threads(std::slice::from_ref(&id), Duration::from_secs(1))
            .await,
        Err(CoreError::RestoreBusy)
    ));
    drop(backup);
    complete(&core, &id).await;
}

#[tokio::test]
async fn quiescence_keeps_tracking_producers_after_the_event_consumer_disconnects() {
    let (core, directory) = fixture("read_file").await;
    let id = thread(&core, &directory).await;
    let (_, mut events) = core
        .start_turn_with_runtime(input(&id), None, runtime(ToolApprovalLevel::Strict))
        .await
        .unwrap();
    let request = approval(&mut events).await;
    let before = core.read_thread(&id).await.unwrap();
    drop(events);
    let ids = [id.clone()];
    let waiting = core.quiesce_threads(&ids, Duration::from_secs(5));
    tokio::pin!(waiting);
    assert!(
        tokio::time::timeout(Duration::from_millis(20), &mut waiting)
            .await
            .is_err()
    );
    assert_eq!(core.read_thread(&id).await.unwrap(), before);
    assert!(
        core.respond_tool_approval(ToolApprovalRespondParams {
            approval_id: request.approval_id,
            decision: ApprovalDecision::Approved,
        })
        .await
        .accepted
    );
    let guard = waiting.await.unwrap();
    let finished = core.read_thread(&id).await.unwrap();
    assert_eq!(finished.thread.status, ThreadStatus::Idle);
    assert_eq!(finished.turns[0].status, TurnStatus::Completed);
    drop(guard);
    complete(&core, &id).await;
}

#[tokio::test]
async fn quiescence_overlapping_sets_can_finish_in_opposite_caller_order() {
    let (core, directory) = fixture("read_file").await;
    let first = thread(&core, &directory).await;
    let second = thread(&core, &directory).await;
    let guard = core
        .quiesce_threads(&[first.clone(), second.clone()], Duration::from_secs(1))
        .await
        .unwrap();
    let ids = [second.clone(), first.clone()];
    let waiting = core.quiesce_threads(&ids, Duration::from_secs(1));
    tokio::pin!(waiting);
    assert!(
        tokio::time::timeout(Duration::from_millis(20), &mut waiting)
            .await
            .is_err()
    );
    drop(guard);
    let other = waiting.await.unwrap();
    for id in [&first, &second] {
        assert_eq!(
            core.start_turn(input(id)).await.unwrap_err(),
            CoreError::ThreadBusy(id.clone())
        );
    }
    drop(other);
    complete(&core, &first).await;
    complete(&core, &second).await;
}
