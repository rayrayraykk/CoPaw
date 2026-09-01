//! Backend command construction for development and packaged builds.

use std::path::{Path, PathBuf};

use tauri::Manager;
use tauri_plugin_shell::{process::Command, ShellExt};

const RUST_CORE_DEFAULT_WORKSPACE_ENV: &str = "QWENPAW_DEFAULT_WORKSPACE";
#[cfg(debug_assertions)]
const CONSOLE_STATIC_DIR_ENV: &str = "QWENPAW_CONSOLE_STATIC_DIR";

/// Builds the command used to start the development Rust Core sidecar.
#[cfg(debug_assertions)]
pub(super) fn create(app: &tauri::AppHandle) -> Result<Command, String> {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    create_development_rust_core(app, &repo_root)
}

/// Builds the command used to start the packaged Rust Core sidecar.
#[cfg(not(debug_assertions))]
pub(super) fn create(app: &tauri::AppHandle) -> Result<Command, String> {
    create_packaged_rust_core(app)
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
    Ok(command
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

#[cfg(test)]
mod tests {
    use super::*;

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
