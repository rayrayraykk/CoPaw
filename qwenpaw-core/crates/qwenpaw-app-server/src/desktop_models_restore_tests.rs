use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;

use super::*;

struct Credentials {
    key: Option<String>,
    fail: bool,
    reads: AtomicUsize,
}

impl DesktopCredentialStore for Credentials {
    fn load_api_key(&self) -> anyhow::Result<Option<String>> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        anyhow::ensure!(!self.fail, "private keyring diagnostics");
        Ok(self.key.clone())
    }

    fn save_api_key(&self, _: Option<&str>) -> anyhow::Result<()> {
        panic!("Candidate hydration must not change the credential store")
    }
}

fn core() -> Core {
    Core::new(qwenpaw_core::ModelConfig {
        api_key: Some(String::from("local-key")),
        base_url: String::from("http://127.0.0.1:1/v1"),
        default_model: String::from("local-model"),
    })
}

#[test]
fn model_hydration_matches_startup_selection_and_returns_staged_bytes_without_mutating_source() {
    let live = core();
    let before = live.backup_snapshot(1024 * 1024).unwrap();
    let registry = default_registry("restored-model", "http://127.0.0.1:2/v1", true);
    let bytes = serde_json::to_vec(&registry).unwrap();
    let original = bytes.clone();
    for key in [Some(String::from("restored-key")), None] {
        let candidate = live.prepare_restore(&before).unwrap();
        let credentials = Credentials {
            key: key.clone(),
            fail: false,
            reads: AtomicUsize::new(0),
        };
        let normalized = hydrate_restore(&candidate, &credentials, &bytes).unwrap();
        let mut expected = live.read_config();
        expected.config.base_url = String::from("http://127.0.0.1:2/v1");
        expected.config.default_model = String::from("restored-model");
        expected.config.api_key_configured = key.is_some();
        assert_eq!(candidate.read_config(), expected);
        assert_eq!(credentials.reads.load(Ordering::SeqCst), 1);
        let parsed: ProviderRegistry = serde_json::from_slice(&normalized).unwrap();
        validate_registry(&parsed).unwrap();
        assert_eq!(
            parsed.providers[DEFAULT_PROVIDER_ID].api_key_configured,
            key.is_some()
        );
        assert_eq!(bytes, original);
        assert_eq!(live.backup_snapshot(1024 * 1024).unwrap(), before);
    }
}

#[test]
fn failed_or_invalid_credential_does_not_change_even_the_candidate_configuration() {
    let candidate = core();
    let before = candidate.backup_snapshot(1024 * 1024).unwrap();
    let config = candidate.read_config();
    let bytes = serde_json::to_vec(&default_registry(
        "restored-model",
        "http://127.0.0.1:2/v1",
        true,
    ))
    .unwrap();
    for (key, fail, error) in [
        (None, true, "Restored model credential could not be loaded"),
        (
            Some(String::from("secret\ninjected")),
            false,
            "Restored model credential is invalid",
        ),
    ] {
        let credentials = Credentials {
            key,
            fail,
            reads: AtomicUsize::new(0),
        };
        assert_eq!(
            hydrate_restore(&candidate, &credentials, &bytes),
            Err(error)
        );
        assert_eq!(candidate.read_config(), config);
        assert_eq!(candidate.backup_snapshot(1024 * 1024).unwrap(), before);
    }
}

#[test]
fn invalid_or_oversized_model_registry_is_rejected_before_credentials_are_read() {
    let candidate = core();
    let credentials = Credentials {
        key: None,
        fail: false,
        reads: AtomicUsize::new(0),
    };
    let before = candidate.read_config();
    for bytes in [
        b"{}".to_vec(),
        vec![b' '; usize::try_from(REGISTRY_MAX_BYTES + 1).unwrap()],
    ] {
        assert!(hydrate_restore(&candidate, &credentials, &bytes).is_err());
    }
    assert_eq!(credentials.reads.load(Ordering::SeqCst), 0);
    assert_eq!(candidate.read_config(), before);
}

#[tokio::test]
async fn applied_candidate_sends_the_restored_model_and_credential_to_the_actual_http_endpoint() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}/v1", listener.local_addr().unwrap());
    let (sent, mut received) = tokio::sync::mpsc::channel(1);
    let router = axum::Router::new().route("/v1/chat/completions", axum::routing::post(
        move |headers: axum::http::HeaderMap, Json(body): Json<Value>| {
            let sent = sent.clone();
            async move {
                sent.send((headers.get("authorization").unwrap().to_str().unwrap().to_owned(), body)).await.unwrap();
                ([ (axum::http::header::CONTENT_TYPE, "text/event-stream") ],
                    "data: {\"choices\":[{\"delta\":{\"content\":\"restored response\"},\"finish_reason\":null}]}\n\ndata: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n")
            }
        }
    ));
    let stop = tokio_util::sync::CancellationToken::new();
    let shutdown = stop.clone();
    let server = tokio::spawn(async move {
        axum::serve(listener, router)
            .with_graceful_shutdown(shutdown.cancelled_owned())
            .await
            .unwrap();
    });
    let live = core();
    let snapshot = live.backup_snapshot(1024 * 1024).unwrap();
    let mut lease = live
        .begin_restore(std::time::Duration::from_secs(2))
        .await
        .unwrap();
    let candidate = lease.prepare_restore(&snapshot).unwrap();
    let bytes = serde_json::to_vec(&default_registry("restored-model", &base, true)).unwrap();
    let credentials = Credentials {
        key: Some(String::from("restored-key")),
        fail: false,
        reads: AtomicUsize::new(0),
    };
    hydrate_restore(&candidate, &credentials, &bytes).unwrap();
    lease.apply(&candidate, 1024 * 1024).await.unwrap();
    drop(lease);
    let directory = tempfile::tempdir().unwrap();
    let thread = live
        .start_thread(qwenpaw_protocol::ThreadStartParams {
            model: None,
            workspace_root: Some(directory.path().to_string_lossy().into_owned()),
        })
        .await
        .unwrap()
        .thread;
    let (_, mut events) = live
        .start_turn(qwenpaw_protocol::TurnStartParams {
            thread_id: thread.id,
            input: vec![qwenpaw_protocol::UserInput::Text {
                text: String::from("test restored runtime"),
            }],
        })
        .await
        .unwrap();
    let mut result = None;
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while let Some(event) = events.recv().await {
            if let qwenpaw_protocol::CoreEvent::TurnCompleted(completed) = event {
                result = Some(completed.turn.status);
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(result, Some(qwenpaw_protocol::TurnStatus::Completed));
    let (authorization, request) = received.try_recv().unwrap();
    assert_eq!(authorization, "Bearer restored-key");
    assert_eq!(request["model"], "restored-model");
    assert_eq!(request["stream"], true);
    assert_eq!(
        request["messages"].as_array().unwrap().last().unwrap(),
        &json!({"role": "user", "content": "test restored runtime"})
    );
    stop.cancel();
    tokio::time::timeout(std::time::Duration::from_secs(2), server)
        .await
        .unwrap()
        .unwrap();
}
