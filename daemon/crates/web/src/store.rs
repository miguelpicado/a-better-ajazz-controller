//! Storage behind the editor API. `FsStore` works on plain folders, so tests point it at a temp dir and the
//! daemon points it at its own data root. Stream Dock's files (`legacy_config`, built-in plugins) are only read.
use serde::Serialize;
use serde_json::{json, Map, Value};
use std::{
    fs, io,
    path::{Path, PathBuf},
};

#[derive(Debug)]
pub struct Error {
    pub status: u16,
    pub msg: String,
}

impl Error {
    pub fn new(status: u16, msg: impl Into<String>) -> Self {
        Error { status, msg: msg.into() }
    }
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        let status = if e.kind() == io::ErrorKind::NotFound { 404 } else { 500 };
        Error::new(status, e.to_string())
    }
}

pub type Result<T> = std::result::Result<T, Error>;

pub struct SceneInfo {
    pub id: String,
    pub name: String,
    /// `DeviceUUID` of the manifest (device model key, e.g. `AKP153E`).
    pub device: String,
    pub keys: usize,
    /// In the device's ordered scene list (shown on the device).
    pub listed: bool,
    /// Scene currently shown on the device.
    pub current: bool,
}

pub struct Plugin {
    pub uuid: String,
    pub dir: PathBuf,
    /// `ok`, `needs-manifest` (encrypted without overlay) or `invalid` (unreadable manifest).
    pub status: &'static str,
    pub encrypted: bool,
    /// Effective manifest comes from `<data>/manifests/<uuid>.json`.
    pub overlay: bool,
    /// Lives in a read-only built-in plugin folder.
    pub builtin: bool,
    /// Effective manifest with `en.json` names applied; `Null` when there is none.
    pub manifest: Value,
}

pub trait Store: Send + Sync {
    /// Ordered: device list first (in device order), then the rest by name.
    fn scenes(&self) -> Result<Vec<SceneInfo>>;
    fn scene(&self, id: &str) -> Result<Value>;
    /// Creates or replaces the scene manifest. The caller validates; the store writes atomically.
    fn put_scene(&self, id: &str, doc: &Value) -> Result<()>;
    fn set_order(&self, ids: &[String]) -> Result<()>;
    fn set_current(&self, id: &str) -> Result<()>;
    fn plugins(&self) -> Result<Vec<Plugin>>;
    /// Image bytes + MIME. `scene` resolves bare names in `<scene>/Images/`; `plugin` resolves paths
    /// relative to that plugin's folder, probing extensions like Stream Dock does.
    fn image(&self, scene: Option<&str>, plugin: Option<&str>, img: &str) -> Option<(Vec<u8>, &'static str)>;
    fn device(&self) -> Result<Value>;
    fn set_device(&self, patch: &Value) -> Result<Value>;
}

/// Scene and plugin ids are used as folder names: allow only `[0-9A-Za-z._-]`, no `..`.
pub fn safe_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 128 && !id.contains("..")
        && id.bytes().all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
}

pub fn parse_json(bytes: &[u8]) -> Option<Value> {
    let s = String::from_utf8_lossy(bytes);
    serde_json::from_str(s.trim_start_matches('\u{feff}')).ok()
}

/// Stream Dock / Qt layout: 4-space indent, keys sorted (serde_json's map is a BTreeMap).
pub fn to_pretty(v: &Value) -> Vec<u8> {
    let mut buf = Vec::new();
    let mut ser = serde_json::Serializer::with_formatter(&mut buf, serde_json::ser::PrettyFormatter::with_indent(b"    "));
    v.serialize(&mut ser).expect("Value always serializes");
    buf
}

/// Write via `<file>.tmp` + rename. With `backup`, the previous content is kept as `<file>.orcadeck.bak`.
pub fn atomic_write(path: &Path, bytes: &[u8], backup: bool) -> io::Result<()> {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("file");
    let tmp = path.with_file_name(format!("{name}.tmp"));
    {
        let mut f = fs::File::create(&tmp)?;
        io::Write::write_all(&mut f, bytes)?;
        f.sync_all()?;
    }
    if backup && path.is_file() {
        fs::copy(path, path.with_file_name(format!("{name}.orcadeck.bak")))?;
    }
    fs::rename(&tmp, path)
}

pub struct FsStore {
    /// `<id>.sdProfile/manifest.json` folders. Written by `put_scene`.
    pub profiles: PathBuf,
    /// Installed plugins (`<uuid>.sdPlugin`). Read only.
    pub plugins: PathBuf,
    /// Extra read-only plugin folders, e.g. Stream Dock's `<install>\plugins` built-ins.
    pub builtin_plugins: Vec<PathBuf>,
    /// Own state: `state.json` (order, current, brightness) and `manifests/<uuid>.json` overlays.
    pub data: PathBuf,
    /// Stream Dock `config` folder, read only: seeds order (DataCache.db) and current scene/brightness (ini)
    /// until `state.json` has its own values.
    pub legacy_config: Option<PathBuf>,
}

struct Legacy {
    model: String,
    serial: String,
    ini: Vec<(String, String, String)>,
}

impl Legacy {
    fn get(&self, section: &str, key: &str) -> Option<&str> {
        self.ini.iter().find(|(s, k, _)| s == section && k == key).map(|(_, _, v)| v.as_str())
    }
}

fn parse_ini(txt: &str) -> Vec<(String, String, String)> {
    let mut sec = String::new();
    let mut out = vec![];
    for l in txt.lines().map(str::trim) {
        if let Some(s) = l.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            sec = s.to_string();
        } else if let Some((k, v)) = l.split_once('=') {
            out.push((sec.clone(), k.trim().to_string(), v.trim().trim_matches('"').to_string()));
        }
    }
    out
}

const IMG_EXT: [&str; 7] = ["", ".png", "@2x.png", ".svg", ".gif", ".jpg", ".jpeg"];

fn mime(p: &Path) -> &'static str {
    match p.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).as_deref() {
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("svg") => "image/svg+xml",
        Some("webp") => "image/webp",
        _ => "application/octet-stream",
    }
}

/// First existing file of `base/rel` + an extension from `IMG_EXT`, staying inside `base`.
fn probe(base: &Path, rel: &str) -> Option<PathBuf> {
    let rel = rel.replace('\\', "/");
    if rel.is_empty() || rel.starts_with('/') || rel.contains(':') || rel.split('/').any(|s| s == "..") {
        return None;
    }
    let root = base.canonicalize().ok()?;
    IMG_EXT.iter().map(|e| base.join(format!("{rel}{e}"))).find(|p| p.is_file())
        .and_then(|p| p.canonicalize().ok()).filter(|p| p.starts_with(&root))
}

/// Applies `en.json` (Elgato locale layout: top-level `Name`/`Description`/`Category`, per-action objects).
fn localize(m: &mut Value, dir: &Path) {
    let Some(Value::Object(loc)) = fs::read(dir.join("en.json")).ok().and_then(|b| parse_json(&b)) else { return };
    for k in ["Name", "Description", "Category"] {
        if let Some(v @ Value::String(_)) = loc.get(k) {
            m[k] = v.clone();
        }
    }
    if let Some(Value::Array(actions)) = m.get_mut("Actions") {
        for a in actions {
            let Some(l) = a.get("UUID").and_then(Value::as_str).and_then(|u| loc.get(u)).cloned() else { continue };
            for k in ["Name", "Tooltip"] {
                if let Some(v @ Value::String(_)) = l.get(k) {
                    a[k] = v.clone();
                }
            }
        }
    }
}

impl FsStore {
    fn state(&self) -> Map<String, Value> {
        match fs::read(self.data.join("state.json")).ok().and_then(|b| parse_json(&b)) {
            Some(Value::Object(m)) => m,
            _ => Map::new(),
        }
    }

    fn save_state(&self, m: Map<String, Value>) -> Result<()> {
        fs::create_dir_all(&self.data)?;
        Ok(atomic_write(&self.data.join("state.json"), &to_pretty(&Value::Object(m)), false)?)
    }

    fn legacy(&self) -> Option<Legacy> {
        let dir = self.legacy_config.as_ref()?;
        let ini = parse_ini(&String::from_utf8_lossy(&fs::read(dir.join("StreamDockConfig.ini")).ok()?));
        let mut l = Legacy { model: "AKP153E".into(), serial: String::new(), ini };
        if let Some(m) = l.get("StreamDockCurrentDevice", "Device") {
            l.model = m.to_string();
        }
        l.serial = l.get("StreamDockCurrentDevice", &format!("{}\\SerialNumber", l.model)).unwrap_or("").to_string();
        Some(l)
    }

    fn legacy_order(&self) -> Vec<String> {
        let (Some(dir), Some(l)) = (&self.legacy_config, self.legacy()) else { return vec![] };
        fs::read(dir.join("DataCache.db")).ok()
            .and_then(|db| crate::sqlite::text_column(&db, &l.model, 0))
            .unwrap_or_default().into_iter()
            .map(|p| p.trim_end_matches(".sdProfile").to_string()).collect()
    }

    fn legacy_current(&self) -> Option<String> {
        let l = self.legacy()?;
        let v = l.get(&format!("{}{}DeviceConfig", l.model, l.serial), "sdProfileName")
            .or_else(|| l.get(&format!("{}DeviceConfig", l.model), "sdProfileName"))?;
        Some(v.trim_end_matches(".sdProfile").to_string())
    }

    fn manifest_path(&self, id: &str) -> Result<PathBuf> {
        if !safe_id(id) {
            return Err(Error::new(400, "bad scene id"));
        }
        Ok(self.profiles.join(format!("{id}.sdProfile")).join("manifest.json"))
    }

    fn load_plugin(&self, dir: PathBuf, builtin: bool) -> Option<Plugin> {
        let uuid = dir.file_name()?.to_str()?.strip_suffix(".sdPlugin")?.to_string();
        let raw = fs::read(dir.join("manifest.json")).ok()?;
        let encrypted = raw.starts_with(b"ELGATO");
        let overlay = encrypted.then(|| fs::read(self.data.join("manifests").join(format!("{uuid}.json"))).ok()).flatten();
        let (status, mut manifest) = match (encrypted, overlay.as_deref().or((!encrypted).then_some(&raw[..]))) {
            (_, Some(b)) => parse_json(b).filter(Value::is_object).map_or(("invalid", Value::Null), |m| ("ok", m)),
            (_, None) => ("needs-manifest", Value::Null),
        };
        if manifest.is_object() {
            localize(&mut manifest, &dir);
        }
        Some(Plugin { uuid, dir, status, encrypted, overlay: overlay.is_some(), builtin, manifest })
    }
}

impl Store for FsStore {
    fn scenes(&self) -> Result<Vec<SceneInfo>> {
        let st = self.state();
        let order: Vec<String> = match st.get("order") {
            Some(Value::Array(a)) => a.iter().filter_map(|v| v.as_str().map(String::from)).collect(),
            _ => self.legacy_order(),
        };
        let mut out = vec![];
        for e in fs::read_dir(&self.profiles)?.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            let Some(id) = name.strip_suffix(".sdProfile") else { continue };
            let Some(m) = fs::read(e.path().join("manifest.json")).ok().and_then(|b| parse_json(&b)) else { continue };
            out.push(SceneInfo {
                id: id.to_string(),
                name: m.get("Name").and_then(Value::as_str).unwrap_or("").trim().to_string(),
                device: m.get("DeviceUUID").and_then(Value::as_str).unwrap_or("").to_string(),
                keys: m.get("Actions").and_then(Value::as_object).map_or(0, |a| a.len()),
                listed: order.iter().any(|o| o == id),
                current: false,
            });
        }
        let pos = |id: &str| order.iter().position(|o| o == id).unwrap_or(usize::MAX);
        out.sort_by(|a, b| pos(&a.id).cmp(&pos(&b.id)).then_with(|| a.name.cmp(&b.name)));
        let current = st.get("current").and_then(Value::as_str).map(String::from).or_else(|| self.legacy_current());
        let idx = current.and_then(|c| out.iter().position(|s| s.id == c)).or_else(|| out.iter().position(|s| s.listed));
        if let Some(i) = idx {
            out[i].current = true;
        }
        Ok(out)
    }

    fn scene(&self, id: &str) -> Result<Value> {
        let p = self.manifest_path(id)?;
        let b = fs::read(&p).map_err(|_| Error::new(404, "scene not found"))?;
        parse_json(&b).ok_or_else(|| Error::new(500, "scene manifest is not valid JSON"))
    }

    fn put_scene(&self, id: &str, doc: &Value) -> Result<()> {
        let p = self.manifest_path(id)?;
        fs::create_dir_all(p.parent().unwrap())?;
        Ok(atomic_write(&p, &to_pretty(doc), true)?)
    }

    fn set_order(&self, ids: &[String]) -> Result<()> {
        let mut st = self.state();
        st.insert("order".into(), json!(ids));
        self.save_state(st)
    }

    fn set_current(&self, id: &str) -> Result<()> {
        let mut st = self.state();
        st.insert("current".into(), json!(id));
        self.save_state(st)
    }

    fn plugins(&self) -> Result<Vec<Plugin>> {
        let mut out = vec![];
        let dirs = std::iter::once((&self.plugins, false)).chain(self.builtin_plugins.iter().map(|d| (d, true)));
        for (dir, builtin) in dirs {
            let Ok(rd) = fs::read_dir(dir) else { continue };
            out.extend(rd.flatten().filter_map(|e| self.load_plugin(e.path(), builtin)));
        }
        out.sort_by(|a, b| a.uuid.cmp(&b.uuid));
        Ok(out)
    }

    fn image(&self, scene: Option<&str>, plugin: Option<&str>, img: &str) -> Option<(Vec<u8>, &'static str)> {
        let bare = !img.contains(['/', '\\']);
        let in_scene = scene.filter(|s| bare && safe_id(s))
            .and_then(|s| probe(&self.profiles.join(format!("{s}.sdProfile")).join("Images"), img));
        let p = in_scene.or_else(|| {
            let uuid = plugin.filter(|p| safe_id(p))?;
            let base = std::iter::once(&self.plugins).chain(&self.builtin_plugins)
                .map(|d| d.join(format!("{uuid}.sdPlugin"))).find(|d| d.is_dir())?;
            probe(&base, img)
        })?;
        Some((fs::read(&p).ok()?, mime(&p)))
    }

    fn device(&self) -> Result<Value> {
        let st = self.state();
        let l = self.legacy();
        let (model, serial) = l.as_ref().map_or(("AKP153E".into(), String::new()), |l| (l.model.clone(), l.serial.clone()));
        let legacy_bright = l.as_ref().and_then(|l| {
            l.get("DeviceBrightness", &format!("{model}{serial}")).or_else(|| l.get("DeviceBrightness", &model))
                .and_then(|v| v.parse::<u64>().ok())
        });
        let brightness = st.get("brightness").and_then(Value::as_u64).or(legacy_bright).unwrap_or(100);
        // ponytail: no HID here; the daemon's own Store reports the live device.
        Ok(json!({ "connected": false, "model": model, "serial": serial, "brightness": brightness, "stub": true }))
    }

    fn set_device(&self, patch: &Value) -> Result<Value> {
        let mut st = self.state();
        if let Some(b) = patch.get("brightness") {
            let b = b.as_u64().filter(|b| *b <= 100).ok_or_else(|| Error::new(400, "brightness must be 0..100"))?;
            st.insert("brightness".into(), json!(b));
            self.save_state(st)?;
        }
        self.device()
    }
}
