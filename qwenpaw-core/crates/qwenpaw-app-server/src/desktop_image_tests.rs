use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use pretty_assertions::assert_eq;

use super::*;

const PNG: &str =
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAIAAACQd1PeAAAADElEQVR4nGP4z8AAAAMBAQDJ/pLvAAAAAElFTkSuQmCC";

async fn upload(client: &reqwest::Client, base: &str, bytes: Vec<u8>) -> String {
    let form = reqwest::multipart::Form::new().part(
        "file",
        reqwest::multipart::Part::bytes(bytes)
            .file_name("红色.png")
            .mime_str("image/png")
            .unwrap(),
    );
    let response = client
        .post(format!("{base}/api/console/upload"))
        .multipart(form)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    response.json::<Value>().await.unwrap()["url"]
        .as_str()
        .unwrap()
        .to_owned()
}

async fn send_image(
    client: &reqwest::Client,
    base: &str,
    fixture: &Fixture,
    stored: &str,
    session: &str,
) -> reqwest::Response {
    client.post(format!("{base}/api/console/chat")).json(&json!({
        "session_id": session,
        "request_context": {"session_project_dirs": [{"path": fixture.directory.path().join("workspace")}]},
        "input": [{"role":"user", "content":[{"type":"text","text":"Before"},{"type":"image","image_url":stored},{"type":"text","text":"After"}]}]
    })).send().await.unwrap()
}

#[tokio::test]
async fn console_uploads_images_to_native_model_and_restores_ordered_history() {
    let fixture = Fixture::new(false).await;
    fixture.configure().await;
    api(&fixture.server, "PUT", "/api/models/active", json!({"provider_id":"gemini", "model":"fixture-gemini", "scope":"agent", "agent_id":"default"})).await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let http = tokio::spawn(fixture.server.clone().run_http(listener));
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .unwrap();
    let bytes = STANDARD.decode(PNG).unwrap();
    let stored = upload(&client, &base, bytes.clone()).await;
    let response = send_image(&client, &base, &fixture, &stored, "1700000000000-image").await;
    let status = response.status();
    let stream = response.text().await.unwrap();
    assert_eq!(status, StatusCode::OK, "{stream}");
    assert!(stream.contains("completed"), "{stream}");
    assert!(stream.contains("原生 Gemini 回复"), "{stream}");
    let chats = client
        .get(format!("{base}/api/chats"))
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
    let id = chats[0]["id"].as_str().unwrap();
    let url = format!("{base}/api/chats/{id}");
    let before = client
        .get(&url)
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
    assert_eq!(
        before["messages"][0]["content"],
        json!([
            {"type":"text","text":"Before"},
            {"type":"image","image_url":format!("data:image/png;base64,{PNG}")},
            {"type":"text","text":"After"}
        ])
    );
    std::fs::write(
        fixture
            .directory
            .path()
            .join("workspace/.qwenpaw/attachments")
            .join(&stored),
        b"mutated",
    )
    .unwrap();
    assert_eq!(
        client
            .get(&url)
            .send()
            .await
            .unwrap()
            .json::<Value>()
            .await
            .unwrap(),
        before
    );
    let requests = fixture.remote.lock().unwrap().requests.clone();
    assert_eq!(requests.len(), 2);
    for (_, request) in requests {
        assert_eq!(
            request["contents"][0],
            json!({"role":"user","parts":[{"text":"Before"},{"inlineData":{"mimeType":"image/png","data":PNG}},{"text":"After"}]})
        );
    }
    assert_rejections_and_large_images(&client, &base, &fixture, id, &url, bytes).await;
    fixture.server.inner.shutdown.cancel();
    tokio::time::timeout(Duration::from_secs(5), http)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
}

async fn assert_rejections_and_large_images(
    client: &reqwest::Client,
    base: &str,
    fixture: &Fixture,
    id: &str,
    url: &str,
    bytes: Vec<u8>,
) {
    let checkpoint = fixture
        .server
        .inner
        .core
        .export_thread_checkpoint(id)
        .await
        .unwrap();
    let invalid = upload(client, base, b"not really an image".to_vec()).await;
    let response = send_image(client, base, fixture, &invalid, "1700000000000-image").await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        fixture
            .server
            .inner
            .core
            .export_thread_checkpoint(id)
            .await
            .unwrap(),
        checkpoint
    );
    let forbidden = client
        .get(url)
        .header("x-agent-id", "foreign")
        .send()
        .await
        .unwrap();
    assert_eq!(forbidden.status(), StatusCode::NOT_FOUND);
    let mut large = bytes;
    large.resize(2 * 1_048_576 + 1, 0);
    let stored = upload(client, base, large).await;
    let response = send_image(client, base, fixture, &stored, "1700000000001-large").await;
    assert_eq!(response.status(), StatusCode::OK);
    assert!(response.text().await.unwrap().contains("completed"));
    let request = fixture
        .remote
        .lock()
        .unwrap()
        .requests
        .last()
        .unwrap()
        .1
        .clone();
    assert_eq!(
        request["contents"][0]["parts"][1],
        json!({"text":"[media omitted from model context: local file is 2097153 bytes, exceeds inline limit of 2097152 bytes]"})
    );
    let chats = client
        .get(format!("{base}/api/chats"))
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
    let large_id = chats
        .as_array()
        .unwrap()
        .iter()
        .find(|chat| chat["session_id"] == "1700000000001-large")
        .unwrap()["id"]
        .as_str()
        .unwrap();
    let history = client
        .get(format!("{base}/api/chats/{large_id}"))
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
    assert_eq!(
        history["messages"][0]["content"][1],
        json!({"type":"image","image_url":stored})
    );
    assert_eq!(
        client
            .get(format!("{base}/api/files/preview/{stored}"))
            .send()
            .await
            .unwrap()
            .bytes()
            .await
            .unwrap()
            .len(),
        2 * 1_048_576 + 1
    );
}
