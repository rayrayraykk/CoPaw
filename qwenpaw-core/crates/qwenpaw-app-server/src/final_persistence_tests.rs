//! Each host reports failed final writes after draining its services.

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::time::Duration;

use pretty_assertions::assert_eq;
use qwenpaw_core::{Core, ModelConfig};
use qwenpaw_protocol::{CoreEvent, ThreadStartParams, TurnStartParams, TurnStatus, UserInput};

use super::AppServer;

async fn failed_server() -> (tempfile::TempDir, AppServer) {
    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("core.sqlite");
    let core = Core::persistent(
        ModelConfig {
            api_key: Some(String::from("fixture-key")),
            base_url: String::from("http://127.0.0.1:1/v1"),
            default_model: String::from("fixture"),
        },
        &database,
    )
    .unwrap();
    let thread = core
        .start_thread(ThreadStartParams {
            model: None,
            workspace_root: Some(directory.path().to_string_lossy().into_owned()),
        })
        .await
        .unwrap()
        .thread;
    let database = rusqlite::Connection::open(database).unwrap();
    database
        .execute_batch(
            "CREATE TRIGGER reject_final BEFORE INSERT ON threads
         WHEN json_extract(NEW.snapshot, '$.turns[#-1].status') != 'inProgress'
         BEGIN SELECT RAISE(FAIL, 'private-sqlite-detail'); END;",
        )
        .unwrap();
    let (_, mut events) = core
        .start_turn(TurnStartParams {
            thread_id: thread.id,
            input: vec![UserInput::Text {
                text: String::from("fixture"),
            }],
        })
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let CoreEvent::TurnCompleted(event) = events.recv().await.expect("terminal event") {
                assert_eq!(event.turn.status, TurnStatus::Failed);
                assert!(
                    !event
                        .turn
                        .error
                        .unwrap()
                        .message
                        .contains("private-sqlite-detail")
                );
                break;
            }
        }
    })
    .await
    .unwrap();
    assert!(core.check_final_persistence().is_err());
    (directory, AppServer::new(core))
}

fn private_file(path: &Path, content: impl AsRef<[u8]>) {
    std::fs::write(path, content).unwrap();
    #[cfg(unix)]
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
}

#[tokio::test]
async fn final_persistence_failure_reaches_stdio_http_and_wss_host_results() {
    for transport in ["stdio", "http", "wss"] {
        let (directory, mut server) = failed_server().await;
        let result = match transport {
            "stdio" => {
                super::stdio::run(server.clone(), tokio::io::empty(), tokio::io::sink()).await
            }
            "http" => {
                let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
                server.inner.shutdown.cancel();
                server.clone().run_http(listener).await
            }
            "wss" => {
                let certificate = directory.path().join("certificate.pem");
                let key = directory.path().join("key.pem");
                let token = directory.path().join("token");
                let rcgen::CertifiedKey { cert, signing_key } =
                    rcgen::generate_simple_self_signed(vec![String::from("localhost")]).unwrap();
                std::fs::write(&certificate, cert.pem()).unwrap();
                private_file(&key, signing_key.serialize_pem());
                private_file(&token, "fixture-only-token-0123456789abcdef");
                server = server.with_remote_auth_token_file(&token).unwrap();
                server.inner.shutdown.cancel();
                let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
                tokio::time::timeout(
                    Duration::from_secs(5),
                    server.clone().run_wss(listener, &certificate, &key),
                )
                .await
                .unwrap()
            }
            _ => unreachable!(),
        };
        assert_eq!(
            result.unwrap_err().to_string(),
            "thread storage failed: one or more final turn writes failed in this Core instance",
            "{transport}"
        );
        assert!(server.inner.shutdown.is_cancelled());
        assert_eq!(
            super::desktop_checkpoints::runtime::task_counts(&server),
            (0, 0)
        );
    }
}
