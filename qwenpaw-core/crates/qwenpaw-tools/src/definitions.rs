use serde_json::Value;
use serde_json::json;

use super::DEFAULT_SHELL_TIMEOUT_MS;
use super::MAX_SHELL_TIMEOUT_MS;
use super::MIN_SHELL_TIMEOUT_MS;

pub(super) fn all() -> Vec<Value> {
    vec![
        list_files(),
        search_text(),
        replace_text(),
        write_file(),
        read_file(),
        shell(),
    ]
}

fn list_files() -> Value {
    json!({
        "type": "function",
        "function": {
            "name": "list_files",
            "description": "List source files under a workspace directory without following symlinks or generated dependency directories.",
            "parameters": {
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "Workspace-relative directory; defaults to the workspace root"},
                    "maxResults": {"type": "integer", "minimum": 1, "maximum": 500}
                },
                "additionalProperties": false
            }
        }
    })
}

fn search_text() -> Value {
    json!({
        "type": "function",
        "function": {
            "name": "search_text",
            "description": "Search UTF-8 workspace files for a literal text query and return path, line number, and matching line.",
            "parameters": {
                "type": "object",
                "properties": {
                    "query": {"type": "string", "description": "Non-empty literal text to find"},
                    "path": {"type": "string", "description": "Workspace-relative file or directory; defaults to the workspace root"},
                    "maxResults": {"type": "integer", "minimum": 1, "maximum": 200}
                },
                "required": ["query"],
                "additionalProperties": false
            }
        }
    })
}

fn replace_text() -> Value {
    json!({
        "type": "function",
        "function": {
            "name": "replace_text",
            "description": "Replace exactly one matching text block in an existing UTF-8 workspace file after user approval.",
            "parameters": {
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "Workspace-relative file path"},
                    "oldText": {"type": "string", "description": "Exact text that must occur once"},
                    "newText": {"type": "string", "description": "Replacement text"}
                },
                "required": ["path", "oldText", "newText"],
                "additionalProperties": false
            }
        }
    })
}

fn write_file() -> Value {
    json!({
        "type": "function",
        "function": {
            "name": "write_file",
            "description": "Write a UTF-8 text file inside an existing workspace directory after user approval.",
            "parameters": {
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "Workspace-relative path"},
                    "content": {"type": "string", "description": "Complete replacement content"}
                },
                "required": ["path", "content"],
                "additionalProperties": false
            }
        }
    })
}

fn read_file() -> Value {
    json!({
        "type": "function",
        "function": {
            "name": "read_file",
            "description": "Read a UTF-8 text file inside the workspace.",
            "parameters": {
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "Workspace-relative path"}
                },
                "required": ["path"],
                "additionalProperties": false
            }
        }
    })
}

fn shell() -> Value {
    json!({
        "type": "function",
        "function": {
            "name": "shell",
            "description": "Run a shell command in the workspace after user approval.",
            "parameters": {
                "type": "object",
                "properties": {
                    "command": {"type": "string", "description": "Shell command"},
                    "timeoutMs": {
                        "type": "integer",
                        "minimum": MIN_SHELL_TIMEOUT_MS,
                        "maximum": MAX_SHELL_TIMEOUT_MS,
                        "description": format!(
                            "Requested timeout in milliseconds; defaults to {DEFAULT_SHELL_TIMEOUT_MS}"
                        )
                    }
                },
                "required": ["command"],
                "additionalProperties": false
            }
        }
    })
}
