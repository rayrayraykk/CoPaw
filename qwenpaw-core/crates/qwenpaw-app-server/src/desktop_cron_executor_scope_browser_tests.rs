use super::*;
use pretty_assertions::assert_eq;

#[tokio::test]
#[ignore = "requires console/dist, Node 24+ and Chrome; run explicitly for browser acceptance"]
async fn original_agents_page_stops_scoped_cron_without_touching_default_approval() {
    let mut fixture = Fixture::new().await;
    let (first, second) = pair(&fixture, true, "write fixture").await;
    fixture
        .request(
            "POST",
            "/api/agents",
            json!({"id":"editor","name":"Editor",
        "active_model":{"provider_id":"openai-compatible","model":"writer-model"}}),
        )
        .await;
    let third = fixture
        .create(json!({"tool_safety":true}), "write fixture", true)
        .await;
    let mut data = read_data(&fixture.server).unwrap();
    fixture.bind_job(&mut data, &third, "editor");
    data.public_ids
        .insert(third.clone(), String::from("shared-job"));
    write_data(&fixture.server, &data).unwrap();
    fixture.server.inner.core.write_ui_language("en").unwrap();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    fixture.server.inner.shutdown.cancel();
    shutdown(&fixture.server).await;
    fixture.server = AppServer::new_desktop_with_stores_and_workspace(
        fixture.server.inner.core.clone(),
        &root.join("../console/dist"),
        String::from("cron-agent-stop-shutdown"),
        Arc::new(Credentials),
        &fixture.directory.path().join("data"),
        &fixture.directory.path().join("workspace"),
    )
    .unwrap();
    for actor in ["default", "writer", "writer", "editor"] {
        start_public(&fixture, actor).await;
    }
    pending(&fixture, 3).await;
    let before = serde_json::to_value(read_data(&fixture.server).unwrap()).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(fixture.server.clone().run_http(listener));
    let result = tokio::time::timeout(
        Duration::from_secs(90),
        tokio::process::Command::new("node")
            .arg(root.join("scripts/console_browser_smoke.mjs"))
            .args([&origin, "/agents", "--cron-agent-stop"])
            .kill_on_drop(true)
            .output(),
    )
    .await;
    let after = serde_json::to_value(read_data(&fixture.server).unwrap()).unwrap();
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
    assert_eq!(
        report["pages"][0]["cronAgentStop"],
        json!({"disabled":true,"reenabled":true,"deleted":true,"selectedFallback":true,"defaultPending":true,"reload":true})
    );
    assert_eq!(after["jobs"], before["jobs"]);
    assert_eq!(after["states"][&first], before["states"][&first]);
    assert_eq!(after["active_runs"].as_object().unwrap().len(), 1);
    assert_eq!(
        after["states"][&second],
        json!({"next_run_at":null,"last_run_at":null,"last_status":null,"last_error":null})
    );
    assert_eq!(after["states"][&third]["last_status"], "cancelled");
    assert_eq!(after["history"][&second].as_array().unwrap().len(), 2);
    assert_eq!(after["history"][&third].as_array().unwrap().len(), 1);
    for path in [
        "workspace/cron-output.txt",
        "data/workspaces/writer/cron-output.txt",
        "data/workspaces/editor/cron-output.txt",
    ] {
        assert!(!fixture.directory.path().join(path).exists());
    }
    assert!(
        fixture
            .directory
            .path()
            .join("data/workspaces/editor/agent.json")
            .is_file()
    );
    assert_eq!(fixture.remote.requests.lock().unwrap().len(), 3);
}
