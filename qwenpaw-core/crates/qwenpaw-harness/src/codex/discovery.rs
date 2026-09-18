//! Read-only standalone executable discovery; never starts a candidate.

use std::collections::HashMap;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

use serde::Serialize;

mod platform;

/// Canonical executable and its actual discovery source for the original UI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BinaryResolution {
    pub path: PathBuf,
    pub source: String,
}

/// Explicit host inputs; does not capture or mutate the process environment.
///
/// `cwd` and `home` must be absolute. Use the host's working directory, not an
/// agent workspace: the original executable discovery is host-scoped.
pub struct DiscoveryContext<'a> {
    pub cwd: &'a Path,
    pub home: &'a Path,
    pub environment: &'a HashMap<OsString, OsString>,
}

impl DiscoveryContext<'_> {
    /// Resolves configured, environment, bundled, PATH, then standalone CLI.
    ///
    /// The distribution owner supplies `bundled`; no Python SDK is imported.
    /// An invalid explicit choice stops discovery unless it is literal `codex`.
    /// A supplied SDK candidate may have source `python-sdk`; other bundled
    /// distributions must use their actual source, not impersonate that SDK.
    #[must_use]
    pub fn resolve(
        &self,
        configured: Option<&OsStr>,
        bundled: Option<&BinaryResolution>,
    ) -> Option<BinaryResolution> {
        if !self.cwd.is_absolute() || !self.home.is_absolute() {
            return None;
        }
        for (candidate, source) in [
            (configured, "configured"),
            (self.env("CODEX_BINARY"), "environment"),
        ] {
            if let Some(candidate) = candidate.filter(|value| !value.is_empty()) {
                let path = self.configured(candidate);
                if path.is_some() || candidate != "codex" {
                    return path.map(|path| BinaryResolution {
                        path,
                        source: source.to_owned(),
                    });
                }
            }
        }
        if let Some(candidate) = bundled {
            // The original SDK branch accepts its owned embedded executable.
            if let Some(path) = self.executable(&candidate.path, false) {
                return Some(BinaryResolution {
                    path,
                    source: candidate.source.clone(),
                });
            }
        }
        if let Some(path) = self.on_path(OsStr::new("codex"))
            && !embedded(&path)
        {
            return Some(BinaryResolution {
                path,
                source: "path".to_owned(),
            });
        }
        self.executable(&self.default_install_candidate(), true)
            .map(|path| BinaryResolution {
                path,
                source: "standalone".to_owned(),
            })
    }

    /// Known standalone location; does not inspect the filesystem.
    #[must_use]
    pub fn default_install_candidate(&self) -> PathBuf {
        platform::standalone(
            self.home,
            self.env("LOCALAPPDATA").map(Path::new),
            cfg!(windows),
        )
    }

    fn env(&self, key: &str) -> Option<&OsStr> {
        if cfg!(windows) {
            self.environment.iter().find_map(|(name, value)| {
                name.to_str()
                    .is_some_and(|name| name.eq_ignore_ascii_case(key))
                    .then_some(value.as_os_str())
            })
        } else {
            self.environment
                .get(OsStr::new(key))
                .map(OsString::as_os_str)
        }
    }

    fn configured(&self, binary: &OsStr) -> Option<PathBuf> {
        let path = self.expand_user(Path::new(binary))?;
        if path.is_absolute()
            || path
                .parent()
                .is_some_and(|parent| !parent.as_os_str().is_empty() && parent != Path::new("."))
        {
            self.executable(&path, true)
        } else {
            self.on_path(binary).filter(|path| !embedded(path))
        }
    }

    fn expand_user(&self, path: &Path) -> Option<PathBuf> {
        // pathlib removes a leading `./` before expanding a user's home.
        let path = path.strip_prefix(".").unwrap_or(path);
        let Some(first) = path.components().next() else {
            return Some(path.to_owned());
        };
        let Some(name) = first.as_os_str().to_str().and_then(|s| s.strip_prefix('~')) else {
            return Some(path.to_owned());
        };
        let home = if name.is_empty() {
            self.home.to_owned()
        } else {
            platform::named_home(name, self.home, self.env("USERNAME"))?
        };
        Some(home.join(path.strip_prefix(first.as_os_str()).ok()?))
    }

    fn absolute(&self, path: &Path) -> PathBuf {
        self.cwd.join(path)
    }

    fn executable(&self, path: &Path, reject_embedded: bool) -> Option<PathBuf> {
        let path = self.absolute(path);
        if !platform::executable(&path) {
            return None;
        }
        let canonical = path.canonicalize().ok()?;
        (!reject_embedded || !embedded(&canonical)).then_some(canonical)
    }

    fn on_path(&self, binary: &OsStr) -> Option<PathBuf> {
        let command = Path::new(binary);
        let parent = command
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty());
        let mut directories = if let Some(parent) = parent {
            vec![parent.to_owned()]
        } else {
            let search_path = self.env("PATH").unwrap_or(platform::default_path());
            if search_path.is_empty() {
                return None;
            }
            let mut directories: Vec<_> = std::env::split_paths(search_path).collect();
            if platform::search_current_directory(
                cfg!(windows),
                self.env("NoDefaultCurrentDirectoryInExePath"),
            ) {
                directories.insert(0, PathBuf::from("."));
            }
            directories
        };
        let name = command.file_name()?;
        let names = if cfg!(windows) {
            platform::windows_names(name, self.env("PATHEXT"))
        } else {
            vec![name.to_owned()]
        };
        // Do not search beyond the first executable when it is embedded: the
        // original which() result shadows later PATH entries, then falls back.
        directories.dedup();
        for directory in directories {
            for name in &names {
                if let Some(path) = self.executable(&directory.join(name), false) {
                    return Some(path);
                }
            }
        }
        None
    }
}

fn embedded(path: &Path) -> bool {
    path.components().any(|part| {
        let name = part.as_os_str().to_string_lossy();
        name.starts_with("openai.chatgpt-") || name.ends_with(".app")
    })
}

#[cfg(test)]
mod tests;
