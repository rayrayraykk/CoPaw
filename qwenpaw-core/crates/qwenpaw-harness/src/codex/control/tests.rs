use super::*;
use crate::codex::CodexProcess;
use crate::codex::tests::{Peer, child_command};

const TIMEOUT: Duration = Duration::from_secs(3);

async fn call(client: CodexClient, operation: &str) -> Result<Value, Error> {
    match operation {
        "status" => Ok(serde_json::to_value(client.account_status(TIMEOUT).await?).unwrap()),
        "models" => Ok(serde_json::to_value(client.models(TIMEOUT).await?).unwrap()),
        "browser" => client.start_login(false, TIMEOUT).await,
        "device" => client.start_login(true, TIMEOUT).await,
        "logout" => {
            client.logout(TIMEOUT).await?;
            Ok(Value::Null)
        }
        _ => panic!("Unknown test operation"),
    }
}

async fn exchange(operation: &str, responses: &[Value]) -> (Result<Value, Error>, Vec<Value>) {
    let mut peer = Peer::new();
    let client = peer.client.clone();
    let operation = operation.to_owned();
    let pending = tokio::spawn(async move { call(client, &operation).await });
    let mut requests = Vec::new();
    for response in responses {
        let mut request = peer.receive().await;
        let id = request.as_object_mut().unwrap().remove("id").unwrap();
        requests.push(request);
        peer.send(json!({"id":id,"result":response})).await;
    }
    (pending.await.unwrap(), requests)
}

fn model(id: &str) -> Value {
    json!({"id":id,"name":id,"description":"","is_default":false,
        "reasoning_efforts":[],"default_reasoning_effort":null})
}

#[tokio::test]
async fn account_fields_are_whitelisted_with_original_truth_semantics() {
    for (account, expected) in [
        (Value::Null, json!({"authenticated":false,"account":null})),
        (json!({}), json!({"authenticated":false,"account":null})),
        (
            json!({"type":"apiKey","secret":"hidden"}),
            json!({"authenticated":true,"account":{"type":"apiKey"}}),
        ),
        (
            json!({"type":"chatgpt","email":null,"planType":"plus","futureCredential":"hidden"}),
            json!({"authenticated":true,"account":{"type":"chatgpt","email":null,"planType":"plus"}}),
        ),
        (
            json!({"futureCredential":"hidden"}),
            json!({"authenticated":true,"account":{}}),
        ),
    ] {
        assert_eq!(
            exchange(
                "status",
                &[json!({"account":account,"requiresOpenaiAuth":false})]
            )
            .await,
            (
                Ok(expected),
                vec![json!({"method":"account/read","params":{"refreshToken":false}})]
            )
        );
    }
}

#[tokio::test]
async fn all_pages_preserve_order_duplicates_defaults_and_efforts() {
    let pages = [
        json!({"data":[{"id":"first"},{}],"nextCursor":"cursor with spaces"}),
        json!({"data":[{"id":"ignored-id","model":"second","displayName":"Second Model",
            "description":"desc","isDefault":true,"hidden":true,
            "supportedReasoningEfforts":[{"reasoningEffort":"low"},{},{"reasoningEffort":"high"}],
            "defaultReasoningEffort":"high"},{"id":"first"}],"nextCursor":null}),
    ];
    assert_eq!(
        exchange("models", &pages).await,
        (
            Ok(json!([
                model("first"),{"id":"second","name":"Second Model","description":"desc","is_default":true,
                    "reasoning_efforts":["low","high"],"default_reasoning_effort":"high"},model("first")
            ])),
            vec![
                json!({"method":"model/list","params":{"cursor":null,"includeHidden":false}}),
                json!({"method":"model/list","params":{"cursor":"cursor with spaces","includeHidden":false}}),
            ]
        )
    );
}

#[tokio::test]
async fn browser_device_and_logout_preserve_original_requests() {
    for (operation, method, params, response) in [
        (
            "browser",
            "account/login/start",
            json!({"type":"chatgpt","useHostedLoginSuccessPage":true,"appBrand":"codex"}),
            json!({"type":"chatgpt","loginId":"one","authUrl":"https://example.invalid/login"}),
        ),
        (
            "device",
            "account/login/start",
            json!({"type":"chatgptDeviceCode"}),
            json!({"type":"chatgptDeviceCode","loginId":"two","verificationUrl":"https://example.invalid/device","userCode":"TEST"}),
        ),
        ("logout", "account/logout", json!({}), Value::Null),
    ] {
        assert_eq!(
            exchange(operation, std::slice::from_ref(&response)).await,
            (Ok(response), vec![json!({"method":method,"params":params})])
        );
    }
    assert_eq!(exchange("browser", &[Value::Null]).await.0, Ok(json!({})));
}

#[tokio::test]
async fn failed_page_returns_error_instead_of_a_partial_catalog() {
    let mut peer = Peer::new();
    let client = peer.client.clone();
    let task = tokio::spawn(async move { client.models(TIMEOUT).await });
    let request = peer.receive().await;
    peer.send(json!({"id":request["id"],"result":{"data":[{"id":"first"}],"nextCursor":"next"}}))
        .await;
    let request = peer.receive().await;
    peer.send(
        json!({"id":request["id"],"error":{"code":-32001,"message":"busy","data":{"retry":true}}}),
    )
    .await;
    assert_eq!(
        task.await.unwrap(),
        Err(Error::Protocol {
            code: -32001,
            message: "busy".to_owned(),
            data: json!({"retry":true})
        })
    );
}

#[tokio::test]
async fn repeat_cursor_invalid_data_and_wrong_account_shapes_fail_explicitly() {
    assert_eq!(
        exchange(
            "models",
            &[
                json!({"data":[],"nextCursor":"same"}),
                json!({"data":[],"nextCursor":"same"})
            ]
        )
        .await
        .0,
        Err(Error::InvalidFrame)
    );
    for invalid in [
        json!({"data":null}),
        json!({"data":[true]}),
        json!({"data":[{"id":"a","supportedReasoningEfforts":null}]}),
        json!({"nextCursor":true}),
    ] {
        assert_eq!(
            exchange("models", &[invalid]).await.0,
            Err(Error::InvalidFrame)
        );
    }
    assert_eq!(
        exchange("status", &[json!({"account":["not-an-account"]})])
            .await
            .0,
        Err(Error::InvalidFrame)
    );
}

#[tokio::test]
async fn whole_pagination_timeout_clears_the_in_flight_waiter() {
    let mut peer = Peer::new();
    let client = peer.client.clone();
    let task = tokio::spawn(async move { client.models(Duration::from_millis(30)).await });
    let request = peer.receive().await;
    peer.send(json!({"id":request["id"],"result":{"data":[{"id":"first"}],"nextCursor":"next"}}))
        .await;
    peer.receive().await;
    assert_eq!(task.await.unwrap(), Err(Error::Timeout));
    assert!(peer.client.shared.state.lock().unwrap().pending.is_empty());
}

#[tokio::test]
async fn control_methods_work_through_an_owned_rust_child() {
    let directory = tempfile::tempdir().unwrap();
    let process = CodexProcess::spawn(child_command(directory.path(), "normal"), TIMEOUT)
        .await
        .unwrap();
    let client = process.client();
    assert_eq!(
        serde_json::to_value(client.account_status(TIMEOUT).await.unwrap()).unwrap(),
        json!({"authenticated":true,"account":{"type":"chatgpt","email":"fixture@example.invalid","planType":"plus"}})
    );
    assert_eq!(
        serde_json::to_value(client.models(TIMEOUT).await.unwrap()).unwrap(),
        json!([model("first"), model("second")])
    );
    assert_eq!(
        client.start_login(false, TIMEOUT).await.unwrap(),
        json!({"type":"chatgpt","loginId":"fixture-login"})
    );
    assert_eq!(
        client.start_login(true, TIMEOUT).await.unwrap(),
        json!({"type":"chatgptDeviceCode","loginId":"fixture-login"})
    );
    client.logout(TIMEOUT).await.unwrap();
    process.shutdown().await.unwrap();
    let records: Value =
        serde_json::from_slice(&std::fs::read(directory.path().join("finished.json")).unwrap())
            .unwrap();
    let methods: Vec<_> = records
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["method"].as_str().unwrap())
        .collect();
    assert_eq!(
        methods,
        [
            "initialize",
            "initialized",
            "account/read",
            "model/list",
            "model/list",
            "account/login/start",
            "account/login/start",
            "account/logout"
        ]
    );
}

#[tokio::test]
#[ignore = "requires qwenpaw Python environment; compare original Codex control methods"]
async fn connected_control_matches_original_python_methods() {
    let cases = [
        ("status", vec![Value::Null]),
        (
            "status",
            vec![json!({"account":null,"requiresOpenaiAuth":false})],
        ),
        ("status", vec![json!({"account":{}})]),
        (
            "status",
            vec![json!({"account":{"type":"apiKey","secret":"not-public"}})],
        ),
        (
            "status",
            vec![
                json!({"account":{"type":"chatgpt","email":null,"planType":"pro","private":"not-public"}}),
            ],
        ),
        ("status", vec![json!({"account":{"unknown":"not-public"}})]),
        ("models", vec![Value::Null]),
        (
            "models",
            vec![
                json!({"data":[{"id":"one"}],"nextCursor":"next"}),
                json!({"data":[{"id":"two","model":"actual","displayName":"Actual","isDefault":true,"description":"desc","supportedReasoningEfforts":[{"reasoningEffort":"low"},{},{"reasoningEffort":"high"}],"defaultReasoningEffort":"high"},{"id":"one"},{}]}),
            ],
        ),
        (
            "models",
            vec![
                json!({"data":[{"model":true,"description":9,"supportedReasoningEfforts":[{"reasoningEffort":true}],"defaultReasoningEffort":2,"isDefault":"false"},{"model":false,"id":0}]}),
            ],
        ),
        (
            "browser",
            vec![
                json!({"type":"chatgpt","loginId":"browser","authUrl":"https://example.invalid/login"}),
            ],
        ),
        ("browser", vec![Value::Null]),
        (
            "device",
            vec![
                json!({"type":"chatgptDeviceCode","loginId":"device","userCode":"TEST","verificationUrl":"https://example.invalid/device"}),
            ],
        ),
        ("logout", vec![json!({"ignored":true})]),
    ];
    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../scripts/codex_control_reference.py");
    for (operation, responses) in cases {
        let (result, requests) = exchange(operation, &responses).await;
        let fixture = json!({"operation":operation,"responses":responses});
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
            json!({"result":result.unwrap(),"requests":requests}),
            expected,
            "operation: {operation}"
        );
    }
}
