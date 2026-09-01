use pretty_assertions::assert_eq;
use std::time::Duration;
use std::time::Instant;

use super::*;

#[tokio::test]
async fn reads_files_inside_the_workspace() {
    let directory = tempfile::tempdir().expect("temporary directory should be created");
    std::fs::write(directory.path().join("hello.txt"), "hello").expect("fixture should be written");
    let workspace = Workspace::open(directory.path()).expect("workspace should open");

    let output = workspace
        .execute(&ToolCall {
            id: String::from("call-1"),
            name: String::from("read_file"),
            arguments: String::from("{\"path\":\"hello.txt\"}"),
        })
        .await
        .expect("file should be read");

    assert_eq!(
        output,
        ToolOutput {
            content: String::from("hello"),
            is_error: false,
        }
    );
}

#[tokio::test]
async fn rejects_files_outside_the_workspace() {
    let parent = tempfile::tempdir().expect("temporary directory should be created");
    let workspace_path = parent.path().join("workspace");
    std::fs::create_dir(&workspace_path).expect("workspace should be created");
    std::fs::write(parent.path().join("secret.txt"), "secret").expect("fixture should be written");
    let workspace = Workspace::open(&workspace_path).expect("workspace should open");

    let error = workspace
        .execute(&ToolCall {
            id: String::from("call-1"),
            name: String::from("read_file"),
            arguments: String::from("{\"path\":\"../secret.txt\"}"),
        })
        .await
        .expect_err("outside path should be rejected");

    assert!(matches!(error, ToolError::OutsideWorkspace { .. }));
}

#[test]
fn resolves_only_workspace_file_references() {
    let parent = tempfile::tempdir().expect("temporary directory should be created");
    let workspace_path = parent.path().join("workspace");
    std::fs::create_dir_all(workspace_path.join("src")).expect("workspace should be created");
    std::fs::write(workspace_path.join("src/lib.rs"), "pub fn value() {}")
        .expect("fixture should be written");
    std::fs::write(parent.path().join("outside.rs"), "secret")
        .expect("outside fixture should be written");
    let workspace = Workspace::open(&workspace_path).expect("workspace should open");

    assert_eq!(
        workspace
            .resolve_file_reference("src/lib.rs")
            .expect("inside file should resolve"),
        "src/lib.rs"
    );
    assert!(matches!(
        workspace
            .resolve_file_reference("src")
            .expect_err("directory reference should fail"),
        ToolError::NotFile { .. }
    ));
    assert!(matches!(
        workspace
            .resolve_file_reference("../outside.rs")
            .expect_err("outside reference should fail"),
        ToolError::OutsideWorkspace { .. }
    ));
}

#[test]
fn shell_always_requires_approval() {
    assert_eq!(
        Workspace::approval_requirement(&ToolCall {
            id: String::from("call-1"),
            name: String::from("shell"),
            arguments: String::from("{\"command\":\"pwd\"}"),
        }),
        ApprovalRequirement::Required
    );
}

#[tokio::test]
async fn terminates_shell_commands_at_the_requested_timeout() {
    let directory = tempfile::tempdir().expect("temporary directory should be created");
    let workspace = Workspace::open(directory.path()).expect("workspace should open");
    let started_at = Instant::now();

    let error = workspace
        .execute(&ToolCall {
            id: String::from("call-1"),
            name: String::from("shell"),
            arguments: serde_json::json!({
                "command": LONG_RUNNING_COMMAND,
                "timeoutMs": 100
            })
            .to_string(),
        })
        .await
        .expect_err("long command should time out");

    assert!(matches!(
        error,
        ToolError::ShellTimedOut { timeout_ms: 100 }
    ));
    assert!(started_at.elapsed() < Duration::from_secs(3));
}

#[tokio::test]
async fn writes_files_inside_the_workspace() {
    let directory = tempfile::tempdir().expect("temporary directory should be created");
    let workspace = Workspace::open(directory.path()).expect("workspace should open");
    let call = ToolCall {
        id: String::from("call-1"),
        name: String::from("write_file"),
        arguments: String::from("{\"path\":\"result.txt\",\"content\":\"done\"}"),
    };

    assert_eq!(
        Workspace::approval_requirement(&call),
        ApprovalRequirement::Required
    );
    assert_eq!(
        workspace
            .execute(&call)
            .await
            .expect("file should be written"),
        ToolOutput {
            content: String::from("Wrote 4 bytes to result.txt"),
            is_error: false,
        }
    );
    assert_eq!(
        std::fs::read_to_string(directory.path().join("result.txt"))
            .expect("written file should be readable"),
        "done"
    );
}

#[tokio::test]
async fn rejects_writes_outside_the_workspace() {
    let parent = tempfile::tempdir().expect("temporary directory should be created");
    let workspace_path = parent.path().join("workspace");
    std::fs::create_dir(&workspace_path).expect("workspace should be created");
    let workspace = Workspace::open(&workspace_path).expect("workspace should open");

    let error = workspace
        .execute(&ToolCall {
            id: String::from("call-1"),
            name: String::from("write_file"),
            arguments: String::from("{\"path\":\"../outside.txt\",\"content\":\"no\"}"),
        })
        .await
        .expect_err("outside write should be rejected");

    assert!(matches!(error, ToolError::OutsideWorkspace { .. }));
    assert!(!parent.path().join("outside.txt").exists());
}

#[tokio::test]
async fn lists_source_files_and_skips_generated_directories() {
    let directory = tempfile::tempdir().expect("temporary directory should be created");
    std::fs::create_dir(directory.path().join("src")).expect("src should be created");
    std::fs::create_dir(directory.path().join("target")).expect("target should be created");
    std::fs::write(directory.path().join("README.md"), "readme").expect("readme should be written");
    std::fs::write(directory.path().join("src/main.rs"), "fn main() {}")
        .expect("source should be written");
    std::fs::write(directory.path().join("target/generated.rs"), "generated")
        .expect("generated file should be written");
    let workspace = Workspace::open(directory.path()).expect("workspace should open");

    let output = workspace
        .execute(&ToolCall {
            id: String::from("call-1"),
            name: String::from("list_files"),
            arguments: String::from("{}"),
        })
        .await
        .expect("files should be listed");

    assert_eq!(
        output,
        ToolOutput {
            content: String::from("README.md\nsrc/main.rs"),
            is_error: false,
        }
    );
}

#[tokio::test]
async fn searches_workspace_text_with_paths_and_line_numbers() {
    let directory = tempfile::tempdir().expect("temporary directory should be created");
    std::fs::create_dir(directory.path().join("src")).expect("src should be created");
    std::fs::write(
        directory.path().join("src/lib.rs"),
        "first line\nQwenPaw core\nlast line\n",
    )
    .expect("source should be written");
    let workspace = Workspace::open(directory.path()).expect("workspace should open");

    let output = workspace
        .execute(&ToolCall {
            id: String::from("call-1"),
            name: String::from("search_text"),
            arguments: String::from("{\"query\":\"QwenPaw\",\"path\":\"src\"}"),
        })
        .await
        .expect("text should be searched");

    assert_eq!(
        output,
        ToolOutput {
            content: String::from("src/lib.rs:2:QwenPaw core"),
            is_error: false,
        }
    );
}

#[tokio::test]
async fn rejects_discovery_outside_the_workspace() {
    let parent = tempfile::tempdir().expect("temporary directory should be created");
    let workspace_path = parent.path().join("workspace");
    std::fs::create_dir(&workspace_path).expect("workspace should be created");
    let workspace = Workspace::open(&workspace_path).expect("workspace should open");

    let error = workspace
        .execute(&ToolCall {
            id: String::from("call-1"),
            name: String::from("list_files"),
            arguments: String::from("{\"path\":\"..\"}"),
        })
        .await
        .expect_err("outside discovery should be rejected");

    assert!(matches!(error, ToolError::OutsideWorkspace { .. }));
}

#[tokio::test]
async fn replaces_exactly_one_text_block_after_approval() {
    let directory = tempfile::tempdir().expect("temporary directory should be created");
    std::fs::write(directory.path().join("config.txt"), "mode = old\n")
        .expect("fixture should be written");
    let workspace = Workspace::open(directory.path()).expect("workspace should open");
    let call = ToolCall {
        id: String::from("call-1"),
        name: String::from("replace_text"),
        arguments: String::from(
            "{\"path\":\"config.txt\",\"oldText\":\"mode = old\",\"newText\":\"mode = new\"}",
        ),
    };

    assert_eq!(
        Workspace::approval_requirement(&call),
        ApprovalRequirement::Required
    );
    assert_eq!(
        workspace
            .execute(&call)
            .await
            .expect("text should be replaced"),
        ToolOutput {
            content: String::from("Replaced one occurrence in config.txt"),
            is_error: false,
        }
    );
    assert_eq!(
        std::fs::read_to_string(directory.path().join("config.txt"))
            .expect("updated file should be readable"),
        "mode = new\n"
    );
}

#[tokio::test]
async fn refuses_ambiguous_text_replacements() {
    let directory = tempfile::tempdir().expect("temporary directory should be created");
    std::fs::write(directory.path().join("repeated.txt"), "same\nsame\n")
        .expect("fixture should be written");
    let workspace = Workspace::open(directory.path()).expect("workspace should open");

    let error = workspace
        .execute(&ToolCall {
            id: String::from("call-1"),
            name: String::from("replace_text"),
            arguments: String::from(
                "{\"path\":\"repeated.txt\",\"oldText\":\"same\",\"newText\":\"new\"}",
            ),
        })
        .await
        .expect_err("ambiguous replacement should be rejected");

    assert!(matches!(
        error,
        ToolError::ExpectedUniqueMatch { matches: 2 }
    ));
    assert_eq!(
        std::fs::read_to_string(directory.path().join("repeated.txt"))
            .expect("original file should be readable"),
        "same\nsame\n"
    );
}

#[cfg(windows)]
const LONG_RUNNING_COMMAND: &str = "ping -n 30 127.0.0.1 >NUL";

#[cfg(not(windows))]
const LONG_RUNNING_COMMAND: &str = "sleep 30";
