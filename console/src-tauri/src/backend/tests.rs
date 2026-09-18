use super::*;

#[test]
fn closed_process_clears_the_ready_port() {
    let state = BackendState::default();
    let generation = state.next_generation();
    state.set_port_if_current(generation, 54321);
    state.set_error_if_current(generation, "process exited".into());

    state.clear_child_if_current(generation);

    assert_eq!(state.port(), None);
    assert_eq!(state.error(), Some("process exited".into()));
}

#[test]
fn old_process_events_leave_the_new_generation_untouched() {
    let state = BackendState::default();
    let old = state.next_generation();
    let current = state.next_generation();
    state.set_port_if_current(current, 54321);

    state.set_port_if_current(old, 12345);
    state.set_error_if_current(old, "old process exited".into());
    state.clear_child_if_current(old);

    assert_eq!((state.port(), state.error()), (Some(54321), None));
}

#[test]
fn stream_loss_preserves_unconfirmed_process_state() {
    let state = BackendState::default();
    let generation = state.next_generation();
    let (_sender, receiver) = watch::channel(false);
    state.with_inner(|inner| {
        inner.port = Some(54321);
        inner.shutdown_token = Some("test-token".into());
        inner.terminated = Some(receiver);
    });

    state.clear_child_if_current(generation);

    assert_eq!(state.port(), None);
    assert_eq!(
        state.error(),
        Some("backend event stream closed without confirmed termination".into())
    );
    state.with_inner(|inner| {
        assert_eq!(inner.shutdown_token.as_deref(), Some("test-token"));
        assert!(inner.terminated.is_some());
    });
}

#[test]
fn concurrent_restarts_wait_for_each_preceding_process() {
    tauri::async_runtime::block_on(async {
        let state = BackendState::default();
        let (old_sender, old_receiver) = watch::channel(false);
        let (new_sender, new_receiver) = watch::channel(false);
        state.with_inner(|inner| inner.terminated = Some(old_receiver));
        let starts = std::cell::Cell::new(0);
        let mut first = std::pin::pin!(state.restart(|| {
            starts.set(starts.get() + 1);
            state.with_inner(|inner| inner.terminated = Some(new_receiver));
        }));
        let mut second = std::pin::pin!(state.restart(|| starts.set(starts.get() + 1)));

        assert!(futures_util::poll!(&mut first).is_pending());
        assert!(futures_util::poll!(&mut second).is_pending());
        assert_eq!(starts.get(), 0);
        old_sender.send_replace(true);
        assert_eq!(first.await, Ok(()));
        assert!(futures_util::poll!(&mut second).is_pending());
        assert_eq!(starts.get(), 1);
        new_sender.send_replace(true);
        assert_eq!(second.await, Ok(()));
        assert_eq!(starts.get(), 2);
    });
}

#[test]
fn shutdown_prevents_inflight_and_queued_restarts_from_starting() {
    tauri::async_runtime::block_on(async {
        let state = BackendState::default();
        let (sender, receiver) = watch::channel(false);
        state.with_inner(|inner| inner.terminated = Some(receiver));
        let starts = std::cell::Cell::new(0);
        let mut first = std::pin::pin!(state.restart(|| starts.set(starts.get() + 1)));
        let mut queued = std::pin::pin!(state.restart(|| starts.set(starts.get() + 1)));
        let mut shutdown = std::pin::pin!(state.shutdown());

        assert!(futures_util::poll!(&mut first).is_pending());
        assert!(futures_util::poll!(&mut queued).is_pending());
        assert!(futures_util::poll!(&mut shutdown).is_pending());
        sender.send_replace(true);
        let expected = Err("Desktop is exiting; backend restart is unavailable".into());
        assert_eq!(first.await, expected);
        assert_eq!(queued.await, expected);
        assert_eq!(shutdown.await, Ok(()));
        assert_eq!(
            state.restart(|| starts.set(starts.get() + 1)).await,
            expected
        );
        assert_eq!(state.shutdown().await, Ok(()));
        assert_eq!(starts.get(), 0);
    });
}

#[test]
fn unconfirmed_termination_never_allows_a_replacement() {
    tauri::async_runtime::block_on(async {
        let state = BackendState::default();
        let (sender, receiver) = watch::channel(false);
        state.with_inner(|inner| inner.terminated = Some(receiver));
        drop(sender);
        let starts = std::cell::Cell::new(0);
        let expected = Err(concat!(
            "backend process ended without a termination event; ",
            "failed to confirm forced backend termination: ",
            "backend process ended without a termination event"
        )
        .into());

        for _ in 0..2 {
            assert_eq!(
                state.restart(|| starts.set(starts.get() + 1)).await,
                expected
            );
            assert!(state.with_inner(|inner| inner.terminated.is_some()));
        }
        assert_eq!(starts.get(), 0);
    });
}

#[test]
fn restart_reports_a_spawn_failure_and_allows_retry() {
    tauri::async_runtime::block_on(async {
        let state = BackendState::default();
        assert_eq!(
            state
                .restart(|| state.set_error("spawn failed".into()))
                .await,
            Err("spawn failed".into())
        );
        assert_eq!(state.restart(|| state.clear_startup_state()).await, Ok(()));
        assert_eq!((state.port(), state.error()), (None, None));
    });
}
