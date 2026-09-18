//! Workspace bootstrap must not treat a selected project as an Agent base.

use super::*;
use pretty_assertions::assert_eq;

fn project(path: &std::path::Path) {
    std::fs::create_dir(path).unwrap();
    std::fs::write(path.join("keep.txt"), b"user project").unwrap();
}

fn unchanged_project(path: &std::path::Path) {
    let mut files = std::fs::read_dir(path)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    files.sort();
    assert_eq!(files, vec![String::from("keep.txt")], "{}", path.display());
    assert_eq!(
        std::fs::read(path.join("keep.txt")).unwrap(),
        b"user project"
    );
}

fn host(core: Core, root: &std::path::Path, base: &std::path::Path, desktop: bool) -> AppServer {
    if desktop {
        std::fs::write(root.join("index.html"), "fixture").unwrap();
        AppServer::new_desktop_with_stores_and_workspace(
            core,
            root,
            String::from("fixture-shutdown"),
            Arc::new(Credentials),
            &root.join("data"),
            base,
        )
        .unwrap()
    } else {
        AppServer::new_workspace_with_stores(core, Arc::new(Credentials), &root.join("data"), base)
            .unwrap()
    }
}

async fn first_bootstrap(desktop: bool) {
    let directory = tempfile::tempdir().unwrap();
    let base = directory.path().join("base");
    std::fs::create_dir(&base).unwrap();
    let selected = directory.path().join("project");
    project(&selected);
    let core = Core::persistent(offline_model(), &directory.path().join("core.sqlite")).unwrap();
    let preferred = core.write_preferred_workspace(&selected).unwrap();
    let server = host(core, directory.path(), &base, desktop);
    unchanged_project(&selected);
    let context = desktop_agents::context_for_agent(&server, "default")
        .await
        .unwrap();
    assert_eq!(context.workspace, base.canonicalize().unwrap());
    assert_eq!(context.project().unwrap(), selected.canonicalize().unwrap());
    assert_eq!(context.config["project_dir"], json!(preferred));
    assert!(base.join("AGENTS.md").is_file());
    assert_eq!(
        server.inner.core.read_preferred_workspace().unwrap(),
        Some(preferred)
    );
    assert_eq!(
        *server
            .inner
            .desktop_workspace
            .as_ref()
            .unwrap()
            .selected
            .read()
            .await,
        selected.canonicalize().unwrap()
    );
    server.inner.shutdown.cancel();
    let reopened = host(
        Core::persistent(offline_model(), &directory.path().join("core.sqlite")).unwrap(),
        directory.path(),
        &base,
        desktop,
    );
    assert_eq!(
        desktop_agents::context_for_agent(&reopened, "default")
            .await
            .unwrap(),
        context
    );
    unchanged_project(&selected);
    reopened.inner.shutdown.cancel();
}

async fn reopen_uses_registered_base(preferred_project: bool, desktop: bool) {
    let fixture = Fixture::new().await;
    let data = fixture.directory.path().join("data");
    let base = fixture.directory.path().join("workspace");
    let fallback = fixture.directory.path().join("caller-base");
    let selected = fixture.directory.path().join("project");
    project(&fallback);
    project(&selected);
    if preferred_project {
        fixture
            .server
            .inner
            .core
            .write_preferred_workspace(&selected)
            .unwrap();
    }
    let catalog = std::fs::read(data.join("agents/catalog.json")).unwrap();
    let marker = std::fs::read(base.join(desktop_agents::identity::MARKER_NAME)).unwrap();
    fixture.server.inner.shutdown.cancel();
    let core = Core::persistent(
        fixture.model.clone(),
        &fixture.directory.path().join("core.sqlite"),
    )
    .unwrap();
    let server = host(core, fixture.directory.path(), &fallback, desktop);
    unchanged_project(&fallback);
    unchanged_project(&selected);
    let context = desktop_agents::context_for_agent(&server, "default")
        .await
        .unwrap();
    assert_eq!(context.workspace, base.canonicalize().unwrap());
    assert_eq!(
        std::fs::read(data.join("agents/catalog.json")).unwrap(),
        catalog
    );
    assert_eq!(
        std::fs::read(base.join(desktop_agents::identity::MARKER_NAME)).unwrap(),
        marker
    );
    server.inner.shutdown.cancel();
}

#[tokio::test]
async fn workspace_host_directory_reopen_does_not_initialize_a_new_caller_base() {
    reopen_uses_registered_base(false, false).await;
}

#[tokio::test]
async fn workspace_host_directory_reopen_does_not_initialize_the_preferred_project() {
    reopen_uses_registered_base(true, false).await;
}

#[tokio::test]
async fn workspace_host_directory_first_bootstrap_keeps_preferred_project_separate() {
    first_bootstrap(false).await;
}

#[tokio::test]
async fn workspace_host_directory_desktop_first_bootstrap_keeps_preferred_project_separate() {
    first_bootstrap(true).await;
}

#[tokio::test]
async fn workspace_host_directory_desktop_reopen_keeps_a_new_caller_base_untouched() {
    reopen_uses_registered_base(false, true).await;
}

#[tokio::test]
async fn workspace_host_directory_desktop_reopen_keeps_the_preferred_project_untouched() {
    reopen_uses_registered_base(true, true).await;
}

fn files(path: &std::path::Path) -> std::collections::BTreeMap<std::ffi::OsString, Vec<u8>> {
    std::fs::read_dir(path)
        .unwrap()
        .map(|entry| {
            let entry = entry.unwrap();
            (entry.file_name(), std::fs::read(entry.path()).unwrap())
        })
        .collect()
}

async fn invalid_default_binding(desktop: bool) {
    for replacement in 0..3 {
        let mut fixture = Fixture::new().await;
        let thread = prepare(&fixture).await;
        actor(&fixture, "writer").await;
        let mut running = crate::desktop_agent_settings::default_running_config();
        running["approval_level"] = json!("OFF");
        fixture
            .request("PUT", "/api/agents/writer", json!({"running":running}))
            .await;
        let writer = chat(&fixture, "writer", "healthy-writer", "work-project").await;
        let root = fixture.directory.path();
        let base = root.join("workspace");
        let fallback = root.join("caller-base");
        let selected = root.join("project");
        project(&fallback);
        project(&selected);
        fixture
            .server
            .inner
            .core
            .write_preferred_workspace(&selected)
            .unwrap();
        fixture.server.inner.shutdown.cancel();
        std::fs::rename(&base, root.join("retained-base")).unwrap();
        if replacement > 0 {
            project(&base);
        }
        if replacement == 2 {
            std::fs::write(
                base.join(desktop_agents::identity::MARKER_NAME),
                serde_json::to_vec(&fixture.data_key("writer")).unwrap(),
            )
            .unwrap();
        }
        let original = (replacement > 0).then(|| files(&base));
        let catalog = std::fs::read(root.join("data/agents/catalog.json")).unwrap();
        let core = Core::persistent(fixture.model.clone(), &root.join("core.sqlite")).unwrap();
        fixture.server = host(core, root, &fallback, desktop);
        unchanged_project(&fallback);
        unchanged_project(&selected);
        if let Some(original) = original {
            assert_eq!(files(&base), original);
        } else {
            assert!(!base.exists());
        }
        assert_eq!(
            std::fs::read(root.join("data/agents/catalog.json")).unwrap(),
            catalog
        );
        let before = fixture
            .server
            .inner
            .core
            .backup_snapshot(1024 * 1024)
            .unwrap();
        let params = serde_json::from_str::<Value>(&request(&thread, "write fixture")).unwrap();
        let Err(error) = fixture
            .server
            .dispatch("turn/start", params["params"].clone())
            .await
        else {
            panic!("invalid default binding admitted a turn");
        };
        assert_eq!(
            (error.code, error.message.as_str()),
            (
                -32000,
                if replacement == 0 {
                    "Registered Agent Workspace is unavailable"
                } else {
                    "Agent 'default' Workspace binding has changed"
                }
            )
        );
        assert_eq!(
            fixture
                .server
                .inner
                .core
                .backup_snapshot(1024 * 1024)
                .unwrap(),
            before
        );
        let messages = exchange(&fixture, &writer, "write fixture").await;
        assert_eq!(
            messages.last().unwrap()["params"]["turn"]["status"],
            json!("completed")
        );
    }
}

#[tokio::test]
async fn workspace_host_directory_invalid_default_does_not_repair_or_disable_other_agents() {
    invalid_default_binding(false).await;
}

#[tokio::test]
async fn workspace_host_directory_desktop_invalid_default_does_not_repair_or_disable_other_agents()
{
    invalid_default_binding(true).await;
}

#[tokio::test]
async fn workspace_host_directory_corrupt_catalog_is_not_bootstrapped_as_a_new_installation() {
    for desktop in [false, true] {
        let fixture = Fixture::new().await;
        let root = fixture.directory.path();
        let fallback = root.join("caller-base");
        project(&fallback);
        let catalog = root.join("data/agents/catalog.json");
        std::fs::write(&catalog, b"invalid catalog").unwrap();
        let core = Core::persistent(fixture.model.clone(), &root.join("core.sqlite")).unwrap();
        core.write_environment_keys(&[String::from("FAIL_IF_READ")])
            .unwrap();
        let before = core.backup_snapshot(1024 * 1024).unwrap();
        let result = if desktop {
            AppServer::new_desktop_with_stores_and_workspace(
                core.clone(),
                root,
                String::from("fixture-shutdown"),
                Arc::new(NoCredentialReads),
                &root.join("data"),
                &fallback,
            )
        } else {
            AppServer::new_workspace_with_stores(
                core.clone(),
                Arc::new(NoCredentialReads),
                &root.join("data"),
                &fallback,
            )
        };
        let Err(error) = result else {
            panic!("corrupt catalog was accepted");
        };
        assert_eq!(error.to_string(), "Rust Agent catalog is invalid");
        assert_eq!(core.backup_snapshot(1024 * 1024).unwrap(), before);
        assert_eq!(std::fs::read(&catalog).unwrap(), b"invalid catalog");
        unchanged_project(&fallback);
    }
}
