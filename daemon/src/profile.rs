//! Reads existing Stream Dock `.sdProfile\manifest.json` scenes unchanged.
use serde::Deserialize;
use std::{collections::BTreeMap, fs, path::{Component, Path, PathBuf}};

#[derive(Deserialize, Debug)]
pub struct State {
    #[serde(rename = "Image", default)]
    pub image: String,
}

#[derive(Deserialize, Debug)]
pub struct Action {
    #[serde(rename = "UUID", default)]
    pub uuid: String,
    #[serde(rename = "Name", default)]
    pub name: String,
    #[serde(rename = "State", default)]
    pub state: usize,
    #[serde(rename = "States", default)]
    pub states: Vec<State>,
    #[serde(rename = "Settings", default)]
    pub settings: serde_json::Value,
}

#[derive(Deserialize, Debug)]
struct Manifest {
    #[serde(rename = "Actions", default)]
    actions: BTreeMap<String, Action>,
}

pub struct Scene {
    pub dir: PathBuf,
    /// ((col,row), action)
    pub actions: Vec<((u8, u8), Action)>,
}

/// A manifest image is a plain relative path: no drive/UNC prefix, root or `..` component.
fn plain_relative(img: &str) -> bool {
    !img.is_empty()
        && !img.contains(':')
        && !Path::new(img).components().any(|c| matches!(c, Component::Prefix(_) | Component::RootDir | Component::ParentDir))
}

/// `base/rel` when it is a file whose canonical path stays inside `base` (no link escapes).
fn inside(base: &Path, rel: &str) -> Option<PathBuf> {
    let root = base.canonicalize().ok()?;
    let path = base.join(rel);
    let real = path.canonicalize().ok()?;
    (path.is_file() && real.starts_with(&root)).then_some(path)
}

impl Scene {
    pub fn load(dir: &Path) -> Result<Scene, String> {
        let txt = fs::read_to_string(dir.join("manifest.json")).map_err(|e| e.to_string())?;
        Self::parse(dir, &txt)
    }

    pub fn parse(dir: &Path, json: &str) -> Result<Scene, String> {
        let m: Manifest = serde_json::from_str(json.trim_start_matches('\u{feff}')).map_err(|e| e.to_string())?;
        let actions = m
            .actions
            .into_iter()
            .filter_map(|(k, a)| {
                let (c, r) = k.split_once(',')?;
                Some(((c.parse().ok()?, r.parse().ok()?), a))
            })
            .collect();
        Ok(Scene { dir: dir.to_path_buf(), actions })
    }

    /// Static image of an action's current state. Bare names live in `<profile>/Images/`; otherwise the
    /// path is relative to the owning plugin (`<stem>.sdPlugin` where stem prefixes the action UUID),
    /// usually without extension. Absolute paths and `..` never leave either folder. Plugins repaint
    /// keys at runtime; this is the placeholder.
    pub fn image_of(&self, a: &Action, plugins: &Path) -> Option<PathBuf> {
        let s = a.states.get(a.state).or_else(|| a.states.first())?;
        let img = s.image.as_str();
        if !plain_relative(img) {
            return None;
        }
        if let Some(p) = inside(&self.dir.join("Images"), img) {
            return Some(p);
        }
        let plugin = fs::read_dir(plugins).ok()?.flatten().map(|e| e.path()).find(|p| {
            p.file_name().and_then(|n| n.to_str()).and_then(|n| n.strip_suffix(".sdPlugin"))
                .is_some_and(|stem| a.uuid.starts_with(&format!("{stem}.")))
        })?;
        ["", ".png", ".jpg", ".jpeg"].iter().find_map(|e| inside(&plugin, &format!("{img}{e}")))
    }
}

/// Re-reads `manifest`, sets only `Actions["<col>,<row>"].Settings` and writes it back atomically
/// with the editor's layout (4-space, sorted keys). Every other field and cell is kept. Returns
/// `false` (and does not write) when the value is already there.
pub fn save_action_settings(manifest: &Path, col: u8, row: u8, settings: &serde_json::Value) -> Result<bool, String> {
    let bytes = fs::read(manifest).map_err(|e| e.to_string())?;
    let text = String::from_utf8_lossy(bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(&bytes));
    let mut doc: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let key = format!("{col},{row}");
    let Some(action) = doc.get_mut("Actions").and_then(|a| a.get_mut(&key)).and_then(|a| a.as_object_mut()) else {
        return Ok(false);
    };
    if action.get("Settings") == Some(settings) {
        return Ok(false);
    }
    action.insert("Settings".into(), settings.clone());
    orcadeck_web::store::atomic_write(manifest, &orcadeck_web::store::to_pretty(&doc), true).map_err(|e| e.to_string())?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("orcadeck-profile-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn parses_real_shape() {
        let j = r#"{"Actions":{"0,0":{"UUID":"x.y","Name":"N","State":0,"States":[{"Image":"a.png"}]},"5,2":{"UUID":"z","States":[]},"bad":{}}}"#;
        let s = Scene::parse(Path::new("d"), j).unwrap();
        assert_eq!(s.actions.len(), 2);
        assert_eq!(s.actions[0].0, (0, 0));
        assert_eq!(s.image_of(&s.actions[1].1, Path::new("nope")), None);
    }

    #[test]
    fn saves_only_the_target_cell_and_keeps_the_rest() {
        let dir = tmp("settings");
        let path = dir.join("manifest.json");
        let original = r#"{"Name":"S","Custom":{"z":1,"a":2},"Actions":{"0,0":{"UUID":"x","Name":"A","Settings":{"old":true},"Extra":1},"1,0":{"UUID":"y","Settings":{"k":2}}}}"#;
        fs::write(&path, original).unwrap();

        assert!(save_action_settings(&path, 0, 0, &json!({"new": 1})).unwrap());
        let doc: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(doc["Actions"]["0,0"]["Settings"], json!({"new": 1}));
        assert_eq!(doc["Actions"]["0,0"]["Extra"], 1);
        assert_eq!(doc["Actions"]["0,0"]["Name"], "A");
        assert_eq!(doc["Actions"]["1,0"]["Settings"], json!({"k": 2}), "other cell untouched");
        assert_eq!(doc["Name"], "S");
        assert_eq!(doc["Custom"], json!({"a": 2, "z": 1}));
        assert_eq!(fs::read(&path).unwrap(), orcadeck_web::store::to_pretty(&doc), "4-space, sorted keys");

        assert!(!save_action_settings(&path, 0, 0, &json!({"new": 1})).unwrap(), "unchanged: no write");
        assert!(!save_action_settings(&path, 9, 9, &json!({})).unwrap(), "missing cell: no write");
    }
}

/// Containment of manifest `States[].Image` paths (REPO-AUDIT.md §4.2): `image_of` must never
/// resolve outside `<profile>/Images/` or the owning plugin folder.
#[cfg(test)]
mod manifest_image_containment {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("orcadeck-profile-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn action(image: &str) -> Action {
        Action {
            uuid: "com.test.demo.one".into(),
            name: String::new(),
            state: 0,
            states: vec![State { image: image.into() }],
            settings: serde_json::Value::Null,
        }
    }

    fn scene(dir: &Path) -> Scene {
        Scene { dir: dir.to_path_buf(), actions: vec![] }
    }

    #[test]
    fn rejects_parent_traversal_out_of_the_profile() {
        let t = tmp("escape-parent");
        let profile = t.join("profiles/S.sdProfile");
        fs::create_dir_all(profile.join("Images")).unwrap();
        fs::write(t.join("profiles/secret.png"), b"secret").unwrap();

        let found = scene(&profile).image_of(&action("../../secret.png"), &t.join("plugins"));
        assert_eq!(found, None, "resolved outside the profile: {found:?}");
    }

    #[test]
    fn rejects_absolute_image_paths() {
        let t = tmp("escape-absolute");
        let profile = t.join("profiles/S.sdProfile");
        fs::create_dir_all(profile.join("Images")).unwrap();
        let secret = t.join("secret.png");
        fs::write(&secret, b"secret").unwrap();

        let found = scene(&profile).image_of(&action(&secret.to_string_lossy()), &t.join("plugins"));
        assert_eq!(found, None, "read an absolute path outside the profile: {found:?}");
    }

    #[test]
    fn rejects_parent_traversal_out_of_the_plugin() {
        let t = tmp("escape-plugin");
        let profile = t.join("profiles/S.sdProfile");
        fs::create_dir_all(profile.join("Images")).unwrap();
        let plugins = t.join("plugins");
        fs::create_dir_all(plugins.join("com.test.demo.sdPlugin")).unwrap();
        fs::write(t.join("secret.png"), b"secret").unwrap();

        let found = scene(&profile).image_of(&action("../../secret.png"), &plugins);
        assert_eq!(found, None, "resolved outside the plugin: {found:?}");
    }
}
