use super::scope::scoped;
use super::*;

async fn add_writer(fixture: &Fixture) {
    fixture
        .request(
            "POST",
            "/api/agents",
            json!({"id":"writer","name":"Writer"}),
        )
        .await;
}
use pretty_assertions::assert_eq;

#[tokio::test]
async fn public_job_http_controls_preserve_another_agents_identical_public_id() {
    let mut fixture = Fixture::new().await;
    add_writer(&fixture).await;
    let first = fixture.create(json!({}), "first", false).await;
    let second = fixture.create(json!({}), "second", false).await;
    let mut data = read_data(&fixture.server).unwrap();
    data.public_ids = BTreeMap::from([
        (first.clone(), String::from("shared")),
        (second.clone(), String::from("shared")),
    ]);
    fixture.bind_job(&mut data, &second, "writer");
    write_data(&fixture.server, &data).unwrap();
    fixture.reopen().await;
    let original = read_data(&fixture.server).unwrap();
    let mut expected = serde_json::to_value(&original.jobs[0]).unwrap();
    expected["id"] = json!("shared");
    assert_eq!(
        fixture.request("GET", "/api/cron/jobs", Value::Null).await,
        json!([expected])
    );
    assert_eq!(
        fixture
            .request("GET", "/api/cron/jobs/shared", Value::Null)
            .await,
        json!({"spec":expected,"state":original.states[&first]})
    );
    for id in [&first, &second] {
        assert_eq!(
            scoped(
                &fixture,
                "default",
                "GET",
                &format!("/api/cron/jobs/{id}"),
                Value::Null
            )
            .await,
            (StatusCode::NOT_FOUND, json!({"detail":"job not found"}))
        );
    }
    expected["name"] = json!("edited");
    expected["task_type"] = json!("text");
    expected["text"] = json!("namespaced text");
    expected["request"] = Value::Null;
    expected["save_result_to_inbox"] = json!(true);
    let mut forged = expected.clone();
    forged["public_id"] = json!("forged");
    forged["public_ids"] = json!({first.clone():"forged"});
    assert_eq!(
        fixture
            .request("PUT", "/api/cron/jobs/shared", forged)
            .await,
        expected
    );
    control_shared_text(&fixture).await;
    assert_eq!(
        fixture
            .request("GET", "/api/cron/jobs/shared/state", Value::Null)
            .await,
        serde_json::to_value(&read_data(&fixture.server).unwrap().states[&first]).unwrap()
    );
    assert_eq!(
        fixture
            .request("DELETE", "/api/cron/jobs/shared", Value::Null)
            .await,
        json!({"deleted":true})
    );
    let after = serde_json::to_value(read_data(&fixture.server).unwrap()).unwrap();
    let mut remaining = serde_json::to_value(original).unwrap();
    remaining["jobs"] = json!([remaining["jobs"][1]]);
    for field in ["states", "public_ids", "workspace_owners"] {
        remaining[field].as_object_mut().unwrap().remove(&first);
    }
    remaining["scheduled"] = json!([second]);
    assert_eq!(after, remaining);
    assert_eq!(
        fixture.request("GET", "/api/cron/jobs", Value::Null).await,
        json!([])
    );
}

async fn control_shared_text(fixture: &Fixture) {
    for (action, response) in [
        ("resume", json!({"resumed":true})),
        ("pause", json!({"paused":true})),
    ] {
        assert_eq!(
            fixture
                .request(
                    "POST",
                    &format!("/api/cron/jobs/shared/{action}"),
                    Value::Null
                )
                .await,
            response
        );
    }
    assert_eq!(
        fixture
            .request("GET", "/api/cron/jobs/shared/history", Value::Null)
            .await,
        json!([])
    );
    fixture.run("shared").await;
    let history = fixture
        .request("GET", "/api/cron/jobs/shared/history", Value::Null)
        .await;
    assert_eq!(
        history,
        json!([{"run_at":history[0]["run_at"],"status":"success","error":null,"trigger":"manual"}])
    );
    let inbox = fixture.inbox();
    assert_eq!(inbox["events"].as_array().unwrap().len(), 1);
    assert_eq!(inbox["events"][0]["source_id"], "shared");
    assert_eq!(inbox["events"][0]["payload"]["job_id"], "shared");
    assert_eq!(inbox["events"][0]["body"], "namespaced text");
}

#[tokio::test]
async fn mapped_agent_runs_keep_public_trace_inbox_and_session_ids_across_restart() {
    let mut fixture = Fixture::new().await;
    let key = fixture
        .create(json!({"share_session":false}), "write fixture", true)
        .await;
    let mut data = read_data(&fixture.server).unwrap();
    data.public_ids
        .insert(key.clone(), String::from("visible-job"));
    write_data(&fixture.server, &data).unwrap();
    for _ in 0..2 {
        fixture.reopen().await;
        fixture.run("visible-job").await;
        fixture.idle().await;
        assert_eq!(
            read_data(&fixture.server).unwrap().states[&key]
                .last_status
                .as_deref(),
            Some("success")
        );
    }
    let inbox = fixture.inbox();
    for event in inbox["events"].as_array().unwrap() {
        assert_eq!(event["source_id"], "visible-job");
        assert_eq!(event["payload"]["job_id"], "visible-job");
    }
    for trace in inbox["traces"].as_object().unwrap().values() {
        assert_eq!(trace["meta"]["job_id"], "visible-job");
        assert_eq!(trace["meta"]["session_id"], "target:cron:visible-job");
    }
    let catalog = fixture.catalog();
    let chats = catalog["chats"].as_object().unwrap();
    assert_eq!(chats.len(), 1);
    assert_eq!(
        chats.values().next().unwrap()["session_id"],
        "target:cron:visible-job"
    );
    assert_eq!(inbox["events"].as_array().unwrap().len(), 2);
    assert_eq!(
        fixture
            .request("GET", "/api/cron/jobs/visible-job/history", Value::Null)
            .await
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert!(!inbox.to_string().contains(&key));
    assert_eq!(fixture.remote.requests.lock().unwrap().len(), 4);
}

#[tokio::test]
#[ignore = "requires console/dist, Node 24+ and Chrome; run explicitly for browser acceptance"]
async fn original_mapped_cron_page_controls_one_namespace_without_touching_its_duplicate() {
    let mut fixture = Fixture::new().await;
    add_writer(&fixture).await;
    let first = fixture
        .create(json!({"share_session":false}), "write fixture", true)
        .await;
    let second = fixture.create(json!({}), "other Agent", false).await;
    let mut data = read_data(&fixture.server).unwrap();
    data.jobs[0].name = String::from("Cron browser fixture");
    data.jobs[0].dispatch.target.session_id = String::from("cron-browser-session");
    data.jobs[0].dispatch.silent = true;
    data.public_ids = BTreeMap::from([
        (first.clone(), String::from("visible-browser-job")),
        (second.clone(), String::from("visible-browser-job")),
    ]);
    fixture.bind_job(&mut data, &second, "writer");
    write_data(&fixture.server, &data).unwrap();
    fixture.server.inner.core.write_ui_language("en").unwrap();
    fixture.reopen().await;
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    fixture.server.inner.shutdown.cancel();
    shutdown(&fixture.server).await;
    fixture.server = AppServer::new_desktop_with_stores_and_workspace(
        fixture.server.inner.core.clone(),
        &root.join("../console/dist"),
        String::from("mapped-cron-shutdown"),
        Arc::new(Credentials),
        &fixture.directory.path().join("data"),
        &fixture.directory.path().join("workspace"),
    )
    .unwrap();
    let original = serde_json::to_value(read_data(&fixture.server).unwrap()).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(fixture.server.clone().run_http(listener));
    let result = tokio::time::timeout(
        Duration::from_secs(90),
        tokio::process::Command::new("node")
            .arg(root.join("scripts/console_browser_smoke.mjs"))
            .args([&origin, "/cron-jobs", "--cron-mapped-crud"])
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
    assert_eq!(
        report["pages"][0]["cronCrud"],
        json!({"created":false,"toggled":true,"manual":true,"history":true,"edited":true,"reload":true,"deleted":true,"agent":true,"mapped":true})
    );
    let remaining = serde_json::to_value(read_data(&fixture.server).unwrap()).unwrap();
    let mut expected = original;
    expected["jobs"] = json!([expected["jobs"][1]]);
    for field in ["states", "public_ids", "workspace_owners"] {
        expected[field].as_object_mut().unwrap().remove(&first);
    }
    expected["scheduled"] = json!([second]);
    assert_eq!(remaining, expected);
    assert_eq!(fixture.remote.requests.lock().unwrap().len(), 2);
    assert_eq!(
        std::fs::read_to_string(fixture.directory.path().join("workspace/cron-output.txt"))
            .unwrap(),
        "created by Cron"
    );
}
