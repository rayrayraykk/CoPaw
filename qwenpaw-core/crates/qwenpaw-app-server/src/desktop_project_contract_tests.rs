use std::fs;

use axum::http::StatusCode;
use pretty_assertions::assert_eq;
use serde_json::{Value, json};

use super::Fixture;

#[tokio::test]
async fn directory_browser_preserves_all_entries_beyond_five_hundred() {
    let fixture = Fixture::new().await;
    let root = fixture
        .directory
        .path()
        .canonicalize()
        .unwrap()
        .join("many-directories");
    fs::create_dir(&root).unwrap();
    let mut expected = Vec::new();
    for index in (0..512).rev() {
        let name = format!("directory-{index:04}");
        let path = root.join(&name);
        fs::create_dir(&path).unwrap();
        expected.push(json!({"name":name,"path":path}));
    }
    expected.reverse();
    fs::create_dir(root.join(".hidden")).unwrap();
    fs::write(root.join("not-a-directory"), "fixture").unwrap();
    for actor in ["default", "writer"] {
        for hidden in [false, true] {
            let mut url =
                reqwest::Url::parse("http://localhost/api/workspace/project-directory/browse-dirs")
                    .unwrap();
            url.query_pairs_mut()
                .append_pair("path", root.to_str().unwrap())
                .append_pair("show_hidden", if hidden { "true" } else { "false" });
            let mut dirs = expected.clone();
            if hidden {
                dirs.insert(0, json!({"name":".hidden","path":root.join(".hidden")}));
            }
            assert_eq!(
                fixture
                    .json(
                        "GET",
                        &format!("{}?{}", url.path(), url.query().unwrap()),
                        actor,
                        Value::Null
                    )
                    .await,
                (
                    StatusCode::OK,
                    json!({"current":root,"parent":root.parent(),"dirs":dirs,"selectable":true})
                )
            );
        }
    }
}

#[tokio::test]
async fn selections_survive_server_reopen_and_failed_switch_without_moving_project_storage() {
    let mut fixture = Fixture::new().await;
    let endpoint = "/api/workspace/project-directory";
    let create = format!("{endpoint}/create");
    let list = format!("{endpoint}/list");
    for actor in ["default", "writer"] {
        assert_eq!(
            fixture
                .json("POST", &create, actor, json!({"name":"shared"}))
                .await
                .0,
            StatusCode::OK
        );
    }
    let outside = fixture
        .directory
        .path()
        .canonicalize()
        .unwrap()
        .join("outside");
    fs::create_dir(&outside).unwrap();
    assert_eq!(
        fixture
            .json("PUT", endpoint, "writer", json!({"path":outside}))
            .await
            .0,
        StatusCode::OK
    );
    let mut before = Vec::new();
    for actor in ["default", "writer"] {
        before.push(fixture.json("GET", endpoint, actor, Value::Null).await);
        before.push(fixture.json("GET", &list, actor, Value::Null).await);
    }
    let config_path = fixture.writer.join("agent.json");
    let config = fs::read(&config_path).unwrap();
    let catalog_path = fixture.directory.path().join("data/agents/catalog.json");
    let catalog = fs::read(&catalog_path).unwrap();
    for path in [
        outside.join("missing"),
        fixture.default_root().join("sentinel.txt"),
    ] {
        assert_eq!(
            fixture
                .json("PUT", endpoint, "writer", json!({"path":path}))
                .await
                .0,
            StatusCode::BAD_REQUEST
        );
    }
    assert_eq!(fs::read(config_path).unwrap(), config);
    assert_eq!(fs::read(catalog_path).unwrap(), catalog);
    fixture.reopen();
    let mut after = Vec::new();
    for actor in ["default", "writer"] {
        after.push(fixture.json("GET", endpoint, actor, Value::Null).await);
        after.push(fixture.json("GET", &list, actor, Value::Null).await);
    }
    assert_eq!(after, before);
    let created = fixture
        .json("POST", &create, "writer", json!({"name":"after-reopen"}))
        .await;
    fixture.assert_writer_project("after-reopen", created).await;
    assert!(!outside.join("coding_projects").exists());
    assert_eq!(
        fixture.json("GET", endpoint, "default", Value::Null).await,
        before[0]
    );
}

#[tokio::test]
async fn explicit_base_selection_and_reset_have_original_put_response_shape() {
    let fixture = Fixture::new().await;
    for (body, is_default) in [(json!({"path":fixture.writer}), false), (json!({}), true)] {
        assert_eq!(
            fixture
                .json("PUT", "/api/workspace/project-directory", "writer", body)
                .await,
            (
                StatusCode::OK,
                json!({
                    "path":fixture.writer,"name":"writer","is_workspace_default":is_default
                })
            )
        );
        assert_eq!(
            fixture
                .json(
                    "GET",
                    "/api/workspace/project-directory",
                    "writer",
                    Value::Null
                )
                .await,
            (
                StatusCode::OK,
                json!({
                    "path":fixture.writer,"name":"writer","is_workspace_default":true,
                    "workspace_dir":fixture.writer,"exists":true
                })
            )
        );
    }
}

#[tokio::test]
async fn project_list_marks_git_worktree_files_as_git_repositories() {
    let fixture = Fixture::new().await;
    let project = fixture.writer.join("coding_projects/worktree");
    fs::create_dir_all(&project).unwrap();
    fs::write(
        project.join(".git"),
        "gitdir: ../repository/.git/worktrees/worktree",
    )
    .unwrap();
    assert_eq!(
        fixture
            .json(
                "GET",
                "/api/workspace/project-directory/list",
                "writer",
                Value::Null
            )
            .await,
        (
            StatusCode::OK,
            json!([{
                "path":project,"name":"worktree","is_git":true,"is_active":false
            }])
        )
    );
}

#[tokio::test]
#[ignore = "requires qwenpaw Python environment; execute original handlers explicitly"]
async fn get_set_reset_and_list_match_original_python_for_both_agents() {
    let fixture = Fixture::new().await;
    let roots = json!({"default":fixture.default_root(),"writer":fixture.writer});
    let mut steps = Vec::new();
    for (actor, root) in [
        ("default", fixture.default_root()),
        ("writer", fixture.writer.clone()),
    ] {
        let project = root.join("coding_projects/shared");
        fs::create_dir_all(&project).unwrap();
        fs::write(project.join(".git"), "gitdir: fixture").unwrap();
        for (method, suffix, body) in [
            ("GET", "", Value::Null),
            ("GET", "/list", Value::Null),
            ("PUT", "", json!({"path":root})),
            ("GET", "", Value::Null),
            ("PUT", "", json!({"path":project})),
            ("GET", "/list", Value::Null),
            ("GET", "", Value::Null),
            ("PUT", "", json!({"path":null})),
            ("GET", "", Value::Null),
            ("GET", "/list", Value::Null),
        ] {
            steps.push(json!([method, suffix, actor, body]));
        }
    }
    let mut actual = Vec::new();
    for step in &steps {
        let (status, body) = fixture
            .json(
                step[0].as_str().unwrap(),
                &format!(
                    "/api/workspace/project-directory{}",
                    step[1].as_str().unwrap()
                ),
                step[2].as_str().unwrap(),
                step[3].clone(),
            )
            .await;
        actual.push(json!([status.as_u16(), body]));
    }
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        tokio::process::Command::new("python")
            .arg(root.join("scripts/project_directory_reference.py"))
            .arg(roots.to_string())
            .arg(json!(steps).to_string())
            .kill_on_drop(true)
            .output(),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let expected: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json!(actual), expected);
}
