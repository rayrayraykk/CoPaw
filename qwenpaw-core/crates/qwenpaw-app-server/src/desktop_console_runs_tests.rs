use super::*;
use pretty_assertions::assert_eq;
use qwenpaw_core::{Core, ModelConfig};
use qwenpaw_protocol::{ThreadStartParams, ThreadStatus, TurnStartParams, TurnStatus, UserInput};

#[tokio::test]
async fn cancellation_drains_real_core_while_the_sse_buffer_is_full() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let core = Core::new(ModelConfig {
        api_key: None,
        base_url: format!("http://{}/v1", listener.local_addr().unwrap()),
        default_model: String::from("fixture"),
    });
    let (seen, received) = tokio::sync::oneshot::channel();
    let model = tokio::spawn(async move {
        let (_socket, _) = listener.accept().await.unwrap();
        seen.send(()).unwrap();
        std::future::pending::<()>().await;
    });
    let server = AppServer::new(core);
    let directory = tempfile::tempdir().unwrap();
    let thread = server
        .inner
        .core
        .start_thread(ThreadStartParams {
            model: None,
            workspace_root: Some(directory.path().to_string_lossy().into_owned()),
        })
        .await
        .unwrap()
        .thread;
    let (started, events) = server
        .inner
        .core
        .start_turn(TurnStartParams {
            thread_id: thread.id.clone(),
            input: vec![UserInput::Text {
                text: String::from("hold"),
            }],
        })
        .await
        .unwrap();
    let cancellation = CancellationToken::new();
    let completed = CancellationToken::new();
    let lease = Lease {
        server: server.clone(),
        query: None,
        turn: started.turn,
        agent: super::super::desktop_agents::AgentContext {
            agent_id: String::from("default"),
            data_key: qwenpaw_storage::WorkspaceDataKey::LegacyAgent(String::from("default")),
            workspace: directory.path().to_path_buf(),
            config: serde_json::json!({}),
        },
        identity: ApprovalSessionInfo {
            agent: String::from("default"),
            session: thread.id.clone(),
            root_session: thread.id.clone(),
        },
        cancellation: cancellation.clone(),
        completed: completed.clone(),
    };
    let (sender, _unread) = mpsc::channel(1);
    sender
        .try_send(Ok(Event::default().data("occupied")))
        .unwrap();
    let task = tokio::spawn(async move {
        consume(&lease, events, sender).await;
    });
    tokio::time::timeout(Duration::from_secs(5), received)
        .await
        .unwrap()
        .unwrap();
    cancellation.cancel();
    tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .unwrap()
        .unwrap();
    assert!(completed.is_cancelled());
    let result = server.inner.core.read_thread(&thread.id).await.unwrap();
    assert_eq!(
        (result.thread.status, result.turns[0].status),
        (ThreadStatus::Idle, TurnStatus::Interrupted)
    );
    model.abort();
    assert!(model.await.unwrap_err().is_cancelled());
}
