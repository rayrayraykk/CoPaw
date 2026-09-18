use super::*;

struct Fixture {
    directory: tempfile::TempDir,
    home: PathBuf,
    cwd: PathBuf,
    environment: HashMap<OsString, OsString>,
}

impl Fixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().canonicalize().unwrap();
        let home = root.join("home with spaces");
        let cwd = root.join("host cwd");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(&cwd).unwrap();
        Self {
            directory,
            home,
            cwd,
            environment: HashMap::from([(OsString::from("PATH"), OsString::new())]),
        }
    }

    fn context(&self) -> DiscoveryContext<'_> {
        DiscoveryContext {
            cwd: &self.cwd,
            home: &self.home,
            environment: &self.environment,
        }
    }

    fn executable(&self, relative: &str) -> PathBuf {
        let path = self.directory.path().join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"fixture only; must not execute").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
        }
        path.canonicalize().unwrap()
    }

    fn standalone(&self) -> PathBuf {
        let path = self.context().default_install_candidate();
        let relative = path
            .strip_prefix(self.directory.path().canonicalize().unwrap())
            .unwrap();
        self.executable(relative.to_str().unwrap())
    }

    fn path(&mut self, directories: &[&Path]) {
        self.environment.insert(
            OsString::from("PATH"),
            std::env::join_paths(directories).unwrap(),
        );
    }
}

fn resolution(path: &Path, source: &str) -> BinaryResolution {
    BinaryResolution {
        path: path.to_owned(),
        source: source.to_owned(),
    }
}

#[test]
fn configured_environment_bundle_path_standalone_preserve_priority() {
    let mut fixture = Fixture::new();
    let manual = fixture.executable("manual/codex");
    let environment = fixture.executable("environment/codex");
    let sdk = fixture.executable("sdk/codex");
    let on_path = fixture.executable(if cfg!(windows) {
        "path/codex.EXE"
    } else {
        "path/codex"
    });
    let standalone = fixture.standalone();
    fixture.path(&[on_path.parent().unwrap()]);
    fixture
        .environment
        .insert("CODEX_BINARY".into(), environment.clone().into());
    let bundled = resolution(&sdk, "python-sdk");
    assert_eq!(
        fixture
            .context()
            .resolve(Some(manual.as_os_str()), Some(&bundled)),
        Some(resolution(&manual, "configured"))
    );
    assert_eq!(
        fixture.context().resolve(None, Some(&bundled)),
        Some(resolution(&environment, "environment"))
    );
    fixture.environment.remove(OsStr::new("CODEX_BINARY"));
    assert_eq!(
        fixture.context().resolve(None, Some(&bundled)),
        Some(resolution(&sdk, "python-sdk"))
    );
    assert_eq!(
        fixture.context().resolve(None, None),
        Some(resolution(&on_path, "path"))
    );
    // Literal codex is an explicit PATH request before the bundled candidate.
    assert_eq!(
        fixture
            .context()
            .resolve(Some(OsStr::new("codex")), Some(&bundled)),
        Some(resolution(&on_path, "configured"))
    );
    fixture.path(&[]);
    assert_eq!(
        fixture.context().resolve(None, None),
        Some(resolution(&standalone, "standalone"))
    );
}

#[test]
fn invalid_explicit_selection_stops_but_literal_codex_falls_back() {
    let mut fixture = Fixture::new();
    let fallback = fixture.standalone();
    assert_eq!(
        fixture
            .context()
            .resolve(Some(OsStr::new("missing-cli")), None),
        None
    );
    assert_eq!(
        fixture
            .context()
            .resolve(Some(OsStr::new("missing/codex")), None),
        None
    );
    assert_eq!(
        fixture.context().resolve(Some(OsStr::new("codex")), None),
        Some(resolution(&fallback, "standalone"))
    );
    fixture
        .environment
        .insert("CODEX_BINARY".into(), "missing/codex".into());
    assert_eq!(fixture.context().resolve(None, None), None);
    fixture
        .environment
        .insert("CODEX_BINARY".into(), "codex".into());
    assert_eq!(
        fixture.context().resolve(None, None),
        Some(resolution(&fallback, "standalone"))
    );
}

#[test]
fn relative_tilde_and_empty_path_preserve_host_scope() {
    let mut fixture = Fixture::new();
    let nested = fixture.executable("host cwd/bin/tool with spaces");
    let tilde = fixture.executable("home with spaces/bin/codex");
    assert_eq!(
        fixture
            .context()
            .resolve(Some(OsStr::new("bin/tool with spaces")), None),
        Some(resolution(&nested, "configured"))
    );
    assert_eq!(
        fixture
            .context()
            .resolve(Some(OsStr::new("~/bin/codex")), None),
        Some(resolution(&tilde, "configured"))
    );
    let local = fixture.executable(if cfg!(windows) {
        "host cwd/codex.EXE"
    } else {
        "host cwd/codex"
    });
    assert_eq!(fixture.context().resolve(None, None), None);
    fixture.path(&[Path::new(""), Path::new("")]);
    assert_eq!(
        fixture.context().resolve(None, None),
        Some(resolution(&local, "path"))
    );
}

#[test]
fn directories_missing_and_non_executable_files_are_not_installed() {
    let fixture = Fixture::new();
    assert_eq!(
        fixture
            .context()
            .resolve(Some(fixture.cwd.as_os_str()), None),
        None
    );
    let missing = fixture.cwd.join("missing");
    assert_eq!(
        fixture.context().resolve(Some(missing.as_os_str()), None),
        None
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let path = fixture.executable("no-permission/codex");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(
            fixture.context().resolve(Some(path.as_os_str()), None),
            None
        );
    }
}

#[test]
fn embedded_path_shadows_later_path_but_not_standalone() {
    let mut fixture = Fixture::new();
    let extension = fixture.executable(if cfg!(windows) {
        "openai.chatgpt-fixture/codex.EXE"
    } else {
        "openai.chatgpt-fixture/codex"
    });
    let later = fixture.executable(if cfg!(windows) {
        "later/codex.EXE"
    } else {
        "later/codex"
    });
    let fallback = fixture.standalone();
    fixture.path(&[extension.parent().unwrap(), later.parent().unwrap()]);
    assert_eq!(
        fixture.context().resolve(None, None),
        Some(resolution(&fallback, "standalone"))
    );
    assert_eq!(
        fixture.context().resolve(Some(extension.as_os_str()), None),
        None
    );
    let app = fixture.executable("ChatGPT.app/Contents/codex");
    assert_eq!(fixture.context().resolve(Some(app.as_os_str()), None), None);
    // Preserve the original SDK branch exception, without auto-discovering it.
    let bundled = resolution(&app, "python-sdk");
    assert_eq!(
        fixture.context().resolve(None, Some(&bundled)),
        Some(bundled)
    );
}

#[cfg(unix)]
#[test]
fn canonical_symlink_target_controls_embedded_rejection_and_result() {
    use std::os::unix::fs::symlink;
    let fixture = Fixture::new();
    let executable = fixture.executable("real/codex");
    let link = fixture.cwd.join("normal-link");
    symlink(&executable, &link).unwrap();
    assert_eq!(
        fixture.context().resolve(Some(link.as_os_str()), None),
        Some(resolution(&executable, "configured"))
    );
    let embedded = fixture.executable("Runtime.app/bin/codex");
    let link = fixture.cwd.join("embedded-link");
    symlink(embedded, &link).unwrap();
    assert_eq!(
        fixture.context().resolve(Some(link.as_os_str()), None),
        None
    );
    let broken = fixture.cwd.join("broken");
    symlink("does-not-exist", &broken).unwrap();
    assert_eq!(
        fixture.context().resolve(Some(broken.as_os_str()), None),
        None
    );
}

#[test]
fn windows_install_roots_and_executable_suffixes_match_original_rules() {
    let home = Path::new("host-home");
    assert_eq!(
        platform::standalone(home, None, true),
        home.join("AppData/Local/Programs/OpenAI/Codex/bin/codex.exe")
    );
    assert_eq!(
        platform::standalone(home, Some(Path::new("local")), true),
        PathBuf::from("local/Programs/OpenAI/Codex/bin/codex.exe")
    );
    assert_eq!(
        platform::standalone(home, Some(Path::new("")), true),
        platform::standalone(home, None, true)
    );
    assert_eq!(
        platform::standalone(home, None, false),
        home.join(".local/bin/codex")
    );
    assert_eq!(
        platform::windows_names(OsStr::new("codex"), Some(OsStr::new(".EXE;.CMD;;.BAT..."))),
        vec![
            OsString::from("codex.EXE"),
            "codex.CMD".into(),
            "codex.BAT".into()
        ]
    );
    assert_eq!(
        platform::windows_names(OsStr::new("CODEX.exe"), Some(OsStr::new(".EXE;.CMD"))),
        vec![
            OsString::from("CODEX.exe"),
            "CODEX.exe.EXE".into(),
            "CODEX.exe.CMD".into()
        ]
    );
}

#[tokio::test]
#[ignore = "requires qwenpaw Python environment; compare original Codex discovery"]
async fn discovery_matches_original_python_function_on_fixture_files() {
    let mut fixture = Fixture::new();
    let manual = fixture.executable("manual/codex");
    let environment = fixture.executable("environment/codex");
    let sdk = fixture.executable("sdk/codex");
    let on_path = fixture.executable(if cfg!(windows) {
        "path/codex.EXE"
    } else {
        "path/codex"
    });
    let embedded = fixture.executable(if cfg!(windows) {
        "Tool.app/bin/codex.EXE"
    } else {
        "Tool.app/bin/codex"
    });
    fixture.standalone();
    for index in 0..18 {
        fixture.environment.remove(OsStr::new("CODEX_BINARY"));
        fixture.path(&[]);
        let (configured, bundled) = match index {
            0 => (None, None),
            1 => (Some(manual.clone()), Some(sdk.clone())),
            2 => {
                fixture
                    .environment
                    .insert("CODEX_BINARY".into(), environment.clone().into());
                (None, Some(sdk.clone()))
            }
            3 => (None, Some(sdk.clone())),
            4 => {
                fixture.path(&[on_path.parent().unwrap()]);
                (None, None)
            }
            5 => {
                fixture.path(&[on_path.parent().unwrap()]);
                (Some("codex".into()), Some(sdk.clone()))
            }
            6 => (Some("missing-cli".into()), Some(sdk.clone())),
            7 => (Some("codex".into()), None),
            8 => {
                fixture
                    .environment
                    .insert("CODEX_BINARY".into(), "missing-cli".into());
                (None, Some(sdk.clone()))
            }
            9 => {
                fixture
                    .environment
                    .insert("CODEX_BINARY".into(), "codex".into());
                (None, Some(sdk.clone()))
            }
            10 => (Some(embedded.clone()), None),
            11 => {
                fixture.path(&[embedded.parent().unwrap(), on_path.parent().unwrap()]);
                (None, None)
            }
            12 => (None, Some(embedded.clone())),
            13 => (Some(fixture.cwd.clone()), None),
            14 => (Some("../manual/codex".into()), None),
            15 => (
                Some(if cfg!(windows) {
                    "~/AppData/Local/Programs/OpenAI/Codex/bin/codex.exe".into()
                } else {
                    "~/.local/bin/codex".into()
                }),
                None,
            ),
            16 => (Some("./../manual/codex".into()), None),
            17 => (
                Some(if cfg!(windows) {
                    "./~/AppData/Local/Programs/OpenAI/Codex/bin/codex.exe".into()
                } else {
                    "./~/.local/bin/codex".into()
                }),
                None,
            ),
            _ => unreachable!(),
        };
        assert_reference_case(&fixture, configured, bundled, index).await;
    }
}

async fn assert_reference_case(
    fixture: &Fixture,
    configured: Option<PathBuf>,
    bundled: Option<PathBuf>,
    index: usize,
) {
    let script =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/codex_discovery_reference.py");
    let candidate = bundled.as_ref().map(|path| resolution(path, "python-sdk"));
    let actual = fixture.context().resolve(
        configured.as_deref().map(Path::as_os_str),
        candidate.as_ref(),
    );
    let environment: std::collections::BTreeMap<_, _> = fixture
        .environment
        .iter()
        .map(|(key, value)| (key.to_str().unwrap(), value.to_str().unwrap()))
        .collect();
    let input = serde_json::json!({"cwd":fixture.cwd,"home":fixture.home,"environment":environment,"configured":configured,"bundled":bundled});
    let output = tokio::time::timeout(
        std::time::Duration::from_secs(20),
        tokio::process::Command::new("python")
            .arg(&script)
            .arg(input.to_string())
            .kill_on_drop(true)
            .output(),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(
        output.status.success(),
        "case {index}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let expected: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        serde_json::to_value(actual).unwrap(),
        expected,
        "case {index}"
    );
}

#[test]
fn current_directory_search_requires_windows_and_no_opt_out_variable() {
    assert!(platform::search_current_directory(true, None));
    assert!(!platform::search_current_directory(false, None));
    for value in ["", "0", "1"] {
        assert!(!platform::search_current_directory(
            true,
            Some(OsStr::new(value))
        ));
    }
}

#[test]
fn relative_context_is_rejected_and_bundle_reports_actual_source() {
    let fixture = Fixture::new();
    let binary = fixture.executable("distribution/codex");
    let bundled = resolution(&binary, "bundled");
    assert_eq!(
        fixture.context().resolve(None, Some(&bundled)),
        Some(bundled.clone())
    );
    let context = DiscoveryContext {
        cwd: Path::new("relative"),
        ..fixture.context()
    };
    assert_eq!(context.resolve(None, Some(&bundled)), None);
    let context = DiscoveryContext {
        home: Path::new("relative"),
        ..fixture.context()
    };
    assert_eq!(context.resolve(None, Some(&bundled)), None);
}
