//! Checkpoint session discovery follows durable ownership, not project paths.

use super::*;
use crate::desktop_agents;
use crate::desktop_chats;
use pretty_assertions::assert_eq;
use sha2::Digest as _;

#[path = "desktop_checkpoint_workspace_tests.rs"]
mod workspace;

async fn actor(fixture: &Fixture, id: &str) {
    fixture
        .request("POST", "/api/agents", json!({"id":id,"name":id}))
        .await;
}

async fn chat(fixture: &Fixture, actor: &str, session: &str, project: &str) -> String {
    let root = fixture.directory.path().join(project);
    std::fs::create_dir_all(&root).unwrap();
    let root = root.canonicalize().unwrap();
    let _lifecycle = fixture
        .server
        .inner
        .desktop_agent_lifecycle_lock
        .lock()
        .await;
    let context = desktop_agents::context_for_agent(&fixture.server, actor)
        .await
        .unwrap();
    desktop_chats::create_console_thread(&fixture.server, &context, session, &root)
        .await
        .unwrap()
        .id
}

fn values(sessions: Vec<desktop_chats::CheckpointSessionInfo>) -> Value {
    json!(
        sessions
            .into_iter()
            .map(|session| json!({
                "thread_id":session.thread_id,"session_id":session.session_id,
                "user_id":session.user_id,"channel":session.channel,
                "title":session.title,"archived":session.archived
            }))
            .collect::<Vec<_>>()
    )
}

async fn sessions(fixture: &Fixture, actor: &str) -> Value {
    values(
        desktop_chats::checkpoint_sessions(&fixture.server, actor)
            .await
            .unwrap(),
    )
}

fn expected(id: &str, session: &str, archived: bool) -> Value {
    json!({"thread_id":id,"session_id":session,"user_id":"desktop", "channel":"console",
        "title":"New Chat","archived":archived})
}

#[tokio::test]
async fn checkpoint_sessions_include_all_owned_projects_and_archived_chats_after_reopen() {
    let mut fixture = Fixture::new().await;
    actor(&fixture, "writer").await;
    let default = chat(&fixture, "default", "same-session", "shared-project").await;
    let first = chat(&fixture, "writer", "a-session", "shared-project").await;
    let second = chat(&fixture, "writer", "b-session", "other-project").await;
    fixture
        .server
        .inner
        .core
        .archive_thread(&qwenpaw_protocol::ThreadArchiveParams {
            thread_id: second.clone(),
        })
        .await
        .unwrap();
    let writer = json!([
        expected(&first, "a-session", false),
        expected(&second, "b-session", true)
    ]);
    assert_eq!(sessions(&fixture, "writer").await, writer);
    assert_eq!(
        sessions(&fixture, "default").await,
        json!([expected(&default, "same-session", false)])
    );
    let graph = fixture
        .request("GET", "/api/workspace/checkpoints/graph", Value::Null)
        .await;
    let session_key = format!(
        "session-{:x}",
        sha2::Sha256::digest(br#"["console","desktop","same-session"]"#)
    );
    assert_eq!(
        graph,
        json!({"summary":{"total":0,"auto":0,"snapshots":0,"safety":0,"heads":0},
        "nodes":[],"sessions":[{"session_key":session_key,"session_id":"same-session",
            "user_id":"desktop","channel":"console","title":"New Chat","archived":false}],"truncated":false})
    );
    fixture.reopen().await;
    assert_eq!(sessions(&fixture, "writer").await, writer);
}

#[tokio::test]
async fn checkpoint_sessions_keep_retained_workspace_without_adopting_reused_agent_id() {
    let mut fixture = Fixture::new().await;
    actor(&fixture, "writer").await;
    let id = chat(&fixture, "writer", "same-session", "shared-project").await;
    let admitted = desktop_agents::context_for_agent(&fixture.server, "writer")
        .await
        .unwrap();
    let root = fixture.directory.path().join("data/workspaces/writer");
    fixture
        .request("DELETE", "/api/agents/writer", Value::Null)
        .await;
    fixture
        .request(
            "POST",
            "/api/agents",
            json!({"id":"editor","name":"Editor","workspace_dir":root}),
        )
        .await;
    fixture
        .request(
            "POST",
            "/api/agents",
            json!({"id":"writer","name":"New writer",
        "workspace_dir":fixture.directory.path().join("new-writer")}),
        )
        .await;
    let new_id = chat(&fixture, "writer", "same-session", "shared-project").await;
    fixture.reopen().await;
    assert_eq!(
        sessions(&fixture, "editor").await,
        json!([expected(&id, "same-session", false)])
    );
    assert_eq!(
        sessions(&fixture, "writer").await,
        json!([expected(&new_id, "same-session", false)])
    );
    assert_eq!(
        values(
            desktop_chats::bound_checkpoint_sessions(&fixture.server, &admitted)
                .await
                .unwrap()
        ),
        json!([expected(&id, "same-session", false)])
    );
}

#[tokio::test]
async fn checkpoint_sessions_never_assign_sdk_threads_to_the_agent_owning_the_project() {
    let fixture = Fixture::new().await;
    actor(&fixture, "writer").await;
    let root = fixture
        .directory
        .path()
        .join("data/workspaces/writer")
        .canonicalize()
        .unwrap();
    let id = fixture
        .server
        .inner
        .core
        .start_thread(qwenpaw_protocol::ThreadStartParams {
            workspace_root: Some(root.to_string_lossy().into_owned()),
            model: None,
        })
        .await
        .unwrap()
        .thread
        .id;
    assert_eq!(sessions(&fixture, "writer").await, json!([]));
    assert_eq!(
        sessions(&fixture, "default").await,
        json!([expected(&id, &id, false)])
    );
    assert_eq!(sessions(&fixture, "writer").await, json!([]));
}

#[tokio::test]
async fn checkpoint_sessions_restore_base_files_without_changing_the_external_project() {
    let mut fixture = Fixture::new().await;
    let id = chat(&fixture, "default", "external-session", "external-project").await;
    let base = fixture.directory.path().join("workspace");
    let project = fixture.directory.path().join("external-project");
    std::fs::write(base.join("notes.txt"), "base before").unwrap();
    std::fs::write(project.join("notes.txt"), "project before").unwrap();
    let thread = fixture
        .server
        .inner
        .core
        .read_thread(&id)
        .await
        .unwrap()
        .thread;
    let snapshot = fixture.request("POST", "/api/workspace/checkpoints/snapshot",
        json!({"session_id":"external-session","user_id":"desktop","channel":"console","name":"external"})).await;
    std::fs::write(base.join("notes.txt"), "base edited").unwrap();
    std::fs::write(project.join("notes.txt"), "project edited").unwrap();
    fixture.reopen().await;
    let request = json!({"commit":snapshot["commit"],"session_id":"external-session",
        "user_id":"desktop","channel":"console","include_memory":false,
        "include_files":true,"files":["notes.txt"]});
    let mut expected = json!({"target":snapshot["commit"],"commit":snapshot["commit"],
        "restored_paths":["sessions/external-session.json","notes.txt"],"deleted_paths":[],
        "file_paths":["notes.txt"],"pre_restore_ref":null,"dry_run":true,
        "include_memory":false,"include_files":true});
    assert_eq!(
        fixture
            .request(
                "POST",
                "/api/workspace/checkpoints/restore/preview",
                request.clone()
            )
            .await,
        expected
    );
    assert_eq!(
        std::fs::read_to_string(base.join("notes.txt")).unwrap(),
        "base edited"
    );
    let restored = fixture
        .request("POST", "/api/workspace/checkpoints/restore", request)
        .await;
    assert!(
        restored["pre_restore_ref"]
            .as_str()
            .unwrap()
            .starts_with("refs/pre-restore/")
    );
    expected["pre_restore_ref"] = restored["pre_restore_ref"].clone();
    expected["dry_run"] = json!(false);
    assert_eq!(restored, expected);
    assert_eq!(
        std::fs::read_to_string(base.join("notes.txt")).unwrap(),
        "base before"
    );
    assert_eq!(
        std::fs::read_to_string(project.join("notes.txt")).unwrap(),
        "project edited"
    );
    assert_eq!(
        fixture
            .server
            .inner
            .core
            .read_thread(&id)
            .await
            .unwrap()
            .thread
            .workspace_root,
        thread.workspace_root
    );
}
