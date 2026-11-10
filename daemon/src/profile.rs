//! Reads existing Stream Dock `.sdProfile\manifest.json` scenes unchanged.
use serde::Deserialize;
use std::{collections::BTreeMap, fs, path::{Path, PathBuf}};

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
    /// usually without extension. Plugins repaint keys at runtime; this is the placeholder.
    pub fn image_of(&self, a: &Action, plugins: &Path) -> Option<PathBuf> {
        let s = a.states.get(a.state).or_else(|| a.states.first())?;
        let img = s.image.as_str();
        if img.is_empty() {
            return None;
        }
        let bundled = self.dir.join("Images").join(img);
        if bundled.is_file() {
            return Some(bundled);
        }
        let plugin = fs::read_dir(plugins).ok()?.flatten().map(|e| e.path()).find(|p| {
            p.file_name().and_then(|n| n.to_str()).and_then(|n| n.strip_suffix(".sdPlugin"))
                .is_some_and(|stem| a.uuid.starts_with(&format!("{stem}.")))
        })?;
        ["", ".png", ".jpg", ".jpeg"].iter().map(|e| plugin.join(format!("{img}{e}"))).find(|p| p.is_file())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_real_shape() {
        let j = r#"{"Actions":{"0,0":{"UUID":"x.y","Name":"N","State":0,"States":[{"Image":"a.png"}]},"5,2":{"UUID":"z","States":[]},"bad":{}}}"#;
        let s = Scene::parse(Path::new("d"), j).unwrap();
        assert_eq!(s.actions.len(), 2);
        assert_eq!(s.actions[0].0, (0, 0));
        assert_eq!(s.image_of(&s.actions[1].1, Path::new("nope")), None);
    }
}
