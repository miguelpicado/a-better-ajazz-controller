//! One-shot staging for plugin packages opened from the OS shell (double click / `orcadeck install`).
//!
//! The shell hands over a path; the editor page must never receive it. The path stays in this
//! process-wide registry behind a random token and is consumed by the first successful install.
//! The token is not an authorization: the API still requires the editor session token/cookie.
use std::{
    path::{Path, PathBuf},
    sync::Mutex,
};

#[derive(Clone)]
pub(crate) struct Staged {
    pub path: PathBuf,
    /// Base name only, for the page to show what the user picked.
    pub name: String,
}

static STAGED: Mutex<Vec<(String, Staged)>> = Mutex::new(Vec::new());

/// Canonical package path. `None` when it is not an installable package.
pub(crate) fn package_kind_error(path: &Path) -> Option<&'static str> {
    let name = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_ascii_lowercase();
    if path.is_dir() {
        if !name.ends_with(".sdplugin") || !path.join("manifest.json").is_file() {
            return Some("la carpeta no es un .sdPlugin con manifest.json");
        }
    } else if !path.is_file()
        || !["streamdeckplugin", "sdplugin", "zip"]
            .iter()
            .any(|e| name.ends_with(&format!(".{e}")))
    {
        return Some("se esperaba un .streamDeckPlugin/.sdPlugin/.zip o una carpeta .sdPlugin");
    }
    None
}

/// Validates a path from the shell and returns it canonicalized (`..` and links resolved).
pub(crate) fn validate(raw: &Path) -> Result<PathBuf, String> {
    let path = raw
        .canonicalize()
        .map_err(|_| format!("no existe: {}", raw.display()))?;
    match package_kind_error(&path) {
        Some(e) => Err(e.to_string()),
        None => Ok(path),
    }
}

fn insert(path: PathBuf) -> String {
    let token: String = crate::random::<16>()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let name = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    let mut reg = STAGED.lock().unwrap_or_else(|e| e.into_inner());
    // ponytail: last 8 staged packages win; nobody double-clicks faster than that.
    if reg.len() >= 8 {
        reg.remove(0);
    }
    reg.push((token.clone(), Staged { path, name }));
    token
}

pub(crate) fn peek(token: &str) -> Option<Staged> {
    STAGED
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .iter()
        .find(|(t, _)| t == token)
        .map(|(_, s)| s.clone())
}

pub(crate) fn consume(token: &str) {
    STAGED
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .retain(|(t, _)| t != token);
}

/// Validates `raw` and keeps it for one install. Returns the single-use token.
pub fn stage_package(raw: &str) -> Result<String, String> {
    Ok(insert(validate(Path::new(raw))?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_plugin_packages_are_staged_and_tokens_are_one_use() {
        let dir = std::env::temp_dir().join(format!("orcadeck-staged-unit-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("com.test.one.sdPlugin")).unwrap();
        std::fs::write(dir.join("com.test.one.sdPlugin/manifest.json"), "{}").unwrap();
        std::fs::write(dir.join("notes.txt"), "nope").unwrap();
        std::fs::write(dir.join("bundle.sdPlugin"), b"zip body").unwrap();

        assert!(stage_package(&dir.join("notes.txt").to_string_lossy()).is_err());
        assert!(stage_package(&dir.join("missing.zip").to_string_lossy()).is_err());
        assert!(stage_package(&dir.join("com.test.one.sdPlugin").to_string_lossy()).is_ok());
        assert!(
            stage_package(&dir.join("bundle.sdPlugin").to_string_lossy()).is_ok(),
            "a .sdPlugin file is a zip package"
        );

        let token = stage_package(&dir.join("com.test.one.sdPlugin").to_string_lossy()).unwrap();
        assert_eq!(token.len(), 32);
        assert!(peek(&token).is_some());
        consume(&token);
        assert!(peek(&token).is_none(), "a token cannot be used twice");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
