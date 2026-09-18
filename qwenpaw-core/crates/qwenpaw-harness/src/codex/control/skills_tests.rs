use super::*;
use crate::codex::tests::Peer;

const TIMEOUT: Duration = Duration::from_secs(3);
const CWD: &str = "workspace with spaces/技能";

fn skill(name: &str, description: &str, source: &str, enabled: bool) -> Value {
    json!({"name":name,"description":description,"provider_id":"codex",
        "source":source,"enabled":enabled,"read_only":true,"scope":"provider"})
}

fn mixed_response() -> Value {
    json!({"data":[null,17,{}, {"skills":null}, {"skills":[
        null, "ignored", {}, {"name":""},
        {"name":"one","scope":"user","description":"first","enabled":false,
         "path":"private-path","private":"not-public"},
        {"name":"one","scope":"user","description":"duplicate","enabled":true},
        {"name":"one","scope":"repo"}, {"name":"two","enabled":null}
    ]}, {"skills":[
        {"name":"one","scope":"repo","description":"later workspace"},
        {"name":"three","scope":"system","enabled":"false"}
    ]}],"errors":[{"message":"original method ignores entry errors"}]})
}

async fn exchange(response: Value) -> (Result<Vec<HarnessDiscoveredSkill>, Error>, Value) {
    let mut peer = Peer::new();
    let client = peer.client.clone();
    let task = tokio::spawn(async move { client.discover_skills(Path::new(CWD), TIMEOUT).await });
    let mut request = peer.receive().await;
    let id = request.as_object_mut().unwrap().remove("id").unwrap();
    peer.send(json!({"id":id,"result":response})).await;
    (task.await.unwrap(), request)
}

#[tokio::test]
async fn full_skill_mapping_preserves_first_name_source_and_hides_private_fields() {
    let (result, request) = exchange(mixed_response()).await;
    assert_eq!(
        serde_json::to_value(result.unwrap()).unwrap(),
        json!([
            skill("one", "first", "user", false),
            skill("one", "", "repo", true),
            skill("two", "", "", false),
            skill("three", "", "system", true)
        ])
    );
    assert_eq!(
        request,
        json!({"method":"skills/list","params":{"cwds":[CWD],"forceReload":false}})
    );
}

#[tokio::test]
async fn missing_empty_and_falsy_skill_lists_keep_original_defaults() {
    for response in [
        Value::Null,
        json!({}),
        json!({"data":[]}),
        json!({"data":[{"skills":null},{"skills":false},{"skills":0},{"skills":""},{}]}),
    ] {
        assert_eq!(exchange(response).await.0, Ok(vec![]));
    }
    assert_eq!(
        serde_json::to_value(
            exchange(json!({"data":[{"skills":[
                {"name":true,"scope":9,"description":false},
                {"name":false},{"name":0},{"name":"x","scope":null,"description":12}
            ]}]}))
            .await
            .0
            .unwrap()
        )
        .unwrap(),
        json!([skill("True", "", "9", true), skill("x", "12", "", true)])
    );
}

#[tokio::test]
async fn invalid_skill_response_does_not_return_partial_results() {
    for response in [
        json!([]),
        json!({"data":null}),
        json!({"data":{}}),
        json!({"data":[{"skills":[{"name":"good"}]},{"skills":17}]}),
        json!({"data":[{"skills":[{"name":"good"},{"name":1.5}]}]}),
    ] {
        assert_eq!(exchange(response).await.0, Err(Error::InvalidFrame));
    }
}

#[tokio::test]
async fn discovery_protocol_error_remains_an_error() {
    let mut peer = Peer::new();
    let client = peer.client.clone();
    let task = tokio::spawn(async move { client.discover_skills(Path::new(CWD), TIMEOUT).await });
    let request = peer.receive().await;
    peer.send(json!({"id":request["id"],"error":{"code":-32001,"message":"busy","data":null}}))
        .await;
    assert_eq!(
        task.await.unwrap(),
        Err(Error::Protocol {
            code: -32001,
            message: "busy".to_owned(),
            data: Value::Null
        })
    );
}

#[tokio::test]
async fn discovery_timeout_and_cancellation_clear_pending_requests() {
    let mut peer = Peer::new();
    let client = peer.client.clone();
    let task = tokio::spawn(async move {
        client
            .discover_skills(Path::new(CWD), Duration::from_millis(30))
            .await
    });
    peer.receive().await;
    assert_eq!(task.await.unwrap(), Err(Error::Timeout));
    assert!(peer.client.shared.state.lock().unwrap().pending.is_empty());

    let client = peer.client.clone();
    let task = tokio::spawn(async move { client.discover_skills(Path::new(CWD), TIMEOUT).await });
    peer.receive().await;
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    assert!(peer.client.shared.state.lock().unwrap().pending.is_empty());
}

#[cfg(unix)]
#[tokio::test]
async fn non_unicode_workspace_is_rejected_without_lossy_rpc() {
    use std::os::unix::ffi::OsStrExt;
    let peer = Peer::new();
    let cwd = Path::new(std::ffi::OsStr::from_bytes(b"invalid-\xff"));
    assert_eq!(
        peer.client.discover_skills(cwd, TIMEOUT).await,
        Err(Error::Io(std::io::ErrorKind::InvalidInput))
    );
    assert!(peer.client.shared.state.lock().unwrap().pending.is_empty());
}

#[tokio::test]
#[ignore = "requires qwenpaw Python environment; compare original skill discovery"]
async fn skill_discovery_matches_original_python_method() {
    let responses = [
        Value::Null,
        json!({}),
        mixed_response(),
        json!({"data":[{"skills":[{"name":true,"scope":9,"description":12,"enabled":null}]}]}),
    ];
    let script =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/codex_control_reference.py");
    for response in responses {
        let (result, request) = exchange(response.clone()).await;
        let fixture = json!({"operation":"skills","cwd":CWD,"responses":[response]});
        let output = tokio::time::timeout(
            Duration::from_secs(20),
            tokio::process::Command::new("python")
                .arg(&script)
                .arg(fixture.to_string())
                .kill_on_drop(true)
                .output(),
        )
        .await
        .unwrap()
        .unwrap();
        assert!(
            output.status.success(),
            "reference error: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let expected: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(
            json!({"result":result.unwrap(),"requests":[request]}),
            expected
        );
    }
}
