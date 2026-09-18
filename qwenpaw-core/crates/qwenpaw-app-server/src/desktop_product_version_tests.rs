use super::*;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn product_http_identity_is_separate_from_core_initialize_identity() {
    let fixture = Fixture::new().await;
    let response = reqwest::Client::builder()
        .no_proxy()
        .build()
        .unwrap()
        .get(format!("{}/api/version", fixture.base))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let actual: Value = response.json().await.unwrap();
    let (tx, mut rx) = tokio::sync::mpsc::channel(8);
    fixture
        .server
        .process_line(
            &mut crate::ConnectionSession::default(),
            &json!({"id":1,"method":"initialize","params":{
                "clientInfo":{"name":"version-test","version":"0.1.0"}
            }})
            .to_string(),
            &tx,
        )
        .await;
    let initialize: Value = serde_json::from_str(&rx.recv().await.unwrap()).unwrap();
    fixture.shutdown().await;
    assert_eq!(
        initialize,
        json!({"id":1,"result":{
            "protocolVersion":3,"serverInfo":{"name":"qwenpaw-core","version":"0.2.0"}
        }})
    );
    assert_eq!(
        actual,
        json!({"version":"2.2.0b5","backend":"rust-core","protocolVersion":3})
    );
}

#[tokio::test]
#[ignore = "requires original product source tree; reads version text without executing Python"]
async fn product_identity_matches_original_release_source() {
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../src/qwenpaw/__version__.py");
    let text =
        std::fs::read_to_string(source).expect("original product version source is required");
    let declared: Vec<_> = text
        .lines()
        .filter_map(|line| {
            line.strip_prefix("__version__ = ")
                .map(|value| serde_json::from_str::<String>(value).unwrap())
        })
        .collect();
    assert_eq!(declared.len(), 1);
    assert_eq!(crate::version().await.0["version"], declared[0]);
}

#[tokio::test]
#[ignore = "requires built original console, Node and headless Chrome"]
async fn product_identity_preserves_original_plugin_compatibility_interaction() {
    let fixture = Fixture::with_browser(true).await;
    let plugins: Vec<_> = [
        ("current", "Current Plugin", "2.x"),
        ("old", "Old Plugin", "1.x"),
    ]
    .into_iter()
    .map(|(id, name, label)| {
        json!({
            "id":format!("@fixture/{id}"),"display_name":name,"version":"1.2.3",
            "developer":"Fixture","owner":"fixture","logo_url":null,"downloads":12,
            "view_count":31,"details_url":null,
            "locales":{"en":{"description":"Compatibility fixture","category":"agent-tool"}},
            "qwenpaw_compat_labels":[label],"is_featured":false,"is_trending":false
        })
    })
    .collect();
    fixture.respond(
        "/qwenpaw/openapi/v1/plugins",
        json!({
            "success":true,"message":"ok","data":{"total":2,"plugins":plugins}
        }),
    );
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = tokio::time::timeout(
        Duration::from_secs(120),
        tokio::process::Command::new("node")
            .arg(root.join("scripts/console_product_version_smoke.mjs"))
            .arg(&fixture.base)
            .kill_on_drop(true)
            .output(),
    )
    .await
    .unwrap()
    .unwrap();
    fixture.shutdown().await;
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    for key in [
        "ok",
        "classification",
        "warning",
        "cancel",
        "reload",
        "noInstall",
    ] {
        assert_eq!(report[key], true);
    }
}
