use std::collections::BTreeMap;
use std::path::Path;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use serde::Deserialize;
use serde_json::Value;
use tokio::process::Command;

mod definitions;
mod discovery;

const MAX_OUTPUT_BYTES: usize = 1_048_576;
const DEFAULT_SHELL_TIMEOUT_MS: u64 = 120_000;
const MIN_SHELL_TIMEOUT_MS: u64 = 100;
const MAX_SHELL_TIMEOUT_MS: u64 = 600_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalRequirement {
    None,
    Required,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolOutput {
    pub content: String,
    pub is_error: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuiltinToolMetadata {
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone)]
pub struct Workspace {
    root: PathBuf,
    environment: BTreeMap<String, String>,
}

impl Workspace {
    /// Opens a workspace rooted at an existing directory.
    ///
    /// # Errors
    ///
    /// Returns an error when the root is missing, cannot be canonicalized, or
    /// is not a directory.
    pub fn open(root: &Path) -> Result<Self, ToolError> {
        let root = root.canonicalize().map_err(ToolError::WorkspaceRoot)?;
        if !root.is_dir() {
            return Err(ToolError::WorkspaceNotDirectory { path: root });
        }
        Ok(Self {
            root,
            environment: BTreeMap::new(),
        })
    }

    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Adds environment variables inherited by Workspace child processes.
    #[must_use]
    pub fn with_environment(mut self, environment: BTreeMap<String, String>) -> Self {
        self.environment = environment;
        self
    }

    /// Resolves an existing file and returns its portable Workspace-relative path.
    ///
    /// # Errors
    ///
    /// Returns an error when the path is missing, is not a file, or resolves
    /// outside this Workspace.
    pub fn resolve_file_reference(&self, requested: &str) -> Result<String, ToolError> {
        let path = self.resolve_existing_path(requested)?;
        if !path.is_file() {
            return Err(ToolError::NotFile { path });
        }
        Ok(relative_display(&self.root, &path))
    }

    #[must_use]
    pub fn approval_requirement(call: &ToolCall) -> ApprovalRequirement {
        match call.name.as_str() {
            "replace_text" | "shell" | "write_file" => ApprovalRequirement::Required,
            _ => ApprovalRequirement::None,
        }
    }

    /// Executes one built-in tool inside this workspace.
    ///
    /// # Errors
    ///
    /// Returns an error for unknown tools, malformed arguments, filesystem
    /// failures, or paths outside the workspace.
    pub async fn execute(&self, call: &ToolCall) -> Result<ToolOutput, ToolError> {
        self.execute_with_shell_config(call, DEFAULT_SHELL_TIMEOUT_MS, None)
            .await
    }

    /// Executes one built-in tool with the current Agent shell defaults.
    ///
    /// A timeout explicitly supplied by a tool call still takes precedence.
    ///
    /// # Errors
    ///
    /// Returns an error for unknown tools, malformed arguments, invalid shell
    /// settings, filesystem failures, or paths outside the Workspace.
    pub async fn execute_with_shell_config(
        &self,
        call: &ToolCall,
        default_shell_timeout_ms: u64,
        shell_executable: Option<&str>,
    ) -> Result<ToolOutput, ToolError> {
        match call.name.as_str() {
            "list_files" => discovery::list_files(&self.root, &call.arguments),
            "read_file" => self.read_file(&call.arguments).await,
            "replace_text" => self.replace_text(&call.arguments).await,
            "search_text" => discovery::search_text(&self.root, &call.arguments),
            "write_file" => self.write_file(&call.arguments).await,
            "shell" => {
                self.shell(&call.arguments, default_shell_timeout_ms, shell_executable)
                    .await
            }
            _ => Err(ToolError::UnknownTool(call.name.clone())),
        }
    }

    async fn read_file(&self, arguments: &str) -> Result<ToolOutput, ToolError> {
        let arguments: ReadFileArguments = serde_json::from_str(arguments)?;
        let path = self.resolve_existing_path(&arguments.path)?;
        if !path.is_file() {
            return Err(ToolError::NotFile { path });
        }
        let bytes = tokio::fs::read(&path).await?;
        let content = truncate(String::from_utf8_lossy(&bytes).into_owned());
        Ok(ToolOutput {
            content,
            is_error: false,
        })
    }

    async fn shell(
        &self,
        arguments: &str,
        default_timeout_ms: u64,
        shell_executable: Option<&str>,
    ) -> Result<ToolOutput, ToolError> {
        let arguments: ShellArguments = serde_json::from_str(arguments)?;
        if arguments.command.trim().is_empty() {
            return Err(ToolError::EmptyCommand);
        }
        let timeout_ms = arguments
            .timeout_ms
            .unwrap_or(default_timeout_ms)
            .clamp(MIN_SHELL_TIMEOUT_MS, MAX_SHELL_TIMEOUT_MS);
        let mut command = configured_shell(&arguments.command, shell_executable)?;
        command
            .kill_on_drop(true)
            .current_dir(&self.root)
            .envs(&self.environment)
            .stdin(Stdio::null());
        let output = tokio::time::timeout(Duration::from_millis(timeout_ms), command.output())
            .await
            .map_err(|_| ToolError::ShellTimedOut { timeout_ms })??;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let content = truncate(format!(
            "exit code: {}\nstdout:\n{}\nstderr:\n{}",
            output.status.code().map_or_else(
                || String::from("terminated by signal"),
                |code| code.to_string()
            ),
            stdout,
            stderr
        ));
        Ok(ToolOutput {
            content,
            is_error: !output.status.success(),
        })
    }

    async fn write_file(&self, arguments: &str) -> Result<ToolOutput, ToolError> {
        let arguments: WriteFileArguments = serde_json::from_str(arguments)?;
        if arguments.content.len() > MAX_OUTPUT_BYTES {
            return Err(ToolError::ContentTooLarge(arguments.content.len()));
        }
        let path = self.resolve_write_path(&arguments.path)?;
        tokio::fs::write(&path, arguments.content.as_bytes()).await?;
        Ok(ToolOutput {
            content: format!(
                "Wrote {} bytes to {}",
                arguments.content.len(),
                arguments.path
            ),
            is_error: false,
        })
    }

    async fn replace_text(&self, arguments: &str) -> Result<ToolOutput, ToolError> {
        let arguments: ReplaceTextArguments = serde_json::from_str(arguments)?;
        if arguments.old_text.is_empty() {
            return Err(ToolError::EmptyOldText);
        }
        if arguments.new_text.len() > MAX_OUTPUT_BYTES {
            return Err(ToolError::ContentTooLarge(arguments.new_text.len()));
        }
        let path = self.resolve_existing_path(&arguments.path)?;
        if !path.is_file() {
            return Err(ToolError::NotFile { path });
        }
        let content = String::from_utf8(tokio::fs::read(&path).await?)?;
        let matches = content.matches(&arguments.old_text).count();
        if matches != 1 {
            return Err(ToolError::ExpectedUniqueMatch { matches });
        }
        let updated = content.replacen(&arguments.old_text, &arguments.new_text, 1);
        tokio::fs::write(&path, updated.as_bytes()).await?;
        Ok(ToolOutput {
            content: format!("Replaced one occurrence in {}", arguments.path),
            is_error: false,
        })
    }

    fn resolve_existing_path(&self, requested: &str) -> Result<PathBuf, ToolError> {
        let requested = Path::new(requested);
        let candidate = if requested.is_absolute() {
            requested.to_path_buf()
        } else {
            self.root.join(requested)
        };
        let canonical = candidate.canonicalize().map_err(ToolError::FileAccess)?;
        if !canonical.starts_with(&self.root) {
            return Err(ToolError::OutsideWorkspace { path: canonical });
        }
        Ok(canonical)
    }

    fn resolve_write_path(&self, requested: &str) -> Result<PathBuf, ToolError> {
        let requested = Path::new(requested);
        let candidate = if requested.is_absolute() {
            requested.to_path_buf()
        } else {
            self.root.join(requested)
        };
        if candidate.exists() {
            let canonical = candidate.canonicalize().map_err(ToolError::FileAccess)?;
            if !canonical.starts_with(&self.root) {
                return Err(ToolError::OutsideWorkspace { path: canonical });
            }
            return Ok(canonical);
        }
        let parent = candidate
            .parent()
            .ok_or_else(|| ToolError::InvalidWritePath {
                path: candidate.clone(),
            })?;
        let file_name = candidate
            .file_name()
            .ok_or_else(|| ToolError::InvalidWritePath {
                path: candidate.clone(),
            })?;
        let parent = parent.canonicalize().map_err(ToolError::FileAccess)?;
        if !parent.starts_with(&self.root) {
            return Err(ToolError::OutsideWorkspace { path: parent });
        }
        Ok(parent.join(file_name))
    }
}

fn configured_shell(command: &str, executable: Option<&str>) -> Result<Command, ToolError> {
    let Some(executable) = executable.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(platform_shell(command));
    };
    if executable.len() > 4_096 || executable.chars().any(char::is_control) {
        return Err(ToolError::InvalidShellExecutable);
    }
    let mut process = Command::new(executable);
    configure_shell_arguments(&mut process, executable, command);
    Ok(process)
}

#[cfg(windows)]
fn configure_shell_arguments(process: &mut Command, executable: &str, command: &str) {
    let name = Path::new(executable)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    if name.eq_ignore_ascii_case("powershell.exe") || name.eq_ignore_ascii_case("pwsh.exe") {
        process.args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            command,
        ]);
    } else if name.eq_ignore_ascii_case("cmd.exe") {
        process.args(["/D", "/S", "/C", command]);
    } else {
        process.args(["-lc", command]);
    }
}

#[cfg(not(windows))]
fn configure_shell_arguments(process: &mut Command, _executable: &str, command: &str) {
    process.args(["-lc", command]);
}

#[must_use]
pub fn definitions() -> Vec<Value> {
    definitions::all()
}

#[must_use]
pub fn builtin_metadata() -> Vec<BuiltinToolMetadata> {
    definitions()
        .into_iter()
        .filter_map(|definition| {
            let function = definition.get("function")?;
            Some(BuiltinToolMetadata {
                name: function.get("name")?.as_str()?.to_owned(),
                description: function
                    .get("description")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
            })
        })
        .collect()
}

#[must_use]
pub fn definition_name(definition: &Value) -> Option<&str> {
    definition.pointer("/function/name").and_then(Value::as_str)
}

#[must_use]
pub fn is_builtin(tool_name: &str) -> bool {
    builtin_metadata()
        .iter()
        .any(|metadata| metadata.name == tool_name)
}

#[derive(Debug, Deserialize)]
struct ReadFileArguments {
    path: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReplaceTextArguments {
    path: String,
    old_text: String,
    new_text: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ShellArguments {
    command: String,
    #[serde(default)]
    timeout_ms: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct WriteFileArguments {
    path: String,
    content: String,
}

#[cfg(windows)]
fn platform_shell(command: &str) -> Command {
    let mut process = Command::new("cmd.exe");
    process.args(["/D", "/S", "/C", command]);
    process
}

#[cfg(not(windows))]
fn platform_shell(command: &str) -> Command {
    let mut process = Command::new("sh");
    process.args(["-lc", command]);
    process
}

fn truncate(mut output: String) -> String {
    if output.len() <= MAX_OUTPUT_BYTES {
        return output;
    }
    let mut boundary = MAX_OUTPUT_BYTES;
    while !output.is_char_boundary(boundary) {
        boundary -= 1;
    }
    output.truncate(boundary);
    output.push_str("\n[output truncated]");
    output
}

fn relative_display(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    #[error("workspace root could not be opened: {0}")]
    WorkspaceRoot(std::io::Error),
    #[error("workspace root is not a directory: {}", path.display())]
    WorkspaceNotDirectory { path: PathBuf },
    #[error("tool arguments are invalid: {0}")]
    InvalidArguments(#[from] serde_json::Error),
    #[error("file could not be resolved: {0}")]
    FileAccess(std::io::Error),
    #[error("path is outside the workspace: {}", path.display())]
    OutsideWorkspace { path: PathBuf },
    #[error("path is not a file: {}", path.display())]
    NotFile { path: PathBuf },
    #[error("path is not a directory: {}", path.display())]
    NotDirectory { path: PathBuf },
    #[error("write path is invalid: {}", path.display())]
    InvalidWritePath { path: PathBuf },
    #[error("write content is too large: {0} bytes")]
    ContentTooLarge(usize),
    #[error("replacement oldText cannot be empty")]
    EmptyOldText,
    #[error("replacement requires exactly one match, found {matches}")]
    ExpectedUniqueMatch { matches: usize },
    #[error("file is not valid UTF-8: {0}")]
    InvalidUtf8(#[from] std::string::FromUtf8Error),
    #[error("search query cannot be empty")]
    EmptySearchQuery,
    #[error("tool I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("shell command cannot be empty")]
    EmptyCommand,
    #[error("shell command timed out after {timeout_ms} ms")]
    ShellTimedOut { timeout_ms: u64 },
    #[error("configured shell executable is invalid")]
    InvalidShellExecutable,
    #[error("unknown tool: {0}")]
    UnknownTool(String),
}

#[cfg(test)]
#[path = "tool_tests.rs"]
mod tests;
