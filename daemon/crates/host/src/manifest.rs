//! Plugin discovery: reads `<uuid>.sdPlugin/manifest.json`, or the reconstructed overlay
//! `<dataRoot>/manifests/<uuid>.json` when the shipped manifest is an encrypted Elgato one.
//! Encrypted manifests are never decrypted (see docs/design/plugin-install-handoff.md §1).

use serde_json::Value;
use std::fmt;
use std::path::{Path, PathBuf};

/// A launchable plugin.
#[derive(Debug, Clone)]
pub struct Plugin {
    /// `manifest.UUID`, or the folder name without `.sdPlugin`.
    pub id: String,
    pub dir: PathBuf,
    /// `CodePathWin` if present, else `CodePath`. Relative to `dir`.
    pub code_path: String,
    pub manifest: Value,
}

/// Why a folder was not turned into a [`Plugin`].
#[derive(Debug, PartialEq)]
pub enum Skip {
    /// No `manifest.json` in the folder.
    Missing,
    /// Encrypted manifest and no overlay in `<dataRoot>/manifests/`.
    NeedsManifest,
    /// Stream Dock built-in, implemented natively by the vendor app.
    PrivateApi,
    NoCodePath,
    Invalid(String),
}

impl fmt::Display for Skip {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Skip::Missing => f.write_str("no manifest.json"),
            Skip::NeedsManifest => f.write_str("encrypted manifest and no overlay"),
            Skip::PrivateApi => f.write_str("PrivateAPI built-in"),
            Skip::NoCodePath => f.write_str("no CodePath"),
            Skip::Invalid(e) => write!(f, "invalid manifest: {e}"),
        }
    }
}

impl Plugin {
    /// Builds a plugin from an already parsed manifest.
    pub fn from_manifest(dir: &Path, manifest: Value) -> Result<Plugin, Skip> {
        if !manifest.is_object() {
            return Err(Skip::Invalid("not a JSON object".into()));
        }
        if manifest.get("PrivateAPI").is_some_and(|v| v != &Value::Bool(false)) {
            return Err(Skip::PrivateApi);
        }
        let code_path = ["CodePathWin", "CodePath"]
            .iter()
            .find_map(|k| manifest.get(*k)?.as_str().filter(|s| !s.is_empty()))
            .ok_or(Skip::NoCodePath)?
            .to_string();
        let id = match manifest.get("UUID").and_then(Value::as_str) {
            Some(u) if !u.is_empty() => u.to_string(),
            _ => folder_id(dir),
        };
        Ok(Plugin { id, dir: dir.to_path_buf(), code_path, manifest })
    }

    /// `Software.MinimumVersion`, if the manifest has one.
    pub fn min_version(&self) -> Option<&str> {
        self.manifest.pointer("/Software/MinimumVersion")?.as_str()
    }

    pub fn version(&self) -> &str {
        self.manifest.get("Version").and_then(Value::as_str).unwrap_or("0.0.0")
    }
}

/// Folder name without the `.sdPlugin` suffix.
fn folder_id(dir: &Path) -> String {
    let name = dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    match name.len().checked_sub(".sdPlugin".len()) {
        Some(n) if name[n..].eq_ignore_ascii_case(".sdPlugin") => name[..n].to_string(),
        _ => name,
    }
}

/// Loads one `*.sdPlugin` folder. `data_root` holds `manifests/<uuid>.json` overlays.
pub fn load(dir: &Path, data_root: Option<&Path>) -> Result<Plugin, Skip> {
    let raw = std::fs::read(dir.join("manifest.json")).map_err(|_| Skip::Missing)?;
    let raw = if raw.starts_with(b"ELGATO") {
        let overlay = data_root
            .map(|d| d.join("manifests").join(format!("{}.json", folder_id(dir))))
            .ok_or(Skip::NeedsManifest)?;
        std::fs::read(overlay).map_err(|_| Skip::NeedsManifest)?
    } else {
        raw
    };
    let text = raw.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(&raw);
    let manifest = serde_json::from_slice(text).map_err(|e| Skip::Invalid(e.to_string()))?;
    Plugin::from_manifest(dir, manifest)
}

/// Scans every `*.sdPlugin` folder under `roots`. Skipped folders are logged. When two roots
/// hold the same plugin id, the first root wins.
pub fn discover(roots: &[PathBuf], data_root: Option<&Path>) -> Vec<Plugin> {
    let mut out: Vec<Plugin> = Vec::new();
    for root in roots {
        let Ok(entries) = std::fs::read_dir(root) else { continue };
        for dir in entries.flatten().map(|e| e.path()) {
            let is_plugin = dir.is_dir()
                && dir.extension().is_some_and(|e| e.eq_ignore_ascii_case("sdPlugin"));
            if !is_plugin {
                continue;
            }
            match load(&dir, data_root) {
                Ok(p) if out.iter().any(|q| q.id == p.id) => {
                    log::info!("{}: duplicate of {}, skipped", dir.display(), p.id)
                }
                Ok(p) => out.push(p),
                Err(why) => log::info!("{}: skipped, {why}", dir.display()),
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("orcadeck-host-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn plugin_dir(root: &Path, name: &str, manifest: &[u8]) -> PathBuf {
        let d = root.join(name);
        fs::create_dir_all(&d).unwrap();
        fs::write(d.join("manifest.json"), manifest).unwrap();
        d
    }

    #[test]
    fn plain_manifest_with_bom_and_folder_id() {
        let root = tmp("plain");
        let d = plugin_dir(&root, "com.x.y.sdPlugin", b"\xEF\xBB\xBF{\"CodePath\":\"a.js\",\"CodePathWin\":\"b.exe\"}");
        let p = load(&d, None).unwrap();
        assert_eq!(p.id, "com.x.y");
        assert_eq!(p.code_path, "b.exe");
    }

    #[test]
    fn skips() {
        let root = tmp("skips");
        let d = plugin_dir(&root, "a.sdPlugin", br#"{"CodePath":"x.exe","PrivateAPI":true}"#);
        assert_eq!(load(&d, None).unwrap_err(), Skip::PrivateApi);
        let d = plugin_dir(&root, "b.sdPlugin", br#"{"UUID":"b"}"#);
        assert_eq!(load(&d, None).unwrap_err(), Skip::NoCodePath);
        let d = plugin_dir(&root, "c.sdPlugin", b"ELGATO\x01\x00garbage");
        assert_eq!(load(&d, Some(&root)).unwrap_err(), Skip::NeedsManifest);
        assert!(matches!(load(&root.join("none"), None), Err(Skip::Missing)));
    }

    #[test]
    fn encrypted_manifest_uses_overlay() {
        let root = tmp("overlay");
        let d = plugin_dir(&root, "com.enc.sdPlugin", b"ELGATO\x01\x00ciphertext");
        let data = root.join("data");
        fs::create_dir_all(data.join("manifests")).unwrap();
        fs::write(data.join("manifests/com.enc.json"), br#"{"UUID":"com.enc","CodePath":"bin/p.exe"}"#).unwrap();
        let p = load(&d, Some(&data)).unwrap();
        assert_eq!((p.id.as_str(), p.code_path.as_str()), ("com.enc", "bin/p.exe"));
    }

    #[test]
    fn discover_skips_and_dedups() {
        let root = tmp("discover");
        plugin_dir(&root, "one.sdPlugin", br#"{"UUID":"dup","CodePath":"a.js"}"#);
        plugin_dir(&root, "two.sdPlugin", br#"{"UUID":"dup","CodePath":"b.js"}"#);
        plugin_dir(&root, "bad.sdPlugin", b"{not json");
        plugin_dir(&root, "notaplugin", br#"{"CodePath":"a.js"}"#);
        let found = discover(&[root], None);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].id, "dup");
    }
}
