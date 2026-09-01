//! Backend command construction for development and packaged builds.

use std::path::{Path, PathBuf};
#[cfg(debug_assertions)]
use std::process::{Command as StdCommand, Stdio};

use tauri::Manager;
use tauri_plugin_shell::{process::Command, ShellExt};

const RUST_CORE_SWITCH_ENV: &str = "QWENPAW_DESKTOP_RUST_CORE";
const RUST_CORE_DEFAULT_WORKSPACE_ENV: &str = "QWENPAW_DEFAULT_WORKSPACE";
#[cfg(debug_assertions)]
const CONSOLE_STATIC_DIR_ENV: &str = "QWENPAW_CONSOLE_STATIC_DIR";

/// Builds the command used to start the selected development sidecar.
#[cfg(debug_assertions)]
pub(super) fn create(app: &tauri::AppHandle) -> Result<Command, String> {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    if rust_core_requested() {
        return create_development_rust_core(app, &repo_root);
    }
    let source_path = repo_root.join("src");
    let command = if command_exists("uv") {
        log::info!(
            "[backend] dev command: uv run python -m qwenpaw.tauri.entry cwd={}",
            repo_root.display(),
        );
        app.shell()
            .command("uv")
            .args(["run", "python", "-m", "qwenpaw.tauri.entry"])
            .current_dir(repo_root)
            .env("PYTHONPATH", source_path.display().to_string())
    } else {
        let (python, prefix_args) = python_command(&repo_root);
        let mut args = prefix_args;
        args.extend(["-m", "qwenpaw.tauri.entry"]);
        log::info!(
            "[backend] dev command: {} {} cwd={}",
            python,
            args.join(" "),
            repo_root.display(),
        );
        app.shell()
            .command(python)
            .args(args)
            .current_dir(repo_root)
            .env("PYTHONPATH", source_path.display().to_string())
    };
    Ok(apply_contributed_environment(app, command))
}

/// Builds the command used to start the selected packaged sidecar.
#[cfg(not(debug_assertions))]
pub(super) fn create(app: &tauri::AppHandle) -> Result<Command, String> {
    if rust_core_requested() {
        return create_packaged_rust_core(app);
    }
    let backend = packaged_backend_executable(app)?;
    let resource_dir = app
        .path()
        .resource_dir()
        .map_err(|err| format!("failed to resolve resource directory: {err}"))?;
    let backend_dir = backend
        .parent()
        .ok_or_else(|| format!("backend executable has no parent: {}", backend.display()))?
        .to_path_buf();
    log::info!(
        "[backend] packaged command: {} cwd={}",
        backend.display(),
        backend_dir.display(),
    );
    let command = app
        .shell()
        .command(backend)
        .current_dir(&backend_dir)
        .env(path_env_key(), path_with_backend_dir(&backend_dir)?)
        .env(
            "QWENPAW_TAURI_RESOURCE_DIR",
            resource_dir.to_string_lossy().to_string(),
        );
    let mut command = apply_contributed_environment(app, command);
    // A complete Playwright Chromium payload exceeds the practical NSIS
    // installer mapping limit on Windows. The sidecar downloads the exact
    // driver-matched revision into the user's QwenPaw data directory instead.
    if cfg!(windows) {
        command = command.env("QWENPAW_DESKTOP_MANAGED_PLAYWRIGHT", "1");
    }
    // Bundled standalone Python used by the backend to install third-party
    // plugin dependencies (sys.executable is the frozen backend, not Python).
    if let Some(python) = packaged_python_runtime(app) {
        log::info!("[backend] bundled python runtime: {}", python.display());
        command = command.env(
            "QWENPAW_DESKTOP_PY_RUNTIME",
            python.to_string_lossy().to_string(),
        );
    } else {
        log::warn!(
            "[backend] bundled python runtime not found; plugin dependency \
             installation will be unavailable"
        );
    }
    if let Some(node_runtime) = packaged_node_runtime(app) {
        log::info!("[backend] bundled node runtime: {}", node_runtime.display());
        command = command.env(
            "QWENPAW_DESKTOP_NODE_RUNTIME",
            node_runtime.to_string_lossy().to_string(),
        );
    } else {
        log::warn!("[backend] bundled node runtime not found");
    }
    Ok(command)
}

fn rust_core_requested() -> bool {
    rust_core_switch_enabled(std::env::var(RUST_CORE_SWITCH_ENV).ok().as_deref())
}

fn rust_core_switch_enabled(value: Option<&str>) -> bool {
    !value.is_some_and(|value| {
        matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "0" | "false" | "no" | "off"
        )
    })
}

#[cfg(debug_assertions)]
fn create_development_rust_core(
    app: &tauri::AppHandle,
    repo_root: &Path,
) -> Result<Command, String> {
    let executable_name = if cfg!(windows) {
        "qwenpaw-core.exe"
    } else {
        "qwenpaw-core"
    };
    let core_root = repo_root.join("qwenpaw-core");
    let executable = core_root.join("target").join("debug").join(executable_name);
    let console_static_dir = std::env::var_os(CONSOLE_STATIC_DIR_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|| repo_root.join("console").join("dist"));
    create_rust_core_command(app, &executable, &console_static_dir, &core_root)
}

#[cfg(not(debug_assertions))]
fn create_packaged_rust_core(app: &tauri::AppHandle) -> Result<Command, String> {
    let resource_dir = app
        .path()
        .resource_dir()
        .map_err(|err| format!("failed to resolve resource directory: {err}"))?;
    let core_dir = resource_dir.join("binaries").join("qwenpaw-core");
    let executable_name = if cfg!(windows) {
        "qwenpaw-core.exe"
    } else {
        "qwenpaw-core"
    };
    create_rust_core_command(
        app,
        &core_dir.join(executable_name),
        &core_dir.join("console"),
        &core_dir,
    )
}

fn create_rust_core_command(
    app: &tauri::AppHandle,
    executable: &Path,
    console_static_dir: &Path,
    working_directory: &Path,
) -> Result<Command, String> {
    validate_rust_core_layout(executable, console_static_dir)?;
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|err| format!("failed to resolve app data directory: {err}"))?;
    let core_data_dir = rust_core_data_dir(&app_data_dir);
    let core_workspace_dir = rust_core_workspace_dir(&app_data_dir);
    std::fs::create_dir_all(&core_workspace_dir).map_err(|err| {
        format!(
            "failed to create Rust Core workspace {}: {err}",
            core_workspace_dir.display()
        )
    })?;
    log::info!(
        "[backend] Rust Core command: {} runtime={} workspace={} console={} data={}",
        executable.display(),
        working_directory.display(),
        core_workspace_dir.display(),
        console_static_dir.display(),
        core_data_dir.display(),
    );
    let command = app
        .shell()
        .command(executable)
        .args(rust_core_arguments(console_static_dir))
        .current_dir(&core_workspace_dir);
    Ok(apply_contributed_environment(app, command)
        .env("QWENPAW_HOME", core_data_dir)
        .env(RUST_CORE_DEFAULT_WORKSPACE_ENV, core_workspace_dir))
}

fn rust_core_arguments(console_static_dir: &Path) -> Vec<String> {
    vec![
        "app-server".to_string(),
        "--listen".to_string(),
        "127.0.0.1:0".to_string(),
        "--desktop".to_string(),
        "--console-static-dir".to_string(),
        console_static_dir.to_string_lossy().to_string(),
    ]
}

fn rust_core_data_dir(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join("rust-core-v1")
}

fn rust_core_workspace_dir(app_data_dir: &Path) -> PathBuf {
    rust_core_data_dir(app_data_dir).join("workspace")
}

fn validate_rust_core_layout(executable: &Path, console_static_dir: &Path) -> Result<(), String> {
    if !executable.is_file() {
        return Err(format!(
            "Rust Core executable not found at {}; build qwenpaw-cli first",
            executable.display()
        ));
    }
    if !console_static_dir.join("index.html").is_file() {
        return Err(format!(
            "Console build not found at {}; run the Console production build first",
            console_static_dir.display()
        ));
    }
    Ok(())
}

#[cfg(not(debug_assertions))]
fn packaged_python_runtime(app: &tauri::AppHandle) -> Option<PathBuf> {
    let base = app
        .path()
        .resource_dir()
        .ok()?
        .join("binaries")
        .join("python-runtime")
        .join("python");
    let candidates = if cfg!(windows) {
        vec![base.join("python.exe")]
    } else {
        vec![
            base.join("bin").join("python3"),
            base.join("bin").join("python"),
        ]
    };
    candidates.into_iter().find(|path| path.is_file())
}

/// Add the variables desktop features contribute to the backend's environment.
///
/// The set comes from [`crate::runtime_env`], so this stays independent of which
/// feature needs what.
fn apply_contributed_environment(app: &tauri::AppHandle, mut command: Command) -> Command {
    for (key, value) in crate::runtime_env::collect(app) {
        command = command.env(key, value);
    }
    command
}

#[cfg(not(debug_assertions))]
fn packaged_node_runtime(app: &tauri::AppHandle) -> Option<PathBuf> {
    let root = app
        .path()
        .resource_dir()
        .ok()?
        .join("binaries")
        .join("node-runtime");
    let node = if cfg!(windows) {
        root.join("node.exe")
    } else {
        root.join("bin").join("node")
    };
    node.is_file().then_some(root)
}

#[cfg(not(debug_assertions))]
fn packaged_backend_executable(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let executable_name = if cfg!(windows) {
        "qwenpaw-backend.exe"
    } else {
        "qwenpaw-backend"
    };
    let path = app
        .path()
        .resource_dir()
        .map_err(|err| format!("failed to resolve resource directory: {err}"))?
        .join("binaries")
        .join("qwenpaw-backend")
        .join(executable_name);

    if path.is_file() {
        Ok(path)
    } else {
        Err(format!(
            "backend executable not found at {}",
            path.display()
        ))
    }
}

#[cfg(not(debug_assertions))]
fn path_with_backend_dir(backend_dir: &Path) -> Result<String, String> {
    let mut paths = vec![backend_dir.to_path_buf()];
    if let Some(existing) = std::env::var_os(path_env_key()) {
        paths.extend(std::env::split_paths(&existing));
    }

    std::env::join_paths(paths)
        .map_err(|err| format!("failed to join backend PATH entries: {err}"))?
        .into_string()
        .map_err(|_| "backend PATH contains non-Unicode data".to_string())
}

#[cfg(all(not(debug_assertions), windows))]
fn path_env_key() -> &'static str {
    "Path"
}

#[cfg(all(not(debug_assertions), not(windows)))]
fn path_env_key() -> &'static str {
    "PATH"
}

#[cfg(debug_assertions)]
fn command_exists(command: &str) -> bool {
    StdCommand::new(command)
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

#[cfg(debug_assertions)]
fn local_python(repo_root: &Path) -> Option<String> {
    let candidates = if cfg!(windows) {
        vec![
            repo_root.join(".venv/Scripts/python.exe"),
            repo_root.join("venv/Scripts/python.exe"),
        ]
    } else {
        vec![
            repo_root.join(".venv/bin/python"),
            repo_root.join("venv/bin/python"),
        ]
    };

    candidates
        .into_iter()
        .find(|path| path.is_file())
        .map(|path| path.display().to_string())
}

#[cfg(debug_assertions)]
fn python_command(repo_root: &Path) -> (String, Vec<&'static str>) {
    if let Some(local) = local_python(repo_root) {
        return (local, vec![]);
    }
    #[cfg(windows)]
    {
        if command_exists("py") {
            return ("py".to_string(), vec!["-3"]);
        }
    }
    if command_exists("python3") {
        ("python3".to_string(), vec![])
    } else {
        ("python".to_string(), vec![])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rust_core_is_default_and_accepts_an_explicit_legacy_fallback() {
        assert!(rust_core_switch_enabled(None));
        for value in ["", "1", "true", "TRUE", "yes", "on", "rust", " on "] {
            assert!(rust_core_switch_enabled(Some(value)));
        }
        for value in ["0", "false", "FALSE", "no", "off", " off "] {
            assert!(!rust_core_switch_enabled(Some(value)));
        }
    }

    #[test]
    fn rust_core_layout_requires_the_binary_and_console_index() {
        let directory = tempfile::tempdir().expect("temporary directory should be created");
        let executable = directory.path().join(if cfg!(windows) {
            "qwenpaw-core.exe"
        } else {
            "qwenpaw-core"
        });
        let console = directory.path().join("console");
        std::fs::create_dir(&console).expect("Console directory should be created");

        assert!(validate_rust_core_layout(&executable, &console).is_err());
        std::fs::write(&executable, "binary").expect("binary fixture should be written");
        assert!(validate_rust_core_layout(&executable, &console).is_err());
        std::fs::write(console.join("index.html"), "index")
            .expect("Console fixture should be written");
        assert_eq!(validate_rust_core_layout(&executable, &console), Ok(()));
    }

    #[test]
    fn rust_core_arguments_use_one_random_loopback_listener() {
        assert_eq!(
            rust_core_arguments(Path::new("console-dist")),
            vec![
                "app-server",
                "--listen",
                "127.0.0.1:0",
                "--desktop",
                "--console-static-dir",
                "console-dist",
            ]
        );
    }

    #[test]
    fn rust_core_uses_a_versioned_data_directory() {
        assert_eq!(
            rust_core_data_dir(Path::new("app-data")),
            PathBuf::from("app-data").join("rust-core-v1")
        );
        assert_eq!(
            rust_core_workspace_dir(Path::new("app-data")),
            PathBuf::from("app-data")
                .join("rust-core-v1")
                .join("workspace")
        );
    }
}
