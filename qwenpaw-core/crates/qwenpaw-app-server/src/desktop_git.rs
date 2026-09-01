//! Workspace-scoped Git compatibility endpoints for the unchanged Console.

use std::path::Component;
use std::path::Path;
use std::path::PathBuf;
use std::process::ExitStatus;
use std::time::Duration;

use axum::Json;
use axum::Router;
use axum::extract::Query;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::http::StatusCode;
use axum::routing::get;
use axum::routing::post;
use serde::Deserialize;
use serde_json::Value;
use serde_json::json;
use tokio::io::AsyncRead;
use tokio::io::AsyncReadExt;
use tokio::process::Command;

use super::AppServer;
use super::desktop_files::resolve_workspace_root;

const GIT_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_GIT_OUTPUT_BYTES: usize = 4 * 1024 * 1024;
const MAX_PATH_BYTES: usize = 4_096;
const MAX_PATHS_PER_REQUEST: usize = 1_024;
const MAX_BRANCH_BYTES: usize = 255;
const MAX_COMMIT_MESSAGE_BYTES: usize = 16_384;
const MAX_LOG_LIMIT: u16 = 200;
const GIT_IDENTITY_ARGS: [&str; 4] = [
    "-c",
    "user.email=qwenpaw@localhost",
    "-c",
    "user.name=QwenPaw",
];
const DEFAULT_GITIGNORE: &str = "# Created by QwenPaw Coding Mode\n\
__pycache__/\n\
*.py[cod]\n\
.venv/\n\
node_modules/\n\
dist/\n\
build/\n\
.DS_Store\n\
.env\n\
.qwenpaw/\n";

type ApiError = (StatusCode, Json<Value>);

pub(super) fn router() -> Router<AppServer> {
    Router::new()
        .route("/api/workspace/git/status", get(git_status))
        .route("/api/workspace/git/branches", get(list_branches))
        .route("/api/workspace/git/checkout", post(checkout_branch))
        .route("/api/workspace/git/diff", get(get_diff))
        .route("/api/workspace/git/stage", post(stage_files))
        .route("/api/workspace/git/unstage", post(unstage_files))
        .route("/api/workspace/git/commit", post(commit_changes))
        .route("/api/workspace/git/log", get(get_log))
        .route("/api/workspace/git/discard", post(discard_changes))
        .route("/api/workspace/git/commit-diff", get(get_commit_diff))
        .route("/api/workspace/git/revert", post(revert_commit))
}

#[derive(Debug)]
struct GitOutput {
    status: ExitStatus,
    stdout: String,
    stderr: String,
}

impl GitOutput {
    fn success(&self) -> bool {
        self.status.success()
    }

    fn error_detail(&self) -> String {
        let stderr = self.stderr.trim();
        if stderr.is_empty() {
            self.stdout.trim().to_owned()
        } else {
            stderr.to_owned()
        }
    }
}

async fn run_git<I, S>(cwd: &Path, arguments: I) -> Result<GitOutput, ApiError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    let mut command = Command::new("git");
    command
        .args(arguments)
        .current_dir(cwd)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_LITERAL_PATHSPECS", "1")
        .env("LC_ALL", "C")
        .kill_on_drop(true)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let mut child = command
        .spawn()
        .map_err(|_| internal_error("Git executable could not be started"))?;
    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| internal_error("Git stdout could not be captured"))?;
    let mut stderr = child
        .stderr
        .take()
        .ok_or_else(|| internal_error("Git stderr could not be captured"))?;
    let completed = tokio::time::timeout(GIT_TIMEOUT, async {
        tokio::try_join!(
            child.wait(),
            read_capped(&mut stdout),
            read_capped(&mut stderr)
        )
    })
    .await;
    let (status, (stdout, stdout_overflow), (stderr, stderr_overflow)) = match completed {
        Ok(Ok(output)) => output,
        Ok(Err(_)) => return Err(internal_error("Git command output could not be read")),
        Err(_) => {
            let _ = child.kill().await;
            let _ = child.wait().await;
            return Err((
                StatusCode::GATEWAY_TIMEOUT,
                Json(json!({"detail": "Git command timed out"})),
            ));
        }
    };
    if stdout_overflow || stderr_overflow {
        return Err((
            StatusCode::PAYLOAD_TOO_LARGE,
            Json(json!({"detail": "Git command output exceeded 4 MiB"})),
        ));
    }
    Ok(GitOutput {
        status,
        stdout: String::from_utf8_lossy(&stdout).into_owned(),
        stderr: String::from_utf8_lossy(&stderr).into_owned(),
    })
}

async fn read_capped(reader: &mut (impl AsyncRead + Unpin)) -> std::io::Result<(Vec<u8>, bool)> {
    let mut output = Vec::new();
    let mut overflow = false;
    let mut buffer = vec![0_u8; 16 * 1024];
    loop {
        let count = reader.read(&mut buffer).await?;
        if count == 0 {
            return Ok((output, overflow));
        }
        let available = MAX_GIT_OUTPUT_BYTES.saturating_sub(output.len());
        let retained = available.min(count);
        output.extend_from_slice(&buffer[..retained]);
        overflow |= retained != count;
    }
}

async fn git_status(
    State(server): State<AppServer>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    let _guard = server.inner.desktop_git_lock.lock().await;
    let workspace = resolve_workspace_root(&server, &headers, Some("project")).await?;
    let branch = ensure_workspace_repository(&workspace).await?;
    let (behind, ahead) = ahead_behind(&workspace).await;
    let output = run_git(
        &workspace,
        [
            "-c",
            "core.quotepath=false",
            "status",
            "--porcelain=v1",
            "-z",
            "--untracked-files=normal",
        ],
    )
    .await?;
    require_success(&output)?;
    Ok(Json(json!({
        "branch": branch,
        "changes": parse_status(&output.stdout),
        "ahead": ahead,
        "behind": behind
    })))
}

async fn ensure_workspace_repository(workspace: &Path) -> Result<String, ApiError> {
    let top_level = run_git(workspace, ["rev-parse", "--show-toplevel"]).await?;
    if top_level.success()
        && PathBuf::from(top_level.stdout.trim())
            .canonicalize()
            .is_ok_and(|path| path == workspace)
    {
        return current_branch(workspace).await;
    }

    let initialized = run_git(workspace, ["init"]).await?;
    require_success(&initialized)?;
    let gitignore = workspace.join(".gitignore");
    let created_gitignore = tokio::fs::metadata(&gitignore).await.is_err();
    if created_gitignore {
        tokio::fs::write(&gitignore, DEFAULT_GITIGNORE)
            .await
            .map_err(|_| internal_error("Default .gitignore could not be written"))?;
        let added = run_git(workspace, ["add", "--", ".gitignore"]).await?;
        require_success(&added)?;
    }
    let mut arguments = GIT_IDENTITY_ARGS.to_vec();
    arguments.extend(["commit", "--quiet", "--allow-empty", "-m", "Initial commit"]);
    let committed = run_git(workspace, arguments).await?;
    require_success(&committed)?;
    current_branch(workspace).await
}

async fn current_branch(workspace: &Path) -> Result<String, ApiError> {
    let output = run_git(workspace, ["rev-parse", "--abbrev-ref", "HEAD"]).await?;
    require_success(&output)?;
    Ok(output.stdout.trim().to_owned())
}

async fn ahead_behind(workspace: &Path) -> (u64, u64) {
    let Ok(output) = run_git(
        workspace,
        ["rev-list", "--left-right", "--count", "@{u}...HEAD"],
    )
    .await
    else {
        return (0, 0);
    };
    if !output.success() {
        return (0, 0);
    }
    let counts = output
        .stdout
        .split_whitespace()
        .filter_map(|value| value.parse::<u64>().ok())
        .collect::<Vec<_>>();
    match counts.as_slice() {
        [behind, ahead] => (*behind, *ahead),
        _ => (0, 0),
    }
}

fn parse_status(output: &str) -> Vec<Value> {
    let mut entries = output.split('\0').filter(|entry| !entry.is_empty());
    let mut changes = Vec::new();
    while let Some(entry) = entries.next() {
        if entry.len() < 4 {
            continue;
        }
        let bytes = entry.as_bytes();
        let staged = char::from(bytes[0]);
        let unstaged = char::from(bytes[1]);
        let path = &entry[3..];
        if matches!(staged, 'R' | 'C') || matches!(unstaged, 'R' | 'C') {
            let _ = entries.next();
        }
        if staged != ' ' && staged != '?' {
            changes.push(json!({
                "path": path.trim_end_matches('/'),
                "status": staged.to_string(),
                "staged": true
            }));
        }
        if unstaged == '?' {
            changes.push(json!({
                "path": path.trim_end_matches('/'),
                "status": "?",
                "staged": false
            }));
        } else if unstaged != ' ' {
            changes.push(json!({
                "path": path.trim_end_matches('/'),
                "status": unstaged.to_string(),
                "staged": false
            }));
        }
    }
    changes
}

async fn list_branches(
    State(server): State<AppServer>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    let _guard = server.inner.desktop_git_lock.lock().await;
    let workspace = resolve_workspace_root(&server, &headers, Some("project")).await?;
    let output = run_git(
        &workspace,
        ["branch", "-a", "--format=%(refname:short)|%(HEAD)"],
    )
    .await?;
    require_repository(&output)?;
    let branches = output
        .stdout
        .lines()
        .filter_map(|line| {
            let (name, head) = line.trim().split_once('|')?;
            Some(json!({
                "name": name,
                "current": head.trim() == "*",
                "remote": name.starts_with("origin/") || name.starts_with("remotes/")
            }))
        })
        .collect::<Vec<_>>();
    Ok(Json(Value::Array(branches)))
}

#[derive(Debug, Deserialize)]
struct CheckoutRequest {
    branch: String,
    #[serde(default)]
    create: bool,
}

async fn checkout_branch(
    State(server): State<AppServer>,
    headers: HeaderMap,
    Json(request): Json<CheckoutRequest>,
) -> Result<Json<Value>, ApiError> {
    validate_branch_name(&request.branch)?;
    let _guard = server.inner.desktop_git_lock.lock().await;
    let workspace = resolve_workspace_root(&server, &headers, Some("project")).await?;
    let checked = run_git(
        &workspace,
        ["check-ref-format", "--branch", &request.branch],
    )
    .await?;
    if !checked.success() {
        return Err(bad_request("Git branch name is invalid"));
    }
    let output = if request.create {
        run_git(&workspace, ["checkout", "-b", &request.branch]).await?
    } else {
        run_git(&workspace, ["checkout", &request.branch]).await?
    };
    require_success(&output)?;
    Ok(Json(json!({"branch": request.branch})))
}

#[derive(Debug, Default, Deserialize)]
struct DiffQuery {
    path: Option<String>,
    #[serde(default)]
    staged: bool,
    #[serde(default)]
    untracked: bool,
}

async fn get_diff(
    State(server): State<AppServer>,
    headers: HeaderMap,
    Query(query): Query<DiffQuery>,
) -> Result<Json<Value>, ApiError> {
    let path = query
        .path
        .as_deref()
        .map(validate_relative_path)
        .transpose()?;
    let _guard = server.inner.desktop_git_lock.lock().await;
    let workspace = resolve_workspace_root(&server, &headers, Some("project")).await?;
    let output = if query.untracked {
        let path = path.ok_or_else(|| bad_request("Untracked diff requires a path"))?;
        run_git(
            &workspace,
            ["diff", "--no-index", "--", null_device(), path],
        )
        .await?
    } else {
        let mut arguments = vec!["diff"];
        if query.staged {
            arguments.push("--staged");
        }
        if let Some(path) = path {
            arguments.extend(["--", path]);
        }
        run_git(&workspace, arguments).await?
    };
    if query.untracked {
        if !matches!(output.status.code(), Some(0 | 1)) {
            return Err(command_error(&output));
        }
    } else {
        require_success(&output)?;
    }
    Ok(Json(json!({"diff": output.stdout})))
}

#[derive(Debug, Default, Deserialize)]
struct PathsRequest {
    #[serde(default)]
    paths: Vec<String>,
}

async fn stage_files(
    State(server): State<AppServer>,
    headers: HeaderMap,
    Json(request): Json<PathsRequest>,
) -> Result<Json<Value>, ApiError> {
    let paths = validated_paths(request.paths)?;
    let _guard = server.inner.desktop_git_lock.lock().await;
    let workspace = resolve_workspace_root(&server, &headers, Some("project")).await?;
    let mut arguments = vec![String::from("add"), String::from("--")];
    arguments.extend(paths.iter().cloned());
    let output = run_git(&workspace, arguments).await?;
    require_success(&output)?;
    Ok(Json(json!({"staged": paths})))
}

async fn unstage_files(
    State(server): State<AppServer>,
    headers: HeaderMap,
    Json(request): Json<PathsRequest>,
) -> Result<Json<Value>, ApiError> {
    let paths = validated_paths(request.paths)?;
    let _guard = server.inner.desktop_git_lock.lock().await;
    let workspace = resolve_workspace_root(&server, &headers, Some("project")).await?;
    let mut arguments = vec![
        String::from("restore"),
        String::from("--staged"),
        String::from("--"),
    ];
    arguments.extend(paths.iter().cloned());
    let output = run_git(&workspace, arguments).await?;
    require_success(&output)?;
    Ok(Json(json!({"unstaged": paths})))
}

#[derive(Debug, Deserialize)]
struct CommitRequest {
    message: String,
}

async fn commit_changes(
    State(server): State<AppServer>,
    headers: HeaderMap,
    Json(request): Json<CommitRequest>,
) -> Result<Json<Value>, ApiError> {
    let message = validate_commit_message(&request.message)?;
    let _guard = server.inner.desktop_git_lock.lock().await;
    let workspace = resolve_workspace_root(&server, &headers, Some("project")).await?;
    let mut arguments = GIT_IDENTITY_ARGS.map(String::from).to_vec();
    arguments.extend([
        String::from("commit"),
        String::from("--quiet"),
        String::from("-m"),
        message.to_owned(),
    ]);
    let output = run_git(&workspace, arguments).await?;
    require_success(&output)?;
    Ok(Json(json!({
        "committed": true,
        "output": output.stdout.trim()
    })))
}

#[derive(Debug, Deserialize)]
struct LogQuery {
    #[serde(default = "default_log_limit")]
    limit: u16,
}

const fn default_log_limit() -> u16 {
    20
}

async fn get_log(
    State(server): State<AppServer>,
    headers: HeaderMap,
    Query(query): Query<LogQuery>,
) -> Result<Json<Value>, ApiError> {
    if query.limit == 0 || query.limit > MAX_LOG_LIMIT {
        return Err(bad_request("Git log limit must be between 1 and 200"));
    }
    let _guard = server.inner.desktop_git_lock.lock().await;
    let workspace = resolve_workspace_root(&server, &headers, Some("project")).await?;
    let limit = format!("-{}", query.limit);
    let output = run_git(
        &workspace,
        [
            "log",
            limit.as_str(),
            "--format=%H%x00%an%x00%ad%x00%s%x00",
            "--date=short",
        ],
    )
    .await?;
    require_repository(&output)?;
    let fields = output
        .stdout
        .split('\0')
        .filter(|field| !field.is_empty())
        .collect::<Vec<_>>();
    let mut commits = Vec::new();
    let mut index = 0;
    while index + 3 < fields.len() {
        commits.push(json!({
            "hash": fields[index].chars().take(8).collect::<String>(),
            "author": fields[index + 1],
            "date": fields[index + 2],
            "message": fields[index + 3]
        }));
        index += 4;
    }
    Ok(Json(Value::Array(commits)))
}

async fn discard_changes(
    State(server): State<AppServer>,
    headers: HeaderMap,
    Json(request): Json<PathsRequest>,
) -> Result<Json<Value>, ApiError> {
    let paths = validated_paths(request.paths)?;
    let _guard = server.inner.desktop_git_lock.lock().await;
    let workspace = resolve_workspace_root(&server, &headers, Some("project")).await?;
    let mut restore_arguments = vec![String::from("restore"), String::from("--")];
    restore_arguments.extend(paths.iter().cloned());
    let restored = run_git(&workspace, restore_arguments).await?;
    let mut clean_arguments = vec![
        String::from("clean"),
        String::from("-fdq"),
        String::from("--"),
    ];
    clean_arguments.extend(paths.iter().cloned());
    let cleaned = run_git(&workspace, clean_arguments).await?;
    if !restored.success() && !cleaned.success() {
        return Err(command_error(&restored));
    }
    Ok(Json(json!({"discarded": paths})))
}

#[derive(Debug, Deserialize)]
struct CommitDiffQuery {
    commit_hash: String,
}

async fn get_commit_diff(
    State(server): State<AppServer>,
    headers: HeaderMap,
    Query(query): Query<CommitDiffQuery>,
) -> Result<Json<Value>, ApiError> {
    validate_commit_hash(&query.commit_hash)?;
    let _guard = server.inner.desktop_git_lock.lock().await;
    let workspace = resolve_workspace_root(&server, &headers, Some("project")).await?;
    let output = run_git(
        &workspace,
        ["show", "--stat", "--patch", &query.commit_hash],
    )
    .await?;
    require_success(&output)?;
    Ok(Json(json!({
        "diff": output.stdout,
        "hash": query.commit_hash
    })))
}

#[derive(Debug, Deserialize)]
struct RevertRequest {
    commit_hash: String,
}

async fn revert_commit(
    State(server): State<AppServer>,
    headers: HeaderMap,
    Json(request): Json<RevertRequest>,
) -> Result<Json<Value>, ApiError> {
    validate_commit_hash(&request.commit_hash)?;
    let _guard = server.inner.desktop_git_lock.lock().await;
    let workspace = resolve_workspace_root(&server, &headers, Some("project")).await?;
    let mut arguments = GIT_IDENTITY_ARGS.map(String::from).to_vec();
    arguments.extend([
        String::from("revert"),
        String::from("--quiet"),
        String::from("--no-edit"),
        request.commit_hash.clone(),
    ]);
    let output = run_git(&workspace, arguments).await?;
    require_success(&output)?;
    Ok(Json(json!({
        "reverted": request.commit_hash,
        "output": output.stdout.trim()
    })))
}

fn validated_paths(paths: Vec<String>) -> Result<Vec<String>, ApiError> {
    if paths.len() > MAX_PATHS_PER_REQUEST {
        return Err(bad_request("Too many Git paths"));
    }
    if paths.is_empty() {
        return Ok(vec![String::from(".")]);
    }
    for path in &paths {
        validate_relative_path(path)?;
    }
    Ok(paths)
}

fn validate_relative_path(path: &str) -> Result<&str, ApiError> {
    if path.is_empty()
        || path.len() > MAX_PATH_BYTES
        || path.chars().any(char::is_control)
        || Path::new(path).is_absolute()
        || path.starts_with(['/', '\\'])
        || path.as_bytes().get(1).is_some_and(|value| *value == b':')
        || path
            .split(['/', '\\'])
            .any(|segment| matches!(segment, "." | ".."))
        || Path::new(path)
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(bad_request("Git path must be relative without traversal"));
    }
    Ok(path)
}

fn validate_branch_name(branch: &str) -> Result<(), ApiError> {
    if branch.is_empty()
        || branch.len() > MAX_BRANCH_BYTES
        || branch.starts_with('-')
        || branch.chars().any(char::is_control)
    {
        return Err(bad_request("Git branch name is invalid"));
    }
    Ok(())
}

fn validate_commit_message(message: &str) -> Result<&str, ApiError> {
    let message = message.trim();
    if message.is_empty() || message.len() > MAX_COMMIT_MESSAGE_BYTES || message.contains('\0') {
        return Err(bad_request("Git commit message is invalid"));
    }
    Ok(message)
}

fn validate_commit_hash(commit_hash: &str) -> Result<(), ApiError> {
    if !(4..=64).contains(&commit_hash.len())
        || !commit_hash.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(bad_request("Git commit hash is invalid"));
    }
    Ok(())
}

fn require_repository(output: &GitOutput) -> Result<(), ApiError> {
    if !output.success()
        && output
            .stderr
            .to_ascii_lowercase()
            .contains("not a git repository")
    {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({"detail": "Not a git repository"})),
        ));
    }
    require_success(output)
}

fn require_success(output: &GitOutput) -> Result<(), ApiError> {
    if output.success() {
        Ok(())
    } else {
        Err(command_error(output))
    }
}

fn command_error(output: &GitOutput) -> ApiError {
    let detail = output.error_detail();
    bad_request(if detail.is_empty() {
        "Git command failed"
    } else {
        &detail
    })
}

fn bad_request(message: &str) -> ApiError {
    (StatusCode::BAD_REQUEST, Json(json!({"detail": message})))
}

fn internal_error(message: &str) -> ApiError {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({"detail": message})),
    )
}

#[cfg(windows)]
const fn null_device() -> &'static str {
    "NUL"
}

#[cfg(not(windows))]
const fn null_device() -> &'static str {
    "/dev/null"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_git_inputs_without_accepting_option_or_path_injection() {
        assert_eq!(
            validate_relative_path("src/main.rs").expect("safe path should validate"),
            "src/main.rs"
        );
        for path in ["", ".", "../secret", "safe/../../secret", "/tmp/file"] {
            assert!(validate_relative_path(path).is_err(), "{path}");
        }
        assert!(validate_branch_name("feature/rust-core").is_ok());
        assert!(validate_branch_name("--upload-pack=evil").is_err());
        assert!(validate_commit_hash("0123abcd").is_ok());
        assert!(validate_commit_hash("HEAD").is_err());
    }

    #[test]
    fn parses_staged_unstaged_and_untracked_status_entries() {
        assert_eq!(
            parse_status("M  staged.txt\0 M modified.txt\0?? untracked dir/\0"),
            vec![
                json!({"path": "staged.txt", "status": "M", "staged": true}),
                json!({"path": "modified.txt", "status": "M", "staged": false}),
                json!({"path": "untracked dir", "status": "?", "staged": false})
            ]
        );
    }

    #[tokio::test]
    async fn initializes_an_inherited_workspace_as_its_own_repository() {
        let parent = tempfile::tempdir().expect("temporary parent should be created");
        let initialized = run_git(parent.path(), ["init"])
            .await
            .expect("parent Git repository should initialize");
        assert!(initialized.success());
        let workspace = parent.path().join("workspace");
        std::fs::create_dir(&workspace).expect("nested Workspace should be created");

        let branch = ensure_workspace_repository(&workspace)
            .await
            .expect("nested Workspace should initialize independently");

        assert!(!branch.is_empty());
        assert!(workspace.join(".git").is_dir());
        let top_level = run_git(&workspace, ["rev-parse", "--show-toplevel"])
            .await
            .expect("nested top-level should resolve");
        assert_eq!(
            PathBuf::from(top_level.stdout.trim())
                .canonicalize()
                .expect("nested top-level should canonicalize"),
            workspace
                .canonicalize()
                .expect("nested Workspace should canonicalize")
        );
    }
}
