use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

pub(super) fn standalone(home: &Path, local: Option<&Path>, windows: bool) -> PathBuf {
    if windows {
        local
            .filter(|path| !path.as_os_str().is_empty())
            .map_or_else(|| home.join("AppData/Local"), Path::to_owned)
            .join("Programs/OpenAI/Codex/bin/codex.exe")
    } else {
        home.join(".local/bin/codex")
    }
}

pub(super) fn executable(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        nix::unistd::access(path, nix::unistd::AccessFlags::X_OK).is_ok()
    }
    #[cfg(not(unix))]
    {
        true
    }
}

pub(super) fn named_home(name: &str, home: &Path, username: Option<&OsStr>) -> Option<PathBuf> {
    #[cfg(unix)]
    {
        let _ = (home, username);
        nix::unistd::User::from_name(name)
            .ok()
            .flatten()
            .map(|user| user.dir)
    }
    #[cfg(not(unix))]
    {
        if username == Some(OsStr::new(name)) {
            Some(home.to_owned())
        } else if username.is_some() && username == home.file_name() {
            Some(home.parent()?.join(name))
        } else {
            None
        }
    }
}

pub(super) fn default_path() -> &'static OsStr {
    if cfg!(windows) {
        OsStr::new(".;C:\\bin")
    } else if cfg!(target_os = "macos") {
        OsStr::new("/usr/bin:/bin:/usr/sbin:/sbin")
    } else {
        OsStr::new("/bin:/usr/bin")
    }
}

pub(super) fn search_current_directory(windows: bool, disabled: Option<&OsStr>) -> bool {
    windows && disabled.is_none()
}

pub(super) fn windows_names(name: &OsStr, pathext: Option<&OsStr>) -> Vec<OsString> {
    let extensions = pathext
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| OsStr::new(".COM;.EXE;.BAT;.CMD;.VBS;.JS;.WS;.MSC"));
    let extensions = extensions.to_string_lossy();
    let extensions: Vec<_> = extensions
        .split(';')
        .filter(|ext| !ext.is_empty())
        .map(|ext| ext.trim_end_matches('.'))
        .collect();
    let mut names = Vec::new();
    let upper_name = name.to_string_lossy().to_uppercase();
    if extensions
        .iter()
        .any(|ext| upper_name.ends_with(&ext.to_uppercase()))
    {
        names.push(name.to_owned());
    }
    for extension in extensions {
        let mut candidate = name.to_owned();
        candidate.push(extension);
        names.push(candidate);
    }
    names
}
