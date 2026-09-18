use super::*;
use pretty_assertions::assert_eq;

#[tokio::test]
#[ignore = "requires console/dist, Node 24+ and Chrome; run explicitly for browser acceptance"]
async fn original_mail_drawer_mutations_preserve_agent_scope_and_survive_reload_and_restart() {
    let mut fixture = Fixture::new().await;
    let original = seed(&fixture).await;
    fixture.server.inner.core.write_ui_language("en").unwrap();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    fixture.server.inner.shutdown.cancel();
    shutdown(&fixture.server).await;
    fixture.server = AppServer::new_desktop_with_stores_and_workspace(
        fixture.server.inner.core.clone(),
        &root.join("../console/dist"),
        String::from("mail-browser-shutdown"),
        Arc::new(Credentials),
        &fixture.directory.path().join("data"),
        &fixture.directory.path().join("workspace"),
    )
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(fixture.server.clone().run_http(listener));
    let result = tokio::time::timeout(
        Duration::from_secs(90),
        tokio::process::Command::new("node")
            .arg(root.join("scripts/console_browser_smoke.mjs"))
            .args([&origin, "/inbox", "--mail-access-crud"])
            .stderr(std::process::Stdio::inherit())
            .kill_on_drop(true)
            .output(),
    )
    .await;
    fixture.server.inner.shutdown.cancel();
    tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let output = result.unwrap().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let report: Value =
        serde_json::from_str(&stdout).unwrap_or_else(|_| panic!("{stdout}\n{stderr}"));
    assert!(output.status.success(), "{report:#}\n{stderr}");
    assert_eq!(report["ok"], true);
    assert_eq!(
        report["pages"][0]["mailAccessCrud"],
        json!({
            "approved":true,"blocked":true,"dismissed":true,"broadcast":true,
            "filtered":true,"removedOnlyWriter":true,"reload":true
        })
    );
    let expected = expected(original);
    assert_eq!(stored(&fixture), expected);
    fixture.reopen().await;
    assert_eq!(stored(&fixture), expected);
    assert!(fixture.remote.requests.lock().unwrap().is_empty());
}

async fn seed(fixture: &Fixture) -> Value {
    let mut workspaces = Vec::new();
    for id in ["writer", "reader", "off"] {
        actor(fixture, id, if id == "off" { "off" } else { "agent_all" }).await;
        let mut state = acl("");
        state["pending"] = json!([pending(id, 1)]);
        if id == "writer" {
            let mut dismiss = pending(id, 2);
            dismiss["sender_address"] = json!("dismiss@example.com");
            state["pending"].as_array_mut().unwrap().push(dismiss);
        }
        workspaces.push(json!({"data_key":fixture.data_key(id),"agents":{id:state}}));
    }
    let original = json!({"version":2,"workspaces":workspaces});
    fixture
        .server
        .inner
        .core
        .write_mail_access_control_data(&original.to_string())
        .unwrap();
    original
}

fn expected(mut original: Value) -> Value {
    let writer = &mut original["workspaces"][0]["agents"]["writer"];
    writer["approved_replay"] = json!([writer["pending"][0]]);
    writer["pending"] = json!([]);
    writer["whitelist"] = json!({"same@example.com":{"remark":"original","display_name":"writer"}});
    let reader = &mut original["workspaces"][1]["agents"]["reader"];
    reader["pending"] = json!([]);
    reader["blacklist"] = json!({"same@example.com":{"remark":"original","display_name":"reader"}});
    reader["whitelist"] =
        json!({"*@example.org":{"remark":"Shared contact","display_name":"Team"}});
    original
}
