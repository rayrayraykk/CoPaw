#![allow(clippy::too_many_lines, clippy::needless_pass_by_value)]

use std::sync::{Arc, Mutex};

use axum::extract::OriginalUri;
use pretty_assertions::assert_eq;
use qwenpaw_core::{Core, ModelConfig};

use super::*;
use crate::DesktopCredentialStore;

#[path = "desktop_plugin_market_tests.rs"]
mod plugins;

#[path = "desktop_product_version_tests.rs"]
mod product_version;

#[path = "desktop_official_plugin_tests.rs"]
mod official;

struct Credentials;

impl DesktopCredentialStore for Credentials {
    fn load_api_key(&self) -> anyhow::Result<Option<String>> {
        Ok(None)
    }
    fn save_api_key(&self, _: Option<&str>) -> anyhow::Result<()> {
        anyhow::bail!("market fixture must not write credentials")
    }
}

#[derive(Debug, PartialEq)]
struct Request {
    path: String,
    query: BTreeMap<String, String>,
    headers: HeaderMap,
}

#[derive(Default)]
struct Remote {
    responses: BTreeMap<String, (StatusCode, Vec<u8>)>,
    requests: Vec<Request>,
    gate: Option<(String, Arc<tokio::sync::Notify>, Arc<tokio::sync::Notify>)>,
}

async fn remote_request(
    State(remote): State<Arc<Mutex<Remote>>>,
    OriginalUri(uri): OriginalUri,
    Query(query): Query<BTreeMap<String, String>>,
    headers: HeaderMap,
) -> (StatusCode, Vec<u8>) {
    let (response, gate) = {
        let mut remote = remote.lock().unwrap();
        remote.requests.push(Request {
            path: uri.path().to_owned(),
            query,
            headers,
        });
        let response = remote
            .responses
            .get(&uri.to_string())
            .or_else(|| remote.responses.get(uri.path()))
            .cloned()
            .unwrap_or((
                StatusCode::NOT_FOUND,
                b"unconfigured fixture response".to_vec(),
            ));
        let gate = remote
            .gate
            .as_ref()
            .filter(|(path, _, _)| path == uri.path())
            .cloned();
        (response, gate)
    };
    if let Some((_, started, release)) = gate {
        started.notify_one();
        release.notified().await;
    }
    response
}

struct Fixture {
    directory: tempfile::TempDir,
    server: AppServer,
    base: String,
    http: tokio::task::JoinHandle<anyhow::Result<()>>,
    remote: Arc<Mutex<Remote>>,
    remote_task: tokio::task::JoinHandle<()>,
}

impl Fixture {
    async fn new() -> Self {
        Self::with_browser(false).await
    }

    async fn with_browser(browser: bool) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let console = directory.path().join("console");
        let workspace = directory.path().join("workspace");
        std::fs::create_dir_all(&console).unwrap();
        std::fs::create_dir_all(&workspace).unwrap();
        std::fs::write(console.join("index.html"), "fixture").unwrap();
        let console = if browser {
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../../console/dist")
                .canonicalize()
                .unwrap()
        } else {
            console
        };
        let core = Core::persistent(
            ModelConfig {
                api_key: None,
                base_url: String::from("http://127.0.0.1:1/v1"),
                default_model: String::from("fixture-model"),
            },
            &directory.path().join("data/threads.sqlite3"),
        )
        .unwrap();
        core.write_ui_language("en").unwrap();
        let mut server = AppServer::new_desktop_with_stores_and_workspace(
            core,
            &console,
            String::from("market-fixture-shutdown"),
            Arc::new(Credentials),
            &directory.path().join("data"),
            &workspace,
        )
        .unwrap();
        // Explicit values suppress process credentials, including empty STS/endpoint values.
        server
            .inner
            .core
            .replace_runtime_environment(BTreeMap::from([
                (
                    String::from("ALIBABA_CLOUD_ACCESS_KEY_ID"),
                    String::from("fixture-id"),
                ),
                (
                    String::from("ALIBABA_CLOUD_ACCESS_KEY_SECRET"),
                    String::from("fixture-secret"),
                ),
                (
                    String::from("ALIBABA_CLOUD_SECURITY_TOKEN"),
                    String::from("fixture-token"),
                ),
                (String::from("ALIYUN_AGENTEXPLORER_ENDPOINT"), String::new()),
            ]))
            .unwrap();
        let remote = Arc::new(Mutex::new(Remote::default()));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        Arc::get_mut(&mut server.inner).unwrap().desktop_market = Sources {
            qwenpaw: format!("{base}/qwenpaw"),
            clawhub: format!("{base}/clawhub"),
            modelscope: format!("{base}/modelscope"),
            aliyun: format!("{base}/aliyun"),
            download: format!("{base}/download"),
        };
        let mut environment = server.inner.core.runtime_environment().unwrap();
        for (key, value) in [
            ("QWENPAW_SKILLS_HUB_BASE_URL", format!("{base}/clawhub")),
            (
                "QWENPAW_SKILLS_HUB_DETAIL_PATH",
                String::from("/api/v1/skills/{slug}"),
            ),
            (
                "QWENPAW_SKILLS_HUB_VERSION_PATH",
                String::from("/api/v1/skills/{slug}/versions/{version}"),
            ),
            (
                "QWENPAW_SKILLS_HUB_FILE_PATH",
                String::from("/api/v1/skills/{slug}/file"),
            ),
            (
                "QWENPAW_SKILLS_HUB_SEARCH_PATH",
                String::from("/api/v1/search"),
            ),
        ] {
            environment.insert(key.to_owned(), value);
        }
        server
            .inner
            .core
            .replace_runtime_environment(environment)
            .unwrap();
        let router = Router::new()
            .fallback(remote_request)
            .with_state(remote.clone());
        let remote_task = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let http = tokio::spawn(server.clone().run_http(listener));
        Self {
            directory,
            server,
            base,
            http,
            remote,
            remote_task,
        }
    }

    fn respond(&self, path: &str, payload: Value) {
        self.remote.lock().unwrap().responses.insert(
            path.to_owned(),
            (StatusCode::OK, serde_json::to_vec(&payload).unwrap()),
        );
    }

    async fn search(&self, body: &str) -> Value {
        let response = reqwest::Client::new()
            .post(format!("{}/api/market/search", self.base))
            .header("content-type", "application/json")
            .body(body.to_owned())
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        response.json().await.unwrap()
    }

    async fn shutdown(self) {
        self.server.inner.shutdown.cancel();
        tokio::time::timeout(Duration::from_secs(5), self.http)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        self.remote_task.abort();
    }

    async fn install(&self, source: &str, version: &str, name: &str) -> Value {
        let response = reqwest::Client::new()
            .post(format!("{}/api/skills/hub/install/start", self.base))
            .json(&json!({"bundle_url":source,"version":version,"target_name":name,"enable":true}))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let task: Value = response.json().await.unwrap();
        let id = task["task_id"].as_str().unwrap();
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let response =
                    reqwest::get(format!("{}/api/skills/hub/install/status/{id}", self.base))
                        .await
                        .unwrap();
                let task: Value = response.json().await.unwrap();
                if matches!(
                    task["status"].as_str(),
                    Some("completed" | "failed" | "cancelled")
                ) {
                    break task;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap()
    }
}

fn skill_content(name: &str) -> String {
    format!(
        "---\nname: {name}\ndescription: Fixture skill\n---\n# Fixture\nUse this fixture for tests.\n"
    )
}

#[tokio::test]
#[ignore = "requires console/dist, Node 24+ and Chrome; run explicitly for browser acceptance"]
async fn original_market_browser_searches_and_installs_from_detail() {
    let fixture = Fixture::with_browser(true).await;
    let uuid = "12345678-1234-1234-1234-123456789abc";
    let mut rows = vec![
        json!({"id":uuid,"display_name":"Fixture Market Skill","description":"A local fixture skill","developer":"Fixture","category":"engineering development"}),
    ];
    rows.extend((1..10).map(|index| json!({"id":format!("@fixture/skill-{index}"),"display_name":format!("Fixture Skill {index}")})));
    fixture.respond(
        "/qwenpaw/openapi/v1/skills",
        json!({"success":true,"data":{"total":11,"skills":rows}}),
    );
    for query in ["", "&category=engineering+development", "&search=fixture"] {
        fixture.respond(&format!("/qwenpaw/openapi/v1/skills?page_size=10&page_number=2{query}"),
            json!({"success":true,"data":{"total":11,"skills":[{"id":"@fixture/more","display_name":"More Fixture Skill"}]}}));
    }
    fixture.remote.lock().unwrap().responses.insert(
        format!("/qwenpaw/api/v1/skills/{uuid}/download"),
        (StatusCode::OK, skill_zip("browser_market", false)),
    );
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = tokio::time::timeout(
        Duration::from_secs(90),
        tokio::process::Command::new("node")
            .arg(root.join("scripts/console_browser_smoke.mjs"))
            .args([&fixture.base, "/market?tab=skills", "--market-crud"])
            .kill_on_drop(true)
            .output(),
    )
    .await
    .unwrap()
    .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let report: Value =
        serde_json::from_str(&stdout).unwrap_or_else(|_| panic!("{stdout}\n{stderr}"));
    assert!(output.status.success(), "{report:#}\n{stderr}");
    assert_eq!(report["ok"], true);
    let expected = json!({"browse":true,"category":true,"search":true,"pagination":true,"detail":true,"installQueue":true,"persistedSkill":true});
    assert_eq!(report["pages"][0]["marketCrud"], expected);
    assert_eq!(
        std::fs::read_to_string(
            fixture
                .directory
                .path()
                .join("workspace/skills/browser_market/SKILL.md")
        )
        .unwrap(),
        skill_content("browser_market")
    );
    println!("{expected:#}");
    fixture.shutdown().await;
}

fn skill_zip(name: &str, nested: bool) -> Vec<u8> {
    use std::io::Write as _;
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    writer
        .start_file(
            if nested {
                "archive/SKILL.md"
            } else {
                "SKILL.md"
            },
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap();
    writer.write_all(skill_content(name).as_bytes()).unwrap();
    writer.finish().unwrap().into_inner()
}

#[tokio::test]
async fn market_commit_and_completion_share_the_cancellation_fence() {
    let fixture = Fixture::new().await;
    fixture.respond(
        "/clawhub/api/v1/skills/claw",
        json!({"skill":{"slug":"claw"},"version":{"version":"v1","files":[{"path":"SKILL.md"}]}}),
    );
    fixture.remote.lock().unwrap().responses.insert(
        String::from("/clawhub/api/v1/skills/claw/file"),
        (StatusCode::OK, skill_content("claw").into_bytes()),
    );
    let started = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    fixture.remote.lock().unwrap().gate = Some((
        String::from("/clawhub/api/v1/skills/claw/file"),
        started.clone(),
        release.clone(),
    ));
    let client = reqwest::Client::new();
    let task: Value = client
        .post(format!("{}/api/skills/hub/install/start", fixture.base))
        .json(&json!({"bundle_url":"https://clawhub.ai/claw"}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let id = task["task_id"].as_str().unwrap();
    tokio::time::timeout(Duration::from_secs(2), started.notified())
        .await
        .unwrap();
    let tasks = fixture.server.inner.desktop_skill_tasks.write().await;
    release.notify_one();
    // The commit must wait for task-state ownership before touching live files.
    tokio::time::timeout(Duration::from_secs(2), async {
        while fixture.server.inner.desktop_skills_lock.try_lock().is_ok() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("installer did not reserve the commit fence");
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(
        !fixture
            .directory
            .path()
            .join("workspace/skills/claw")
            .exists()
    );
    assert_eq!(
        serde_json::to_value(&tasks[id]).unwrap()["status"],
        "importing"
    );
    let cancel_url = format!("{}/api/skills/hub/install/cancel/{id}", fixture.base);
    let pending_cancel = tokio::spawn(async move {
        reqwest::Client::new()
            .post(cancel_url)
            .send()
            .await
            .unwrap()
            .json::<Value>()
            .await
            .unwrap()
    });
    drop(tasks);
    let cancelled = tokio::time::timeout(Duration::from_secs(2), pending_cancel)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(cancelled, json!({"task_id":id,"status":"completed"}));
    let task: Value = client
        .get(format!(
            "{}/api/skills/hub/install/status/{id}",
            fixture.base
        ))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(task["status"], "completed");
    assert_eq!(
        task["result"],
        json!({"installed":true,"name":"claw","enabled":true,"source_url":"https://clawhub.ai/claw","installed_from":"clawhub"})
    );
    assert_eq!(
        std::fs::read_to_string(
            fixture
                .directory
                .path()
                .join("workspace/skills/claw/SKILL.md")
        )
        .unwrap(),
        skill_content("claw")
    );
    fixture.shutdown().await;
}

#[tokio::test]
async fn market_cancel_during_stalled_file_headers_drops_worker_without_installing() {
    let fixture = Fixture::new().await;
    fixture.respond(
        "/clawhub/api/v1/skills/claw",
        json!({"skill":{"slug":"claw"},"version":{"version":"v1","files":[{"path":"SKILL.md"}]}}),
    );
    fixture.remote.lock().unwrap().responses.insert(
        String::from("/clawhub/api/v1/skills/claw/file"),
        (StatusCode::OK, skill_content("claw").into_bytes()),
    );
    let started = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    fixture.remote.lock().unwrap().gate = Some((
        String::from("/clawhub/api/v1/skills/claw/file"),
        started.clone(),
        release.clone(),
    ));
    let client = reqwest::Client::new();
    let task: Value = client
        .post(format!("{}/api/skills/hub/install/start", fixture.base))
        .json(&json!({"bundle_url":"https://clawhub.ai/claw"}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let id = task["task_id"].as_str().unwrap();
    tokio::time::timeout(Duration::from_secs(2), started.notified())
        .await
        .unwrap();
    let cancelled: Value = client
        .post(format!(
            "{}/api/skills/hub/install/cancel/{id}",
            fixture.base
        ))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(cancelled, json!({"task_id":id,"status":"cancelled"}));
    tokio::time::timeout(Duration::from_secs(2), async {
        while fixture
            .server
            .inner
            .desktop_skill_cancellations
            .read()
            .await
            .contains_key(id)
        {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let task: Value = client
        .get(format!(
            "{}/api/skills/hub/install/status/{id}",
            fixture.base
        ))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(task["status"], "cancelled");
    assert_eq!(task["result"], Value::Null);
    assert_eq!(task["error"], Value::Null);
    assert!(
        !fixture
            .directory
            .path()
            .join("workspace/skills/claw")
            .exists()
    );
    release.notify_one();
    fixture.remote.lock().unwrap().gate = None;
    assert_eq!(
        fixture.install("https://clawhub.ai/claw", "", "claw").await["status"],
        "completed"
    );
    fixture.shutdown().await;
}

#[tokio::test]
async fn market_all_four_detail_sources_install_workspace_and_pool_without_python() {
    let fixture = Fixture::new().await;
    let uuid = "12345678-1234-1234-1234-123456789abc";
    fixture.remote.lock().unwrap().responses.extend([
        (
            format!("/qwenpaw/api/v1/skills/{uuid}/download"),
            (StatusCode::OK, skill_zip("qwen", false)),
        ),
        (
            String::from("/qwenpaw/skills/@owner/named/archive/zip/old"),
            (StatusCode::OK, skill_zip("named", true)),
        ),
        (
            String::from("/modelscope/skills/@owner/model/archive/zip/feature%2Ftest"),
            (StatusCode::OK, skill_zip("model", true)),
        ),
        (
            String::from("/clawhub/api/v1/skills/claw/file?path=SKILL.md&version=v2"),
            (StatusCode::OK, skill_content("claw").into_bytes()),
        ),
        (
            String::from("/clawhub/api/v1/skills/claw/file?path=references%2Fguide.md&version=v2"),
            (StatusCode::OK, b"Fixture reference".to_vec()),
        ),
    ]);
    fixture.respond("/clawhub/api/v1/skills/claw", json!({"skill":{"slug":"claw","displayName":"Claw Title"},"latestVersion":{"version":"v1"}}));
    fixture.respond("/clawhub/api/v1/skills/claw/versions/v2", json!({"version":{"version":"v2","files":[{"path":"SKILL.md"},{"path":"references/guide.md"}]}}));
    fixture.respond(
        "/aliyun/openapi/skills/cloud%2Fone",
        json!({"requestId":"fixture","content":skill_content("cloud")}),
    );
    let sources = [
        (
            format!("https://platform.agentscope.io/skills/{uuid}"),
            "",
            "qwen",
            "qwenpaw",
        ),
        (
            String::from("https://platform.agentscope.io/skills/@owner/named/archive/zip/old.zip"),
            "",
            "named",
            "qwenpaw",
        ),
        (
            String::from("https://modelscope.cn/skills/@owner/model"),
            "feature/test",
            "model",
            "modelscope",
        ),
        (
            String::from("https://clawhub.ai/alice/claw"),
            "v2",
            "claw",
            "clawhub",
        ),
        (
            String::from("https://api.aliyun.com/agentexplorer/skills/cloud%2Fone"),
            "ignored",
            "cloud",
            "aliyun",
        ),
    ];
    let client = reqwest::Client::new();
    for (source, version, name, origin) in &sources {
        let task = fixture.install(source, version, name).await;
        assert_eq!(task["status"], "completed", "{task}");
        assert_eq!(task["error"], Value::Null);
        assert_eq!(
            task["result"],
            json!({"installed":true,"name":name,"enabled":true,"source_url":source,"installed_from":origin})
        );
        assert_eq!(
            std::fs::read_to_string(
                fixture
                    .directory
                    .path()
                    .join("workspace/skills")
                    .join(name)
                    .join("SKILL.md")
            )
            .unwrap(),
            skill_content(name)
        );
        let pool = client
            .post(format!("{}/api/skills/pool/import", fixture.base))
            .json(&json!({"bundle_url":source,"version":version,"enable":true}))
            .send()
            .await
            .unwrap();
        assert_eq!(pool.status(), StatusCode::OK);
        assert_eq!(
            pool.json::<Value>().await.unwrap(),
            json!({"installed":true,"name":name,"enabled":false,"source_url":source,"installed_from":origin})
        );
        assert_eq!(
            std::fs::read_to_string(
                fixture
                    .directory
                    .path()
                    .join("data/skill_pool")
                    .join(name)
                    .join("SKILL.md")
            )
            .unwrap(),
            skill_content(name)
        );
    }
    assert_eq!(
        std::fs::read_to_string(
            fixture
                .directory
                .path()
                .join("workspace/skills/claw/references/guide.md")
        )
        .unwrap(),
        "Fixture reference"
    );
    let manifest: Value = serde_json::from_slice(
        &std::fs::read(fixture.directory.path().join("workspace/skill.json")).unwrap(),
    )
    .unwrap();
    for (_, _, name, origin) in &sources {
        assert_eq!(manifest["skills"][name]["installed_from"], *origin);
        assert_eq!(manifest["skills"][name]["enabled"], true);
    }
    let conflict = fixture.install(&sources[0].0, "", "qwen").await;
    assert_eq!(conflict["status"], "failed");
    assert_eq!(conflict["result"]["detail"]["reason"], "conflict");
    {
        let remote = fixture.remote.lock().unwrap();
        let signed = remote
            .requests
            .iter()
            .filter(|request| request.path.starts_with("/aliyun"))
            .collect::<Vec<_>>();
        assert_eq!(signed.len(), 2);
        for request in signed {
            assert_eq!(request.headers["x-acs-action"], "GetSkillContent");
        }
        assert_eq!(
            remote
                .requests
                .iter()
                .filter(|request| request.path.contains("/versions/v2"))
                .count(),
            2
        );
    }
    fixture.shutdown().await;
}

#[tokio::test]
async fn market_failed_missing_and_unsafe_files_do_not_install_partial_skills() {
    let fixture = Fixture::new().await;
    fixture.respond("/clawhub/api/v1/skills/claw", json!({"skill":{"slug":"claw"},"version":{"version":"v1","files":[{"path":"SKILL.md"},{"path":"references/missing.md"}]}}));
    fixture.remote.lock().unwrap().responses.insert(
        String::from("/clawhub/api/v1/skills/claw/file?path=SKILL.md&version=v1"),
        (StatusCode::OK, skill_content("claw").into_bytes()),
    );
    let result = fixture.install("https://clawhub.ai/claw", "", "claw").await;
    assert_eq!(result["status"], "failed");
    assert!(
        !fixture
            .directory
            .path()
            .join("workspace/skills/claw")
            .exists()
    );
    for path in ["../escape", "C:\\escape", "/absolute", "references\\escape"] {
        fixture.respond(
            "/clawhub/api/v1/skills/claw",
            json!({"skill":{"slug":"claw"},"version":{"version":"v1","files":[{"path":path}]}}),
        );
        let result = fixture.install("https://clawhub.ai/claw", "", "claw").await;
        assert_eq!(result["status"], "failed");
        assert_eq!(
            result["result"],
            json!({"detail":"ClawHub file path is invalid"})
        );
    }
    fixture.respond("/aliyun/openapi/skills/cloud", json!({"content":""}));
    let result = fixture
        .install(
            "https://api.aliyun.com/agentexplorer/skills/cloud",
            "",
            "cloud",
        )
        .await;
    assert_eq!(result["status"], "failed");
    assert_eq!(
        result["result"],
        json!({"detail":"Aliyun GetSkillContent response is missing content"})
    );
    assert!(
        !fixture
            .directory
            .path()
            .join("workspace/skills/cloud")
            .exists()
    );
    fixture.shutdown().await;
}

fn item(source: &str, slug: &str, name: &str, source_url: &str) -> Value {
    json!({"source": source, "slug": slug, "name": name, "description": null,
        "source_url": source_url, "version": null, "author": null, "icon_url": null, "stats": null})
}

#[tokio::test]
async fn market_providers_categories_and_empty_search_preserve_contract() {
    let fixture = Fixture::new().await;
    let client = reqwest::Client::new();
    let read_providers = || {
        client
            .get(format!("{}/api/market/providers", fixture.base))
            .send()
    };
    let expected = json!([
        {"key":"qwenpaw","label":"QwenPaw","available":true,"reason":null,"supports_browse":true},
        {"key":"clawhub","label":"ClawHub","available":true,"reason":null,"supports_browse":true},
        {"key":"modelscope","label":"ModelScope","available":true,"reason":null,"supports_browse":true},
        {"key":"aliyun","label":"Aliyun","available":true,"reason":null,"supports_browse":true}
    ]);
    assert_eq!(
        read_providers()
            .await
            .unwrap()
            .json::<Value>()
            .await
            .unwrap(),
        expected
    );
    let mut environment = fixture.server.inner.core.runtime_environment().unwrap();
    environment.insert(
        String::from("ALIBABA_CLOUD_ACCESS_KEY_SECRET"),
        String::new(),
    );
    fixture
        .server
        .inner
        .core
        .replace_runtime_environment(environment)
        .unwrap();
    let mut unavailable = expected;
    unavailable[3]["available"] = json!(false);
    unavailable[3]["reason"] = json!(
        "missing env vars: ALIBABA_CLOUD_ACCESS_KEY_SECRET (set Aliyun AK/SK so requests can be signed)"
    );
    assert_eq!(
        read_providers()
            .await
            .unwrap()
            .json::<Value>()
            .await
            .unwrap(),
        unavailable
    );
    let ids = [
        "app",
        "engineering-development",
        "data-research",
        "document-office",
        "design-creation",
        "automation-integration",
        "product-management",
        "marketing-growth",
        "security-compliance",
        "education-knowledge",
        "plugin-development",
        "skills-management",
        "others",
    ];
    for (lang, labels) in [
        (
            "en",
            [
                "Apps",
                "Engineering",
                "Data & Research",
                "Docs & Office",
                "Design",
                "Automation",
                "Product",
                "Marketing",
                "Security",
                "Education",
                "Plugin Dev",
                "Skills",
                "Others",
            ],
        ),
        (
            "zh-CN",
            [
                "应用",
                "工程开发",
                "数据研究",
                "文档办公",
                "设计创作",
                "自动化集成",
                "产品管理",
                "营销增长",
                "安全合规",
                "教育知识",
                "Plugin 开发",
                "Skills 管理",
                "其它",
            ],
        ),
    ] {
        let response = client
            .get(format!(
                "{}/api/market/categories?lang={lang}",
                fixture.base
            ))
            .send()
            .await
            .unwrap();
        assert_eq!(
            response.json::<Value>().await.unwrap(),
            json!(
                ids.into_iter()
                    .zip(labels)
                    .map(|(id, label)| json!({"id":id,"label":label}))
                    .collect::<Vec<_>>()
            )
        );
    }
    assert_eq!(
        fixture.search("{}").await,
        json!({"results":[],"errors":[],"by_provider":{}})
    );
    assert!(fixture.remote.lock().unwrap().requests.is_empty());
    fixture.shutdown().await;
}

#[tokio::test]
async fn market_search_keeps_requested_order_localization_and_native_categories() {
    let fixture = Fixture::new().await;
    fixture.respond("/qwenpaw/openapi/v1/skills", json!({"success":true,"data":{"total":3,"skills":[
        {"id":"@owner/中文","display_name":"Qwen skill","description":"base","developer":"Dev","owner":"ignored",
         "downloads":"12","view_count":4,"category":"base","version":"v2","logo_url":"https://example.com/logo.png",
         "locales":{"zh":{"description":"说明","category":"工程"},"en":{"description":"English"}}}, {"id":""} ]}}));
    fixture.respond(
        "/modelscope/openapi/v1/skills",
        json!({"data":{"total":1,"skills":[{"id":"@alice/model","owner":"ignored"}]}}),
    );
    let mut model = item(
        "modelscope",
        "@alice/model",
        "@alice/model",
        "https://modelscope.cn/skills/@alice/model",
    );
    model["author"] = json!("alice");
    let result = fixture.search(r#"{"provider_pages":{"modelscope":0,"qwenpaw":1},"limit":2,"lang":"zh-CN","category":"engineering-development"}"#).await;
    assert_eq!(
        result,
        json!({"results":[model, {
        "source":"qwenpaw","slug":"@owner/中文","name":"Qwen skill","description":"说明",
        "source_url":"https://platform.agentscope.io/skills/@owner/%E4%B8%AD%E6%96%87","version":"v2",
        "author":"Dev","icon_url":"https://example.com/logo.png","stats":{"downloads":12,"views":4,"category":"工程"}
    }],"errors":[],"by_provider":{"modelscope":{"has_more":false,"total":1},"qwenpaw":{"has_more":true,"total":3}}})
    );
    {
        let remote = fixture.remote.lock().unwrap();
        assert_eq!(remote.requests.len(), 2);
        for request in &remote.requests {
            assert_eq!(
                request.query,
                BTreeMap::from([
                    (String::from("page_number"), String::from("1")),
                    (String::from("page_size"), String::from("2")),
                    if request.path.starts_with("/qwenpaw") {
                        (
                            String::from("category"),
                            String::from("engineering development"),
                        )
                    } else {
                        (
                            String::from("filter.category"),
                            String::from("developer-tools"),
                        )
                    }
                ])
            );
            assert!(!request.headers.contains_key("authorization"));
        }
    }
    fixture.shutdown().await;
}

#[tokio::test]
async fn market_clawhub_keyword_overfetch_paginates_and_uses_legacy_field_priority() {
    let fixture = Fixture::new().await;
    fixture.respond("/clawhub/api/v1/search", json!({"skills":[
        {"slug":"first"}, {"name":"fallback", "displayName":"ignored", "description":"Description", "summary":"ignored",
            "owner":{"displayName":"Alice", "handle":"ignored", "image":"https://example.com/icon"},
            "url":"https://clawhub.ai/alice/fallback", "version":"2"}, {"slug":"last"}, {}
    ]}));
    assert_eq!(
        fixture
            .search(r#"{"provider_pages":{"clawhub":2},"limit":1,"category":"app","lang":"zh"}"#)
            .await,
        json!({"results":[{"source":"clawhub","slug":"fallback","name":"fallback","description":"Description",
            "source_url":"https://clawhub.ai/alice/fallback","version":"2","author":"Alice",
            "icon_url":"https://example.com/icon","stats":null}],"errors":[],"by_provider":{"clawhub":{"has_more":true,"total":3}}})
    );
    assert_eq!(fixture.search(r#"{"provider_pages":{"clawhub":9223372036854775807},"limit":50,"query":" own query ","category":"app"}"#).await,
        json!({"results":[],"errors":[],"by_provider":{"clawhub":{"has_more":false,"total":3}}}));
    {
        let remote = fixture.remote.lock().unwrap();
        assert_eq!(
            remote
                .requests
                .iter()
                .map(|request| &request.query)
                .collect::<Vec<_>>(),
            vec![
                &BTreeMap::from([
                    (String::from("q"), String::from("应用 PawApp")),
                    (String::from("limit"), String::from("500"))
                ]),
                &BTreeMap::from([
                    (String::from("q"), String::from("own query")),
                    (String::from("limit"), String::from("500"))
                ]),
            ]
        );
    }
    fixture.shutdown().await;
}

#[tokio::test]
async fn market_cursor_sources_walk_pages_and_sign_aliyun_with_sts() {
    let fixture = Fixture::new().await;
    fixture.respond(
        "/clawhub/api/v1/skills?limit=1&sort=recommended",
        json!({"items":[{"slug":"first"}],"nextCursor":"two +"}),
    );
    fixture.respond("/clawhub/api/v1/skills?limit=1&sort=recommended&cursor=two+%2B", json!({"items":[{"slug":"last","tags":{"latest":"v1"},"stats":{"downloads":8,"stars":2,"installs":"bad"}}]}));
    fixture.respond(
        "/aliyun/openapi/skills?maxResults=1",
        json!({"data":[{"skillName":"first"}],"nextToken":"two +","totalCount":2}),
    );
    fixture.respond("/aliyun/openapi/skills?maxResults=1&nextToken=two%20%2B", json!({"data":[{"skillName":"folder/中文","displayName":"Cloud","installCount":"4","likeCount":1,"categoryName":"Tools","subCategoryName":"Code","updatedAt":"today"}],"totalCount":"2"}));
    let mut claw = item("clawhub", "last", "last", "https://clawhub.ai/last");
    claw["version"] = json!("v1");
    claw["stats"] = json!({"downloads":8,"stars":2});
    let mut cloud = item(
        "aliyun",
        "folder/中文",
        "Cloud",
        "https://api.aliyun.com/agentexplorer/skills/folder%2F%E4%B8%AD%E6%96%87",
    );
    cloud["stats"] = json!({"installs":4,"likes":1,"category":"Tools / Code","updated_at":"today"});
    assert_eq!(
        fixture
            .search(r#"{"provider_pages":{"clawhub":2,"aliyun":2},"limit":1}"#)
            .await,
        json!({"results":[claw,cloud],"errors":[],"by_provider":{"clawhub":{"has_more":false,"total":0},"aliyun":{"has_more":false,"total":2}}})
    );
    assert_eq!(
        fixture
            .search(r#"{"provider_pages":{"clawhub":51,"aliyun":51}}"#)
            .await,
        json!({"results":[],"errors":[],"by_provider":{"clawhub":{"has_more":false,"total":0},"aliyun":{"has_more":false,"total":0}}})
    );
    {
        let remote = fixture.remote.lock().unwrap();
        assert_eq!(remote.requests.len(), 4);
        let signed = remote
            .requests
            .iter()
            .filter(|request| request.path.starts_with("/aliyun"))
            .collect::<Vec<_>>();
        for request in &signed {
            assert_eq!(request.headers["x-acs-action"], "SearchSkills");
            assert_eq!(request.headers["x-acs-version"], "2026-03-17");
            assert_eq!(request.headers["x-acs-security-token"], "fixture-token");
            let auth = request.headers["authorization"].to_str().unwrap();
            assert!(auth.starts_with("ACS3-HMAC-SHA256 Credential=fixture-id,SignedHeaders="));
            assert!(auth.contains("x-acs-security-token;"));
            assert!(!auth.contains("fixture-secret"));
        }
        assert_ne!(
            signed[0].headers["x-acs-signature-nonce"],
            signed[1].headers["x-acs-signature-nonce"]
        );
    }
    fixture.shutdown().await;
}

#[tokio::test]
async fn market_partial_errors_and_validation_do_not_fake_success_or_leak_upstream_body() {
    let fixture = Fixture::new().await;
    fixture.respond(
        "/modelscope/openapi/v1/skills",
        json!({"data":{"skills":[],"total":0}}),
    );
    fixture.respond(
        "/qwenpaw/openapi/v1/skills",
        json!({"success":false,"message":"fixture-secret"}),
    );
    fixture.remote.lock().unwrap().responses.insert(
        String::from("/clawhub/api/v1/skills"),
        (StatusCode::TEMPORARY_REDIRECT, b"fixture-secret".to_vec()),
    );
    fixture.respond("/aliyun/openapi/skills", json!({"wrong":"fixture-secret"}));
    assert_eq!(
        fixture
            .search(r#"{"provider_pages":{"clawhub":1,"qwenpaw":1,"modelscope":1,"aliyun":1}}"#)
            .await,
        json!({"results":[],"errors":[
            {"provider":"clawhub","message":"Market provider returned HTTP 307"},
            {"provider":"qwenpaw","message":"Market provider reported failure"},
            {"provider":"aliyun","message":"Market provider returned an invalid catalog"}],
            "by_provider":{"modelscope":{"has_more":false,"total":0}}})
    );
    let client = reqwest::Client::new();
    for body in [
        json!({"limit":0}),
        json!({"limit":51}),
        json!({"provider_pages":{"unknown":1}}),
    ] {
        let response = client
            .post(format!("{}/api/market/search", fixture.base))
            .json(&body)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
    assert_eq!(fixture.remote.lock().unwrap().requests.len(), 4);
    fixture.shutdown().await;
}
