use super::*;
use axum::Router;
use axum::routing::get;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn official_catalog_transport_handles_gzip_header_without_following_redirects() {
    use std::io::Write as _;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    let mut compressed = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    compressed.write_all(br#"{"files":null}"#).unwrap();
    let compressed = compressed.finish().unwrap();
    let visited = Arc::new(AtomicBool::new(false));
    let destination = visited.clone();
    let router = Router::new()
        .route(
            "/gzip",
            get(move || {
                let bytes = compressed.clone();
                async move { ([("content-encoding", "gzip")], bytes) }
            }),
        )
        .route(
            "/redirect",
            get(|| async { axum::response::Redirect::temporary("/destination") }),
        )
        .route(
            "/destination",
            get(move || {
                let destination = destination.clone();
                async move {
                    destination.store(true, Ordering::SeqCst);
                    Json(json!({}))
                }
            }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    assert_eq!(
        fetch::<Value>(&base, "/gzip").await,
        Ok(json!({"files":null}))
    );
    assert_eq!(fetch::<Value>(&base, "/redirect").await, Err(()));
    assert!(!visited.load(Ordering::SeqCst));
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
}

#[test]
fn official_catalog_source_urls_cannot_replace_the_configured_origin() {
    let base = "https://download.qwenpaw.agentscope.io";
    assert!(source_url(base, "https://external.invalid/index.json").is_none());
    for path in [
        "//external.invalid/index.json",
        "/\\external.invalid/index.json",
        "/plugins/index.json",
    ] {
        assert_eq!(
            source_url(base, path).unwrap().origin(),
            url::Url::parse(base).unwrap().origin()
        );
    }
}
