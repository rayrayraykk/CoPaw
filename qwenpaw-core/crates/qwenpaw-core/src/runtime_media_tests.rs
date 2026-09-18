use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use pretty_assertions::assert_eq;
use serde_json::json;

use super::*;

const PNG: &str =
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAIAAACQd1PeAAAADElEQVR4nGP4z8AAAAMBAQDJ/pLvAAAAAElFTkSuQmCC";

async fn drain(mut events: TurnEventStream) {
    tokio::time::timeout(Duration::from_secs(10), async move {
        while let Some(event) = events.recv().await {
            if let CoreEvent::TurnCompleted(event) = event {
                assert_eq!(event.turn.status, TurnStatus::Completed, "{event:#?}");
                return;
            }
        }
        panic!("missing completion");
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn image_only_turn_survives_tools_checkpoint_reopen_and_file_changes() {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let base_url = start_tool_model_server(requests.clone(), "read_file").await;
    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("db.sqlite3");
    let image = directory.path().join("red.png");
    std::fs::write(&image, STANDARD.decode(PNG).unwrap()).unwrap();
    std::fs::write(directory.path().join("notes.txt"), "workspace secret").unwrap();
    let config = ModelConfig {
        api_key: None,
        base_url,
        default_model: String::from("test"),
    };
    let core = Core::persistent(config.clone(), &database).unwrap();
    let id = core
        .start_thread(ThreadStartParams {
            model: None,
            workspace_root: Some(directory.path().to_string_lossy().into_owned()),
        })
        .await
        .unwrap()
        .thread
        .id;
    let (response, events) = core
        .start_turn(TurnStartParams {
            thread_id: id.clone(),
            input: vec![UserInput::Image {
                path: String::from("red.png"),
            }],
        })
        .await
        .unwrap();
    std::fs::write(&image, b"changed after input was accepted").unwrap();
    drain(events).await;
    assert_eq!(
        response.turn.items[0],
        Item::UserMessage {
            id: response.turn.items[0].id().to_owned(),
            text: String::new(),
            input: Some(vec![UserInput::Image {
                path: String::from("red.png")
            }])
        }
    );
    let checkpoint = core.export_thread_checkpoint(&id).await.unwrap();
    let content = core.read_user_inputs(&id).await.unwrap();
    assert_eq!(content.len(), 1);
    assert!(
        !serde_json::to_string(&core.read_thread(&id).await.unwrap())
            .unwrap()
            .contains(PNG)
    );
    core.restore_thread_checkpoint(&id, checkpoint.clone())
        .await
        .unwrap();
    assert_eq!(core.read_user_inputs(&id).await.unwrap(), content);
    drop(core);
    let reopened = Core::persistent(config, &database).unwrap();
    assert_eq!(
        reopened.export_thread_checkpoint(&id).await.unwrap(),
        checkpoint
    );
    let (_, events) = reopened
        .start_turn(TurnStartParams {
            thread_id: id.clone(),
            input: vec![UserInput::Text {
                text: String::from("Recall that image"),
            }],
        })
        .await
        .unwrap();
    drain(events).await;
    assert_eq!(reopened.read_user_inputs(&id).await.unwrap(), content);
    let requests = requests.lock().await;
    assert_eq!(requests.len(), 3);
    for request in requests.iter() {
        assert_eq!(
            request["messages"][1],
            json!({"role":"user", "content":[{"type":"image_url","image_url":{"url":format!("data:image/png;base64,{PNG}")}}]})
        );
        assert!(!request.to_string().contains("user_input"));
    }
}

#[tokio::test]
async fn invalid_image_does_not_commit_a_partial_turn_or_change_model() {
    let directory = tempfile::tempdir().unwrap();
    let core = Core::new(ModelConfig {
        api_key: None,
        base_url: String::from("http://127.0.0.1:9"),
        default_model: String::from("before"),
    });
    let id = core
        .start_thread(ThreadStartParams {
            model: None,
            workspace_root: Some(directory.path().to_string_lossy().into_owned()),
        })
        .await
        .unwrap()
        .thread
        .id;
    let before = core.export_thread_checkpoint(&id).await.unwrap();
    let error = core
        .start_turn_with_model(
            TurnStartParams {
                thread_id: id.clone(),
                input: vec![
                    UserInput::Text {
                        text: String::from("must not persist"),
                    },
                    UserInput::Image {
                        path: String::from("missing.png"),
                    },
                ],
            },
            Some((
                ModelConfig {
                    api_key: None,
                    base_url: String::from("http://127.0.0.1:9"),
                    default_model: String::from("after"),
                },
                ModelRequestOptions::default(),
            )),
        )
        .await
        .err()
        .unwrap();
    assert!(matches!(error, CoreError::Media(_)));
    assert_eq!(core.export_thread_checkpoint(&id).await.unwrap(), before);
}
