use std::collections::VecDeque;
use std::path::Path;
use std::path::PathBuf;

use serde::Deserialize;

use super::ToolError;
use super::ToolOutput;

const DEFAULT_LIST_RESULTS: usize = 200;
const MAX_LIST_RESULTS: usize = 500;
const DEFAULT_SEARCH_RESULTS: usize = 100;
const MAX_SEARCH_RESULTS: usize = 200;
const MAX_SCANNED_FILES: usize = 10_000;
const MAX_SEARCH_FILE_BYTES: u64 = 1_048_576;
const MAX_MATCH_LINE_CHARS: usize = 300;
const IGNORED_DIRECTORIES: &[&str] = &[".git", ".venv", "build", "dist", "node_modules", "target"];

pub(super) fn list_files(root: &Path, arguments: &str) -> Result<ToolOutput, ToolError> {
    let arguments: ListFilesArguments = serde_json::from_str(arguments)?;
    let start = resolve_path(root, arguments.path.as_deref().unwrap_or("."))?;
    let limit = arguments
        .max_results
        .unwrap_or(DEFAULT_LIST_RESULTS)
        .clamp(1, MAX_LIST_RESULTS);
    let files = collect_files(root, &start, limit)?;
    let content = if files.is_empty() {
        String::from("No files found.")
    } else {
        files.join("\n")
    };
    Ok(ToolOutput {
        content,
        is_error: false,
    })
}

pub(super) fn search_text(root: &Path, arguments: &str) -> Result<ToolOutput, ToolError> {
    let arguments: SearchTextArguments = serde_json::from_str(arguments)?;
    if arguments.query.is_empty() {
        return Err(ToolError::EmptySearchQuery);
    }
    let start = resolve_path(root, arguments.path.as_deref().unwrap_or("."))?;
    let limit = arguments
        .max_results
        .unwrap_or(DEFAULT_SEARCH_RESULTS)
        .clamp(1, MAX_SEARCH_RESULTS);
    let paths = collect_files(root, &start, MAX_SCANNED_FILES)?;
    let mut matches = Vec::new();
    for relative in paths {
        let path = root.join(relative_path(&relative));
        if std::fs::metadata(&path)?.len() > MAX_SEARCH_FILE_BYTES {
            continue;
        }
        let Ok(content) = std::fs::read_to_string(path) else {
            continue;
        };
        for (index, line) in content.lines().enumerate() {
            if line.contains(&arguments.query) {
                matches.push(format!(
                    "{}:{}:{}",
                    relative,
                    index + 1,
                    truncate_chars(line, MAX_MATCH_LINE_CHARS)
                ));
                if matches.len() == limit {
                    break;
                }
            }
        }
        if matches.len() == limit {
            break;
        }
    }
    let content = if matches.is_empty() {
        String::from("No matches found.")
    } else {
        matches.join("\n")
    };
    Ok(ToolOutput {
        content,
        is_error: false,
    })
}

fn collect_files(root: &Path, start: &Path, limit: usize) -> Result<Vec<String>, ToolError> {
    if start.is_file() {
        return Ok(vec![relative_display(root, start)]);
    }
    if !start.is_dir() {
        return Err(ToolError::NotDirectory {
            path: start.to_path_buf(),
        });
    }
    let mut directories = VecDeque::from([start.to_path_buf()]);
    let mut files = Vec::new();
    while let Some(directory) = directories.pop_front() {
        let mut entries = std::fs::read_dir(directory)?.collect::<Result<Vec<_>, _>>()?;
        entries.sort_by_key(std::fs::DirEntry::file_name);
        for entry in entries {
            let file_type = entry.file_type()?;
            if file_type.is_symlink() {
                continue;
            }
            if file_type.is_dir() {
                if !is_ignored_directory(&entry.file_name()) {
                    directories.push_back(entry.path());
                }
            } else if file_type.is_file() {
                files.push(relative_display(root, &entry.path()));
                if files.len() == limit {
                    return Ok(files);
                }
            }
        }
    }
    Ok(files)
}

fn resolve_path(root: &Path, requested: &str) -> Result<PathBuf, ToolError> {
    let requested = Path::new(requested);
    let candidate = if requested.is_absolute() {
        requested.to_path_buf()
    } else {
        root.join(requested)
    };
    let canonical = candidate.canonicalize().map_err(ToolError::FileAccess)?;
    if !canonical.starts_with(root) {
        return Err(ToolError::OutsideWorkspace { path: canonical });
    }
    Ok(canonical)
}

fn relative_display(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

fn relative_path(display: &str) -> PathBuf {
    display.split('/').collect()
}

fn is_ignored_directory(name: &std::ffi::OsStr) -> bool {
    let name = name.to_string_lossy();
    IGNORED_DIRECTORIES.contains(&name.as_ref())
}

fn truncate_chars(value: &str, limit: usize) -> String {
    let mut characters = value.chars();
    let prefix = characters.by_ref().take(limit).collect::<String>();
    if characters.next().is_some() {
        format!("{prefix}…")
    } else {
        prefix
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListFilesArguments {
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    max_results: Option<usize>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SearchTextArguments {
    query: String,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    max_results: Option<usize>,
}
