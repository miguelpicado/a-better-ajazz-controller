//! Scene order for the built-in `com.hotspot.streamdock.profile.rotate` key (Scene Shift).
//! [`orcadeck_web::FsStore`] already reads the editor's `state.json` and seeds it from Stream
//! Dock's DataCache/ini, so this only picks the device's listed scenes, steps through them and
//! loads the chosen manifest. Everything happens on a key press: no polling, no server.
use crate::profile;
use orcadeck_web::store::{safe_id, FsStore, Store};
use std::path::{Path, PathBuf};

/// Action the app implements itself (`PrivateAPI`, no `CodePath`).
pub const ROTATE: &str = "com.hotspot.streamdock.profile.rotate";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SceneTarget {
    Next,
    Previous,
    Id(String),
}

pub struct SceneSet {
    store: FsStore,
    profiles: PathBuf,
    /// Listed scenes of this device, in the device's order.
    order: Vec<String>,
    current: String,
}

/// What the action asks for: `Settings.ProfileUUID` empty is next, `PreviousScene` previous,
/// anything else a scene id (accepted with or without the `.sdProfile` suffix).
pub fn target_of(action: &profile::Action) -> Option<SceneTarget> {
    (action.uuid == ROTATE).then(|| match action.settings.get("ProfileUUID").and_then(|v| v.as_str()).unwrap_or("") {
        "" => SceneTarget::Next,
        "PreviousScene" => SceneTarget::Previous,
        id => SceneTarget::Id(id.trim_end_matches(".sdProfile").to_string()),
    })
}

impl SceneSet {
    /// Reads the scene list once. The scene given on the command line wins over the stored current
    /// one, which is persisted so the editor agrees.
    pub fn open(profile: &Path, plugins: &Path, data: &Path, device: &str) -> Result<SceneSet, String> {
        let parent = profile.parent().unwrap_or(Path::new(""));
        let profiles = if parent.as_os_str().is_empty() { PathBuf::from(".") } else { parent.to_path_buf() };
        let legacy_config = profiles.parent().filter(|p| !p.as_os_str().is_empty()).map(|p| p.join("config"));
        let store = FsStore {
            profiles: profiles.clone(),
            plugins: plugins.to_path_buf(),
            builtin_plugins: Vec::new(),
            data: data.to_path_buf(),
            legacy_config,
            downloads: None,
        };
        let scenes = store.scenes().map_err(|e| e.msg)?;
        let order = scenes.iter().filter(|s| s.listed && s.device == device).map(|s| s.id.clone()).collect();
        let current = profile.file_name().and_then(|n| n.to_str()).unwrap_or_default().trim_end_matches(".sdProfile").to_string();
        if !scenes.iter().any(|s| s.current && s.id == current) {
            if let Err(e) = store.set_current(&current) {
                eprintln!("scene {current}: state.json: {}", e.msg);
            }
        }
        Ok(SceneSet { store, profiles, order, current })
    }

    pub fn current(&self) -> &str {
        &self.current
    }

    /// Scene id to switch to, when the explicit target exists or the order has a neighbour.
    pub fn resolve(&self, target: &SceneTarget) -> Option<String> {
        match target {
            SceneTarget::Id(id) => self.exists(id).then(|| id.clone()),
            SceneTarget::Next => self.step(1),
            SceneTarget::Previous => self.step(-1),
        }
    }

    fn exists(&self, id: &str) -> bool {
        safe_id(id) && self.profiles.join(format!("{id}.sdProfile")).join("manifest.json").is_file()
    }

    /// Next/previous with wrap-around. A current scene outside the list has no position: next is
    /// the first scene and previous the last.
    fn step(&self, delta: isize) -> Option<String> {
        let len = self.order.len() as isize;
        let i = match self.order.iter().position(|id| *id == self.current) {
            Some(i) => (i as isize + delta).rem_euclid(len),
            None if delta > 0 => 0,
            None => len - 1,
        };
        self.order.get(usize::try_from(i).ok()?).cloned()
    }

    /// Loads a scene and makes it current. Stream Dock's own files are only read.
    pub fn switch(&mut self, id: &str) -> Result<profile::Scene, String> {
        if !self.exists(id) {
            return Err(format!("scene {id} not found"));
        }
        let scene = profile::Scene::load(&self.profiles.join(format!("{id}.sdProfile")))?;
        if let Err(e) = self.store.set_current(id) {
            eprintln!("scene {id}: state.json: {}", e.msg);
        }
        self.current = id.to_string();
        Ok(scene)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn fixture(name: &str) -> (PathBuf, PathBuf) {
        let root = std::env::temp_dir().join(format!("orcadeck-scenes-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (profiles, data) = (root.join("profiles"), root.join("data"));
        std::fs::create_dir_all(&data).unwrap();
        for id in ["A", "B", "C", "D"] {
            let dir = profiles.join(format!("{id}.sdProfile"));
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("manifest.json"), format!(r#"{{"Name":"{id}","DeviceUUID":"AKP153E","Actions":{{}}}}"#)).unwrap();
        }
        // C is not listed; D closes the order A -> B -> D.
        std::fs::write(data.join("state.json"), br#"{"order":["A","B","D"],"current":"A"}"#).unwrap();
        (profiles, data)
    }

    fn open(profiles: &Path, data: &Path, scene: &str) -> SceneSet {
        SceneSet::open(&profiles.join(format!("{scene}.sdProfile")), profiles, data, "AKP153E").unwrap()
    }

    #[test]
    fn targets_come_from_rotate_settings_only() {
        let action = |uuid: &str, settings| profile::Action {
            uuid: uuid.into(),
            name: String::new(),
            state: 0,
            states: Vec::new(),
            settings,
        };
        assert_eq!(target_of(&action("com.other", json!({}))), None);
        assert_eq!(target_of(&action(ROTATE, json!({}))), Some(SceneTarget::Next));
        assert_eq!(target_of(&action(ROTATE, json!({"ProfileUUID": ""}))), Some(SceneTarget::Next));
        assert_eq!(target_of(&action(ROTATE, json!({"ProfileUUID": "PreviousScene"}))), Some(SceneTarget::Previous));
        assert_eq!(target_of(&action(ROTATE, json!({"ProfileUUID": "ABC"}))), Some(SceneTarget::Id("ABC".into())));
        assert_eq!(target_of(&action(ROTATE, json!({"ProfileUUID": "ABC.sdProfile"}))), Some(SceneTarget::Id("ABC".into())));
    }

    #[test]
    fn next_and_previous_wrap_in_device_order() {
        let (profiles, data) = fixture("order");
        let mut set = open(&profiles, &data, "A");
        assert_eq!(set.resolve(&SceneTarget::Next).as_deref(), Some("B"));
        assert_eq!(set.resolve(&SceneTarget::Previous).as_deref(), Some("D"), "wraps to the last");
        set.switch("D").unwrap();
        assert_eq!(set.resolve(&SceneTarget::Next).as_deref(), Some("A"), "wraps to the first");
        assert_eq!(set.resolve(&SceneTarget::Previous).as_deref(), Some("B"));
    }

    #[test]
    fn explicit_ids_need_a_manifest_and_are_safe() {
        let (profiles, data) = fixture("explicit");
        let set = open(&profiles, &data, "A");
        assert_eq!(set.resolve(&SceneTarget::Id("C".into())).as_deref(), Some("C"), "not listed, still exists");
        assert_eq!(set.resolve(&SceneTarget::Id("../A".into())), None);
        assert_eq!(set.resolve(&SceneTarget::Id("nope".into())), None);
    }

    #[test]
    fn current_outside_order_starts_at_an_end() {
        let (profiles, data) = fixture("outside");
        let set = open(&profiles, &data, "C");
        assert_eq!(set.resolve(&SceneTarget::Next).as_deref(), Some("A"));
        assert_eq!(set.resolve(&SceneTarget::Previous).as_deref(), Some("D"));
    }

    #[test]
    fn switch_loads_and_persists_current() {
        let (profiles, data) = fixture("switch");
        let mut set = open(&profiles, &data, "A");
        assert_eq!(set.switch("B").unwrap().dir, profiles.join("B.sdProfile"));
        assert_eq!(set.current(), "B");
        let state: serde_json::Value = serde_json::from_slice(&std::fs::read(data.join("state.json")).unwrap()).unwrap();
        assert_eq!(state["current"], "B");
        assert_eq!(state["order"], json!(["A", "B", "D"]), "order is not rewritten");
        assert!(set.switch("nope").is_err());
    }
}
