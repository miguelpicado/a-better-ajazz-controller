//! Storage behind the editor API. `FsStore` works on plain folders, so tests point it at a temp dir and the
//! daemon points it at its own data root. Stream Dock's files (`legacy_config`, built-in plugins) are only read.
use orcadeck_plugman::{inspect_package, install_package, Confidence, InstallOptions};
use serde::Serialize;
use serde_json::{json, Map, Value};
use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

#[derive(Debug)]
pub struct Error {
    pub status: u16,
    pub msg: String,
}

impl Error {
    pub fn new(status: u16, msg: impl Into<String>) -> Self {
        Error {
            status,
            msg: msg.into(),
        }
    }
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        let status = if e.kind() == io::ErrorKind::NotFound {
            404
        } else {
            500
        };
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
    /// `builtin`, `streamdock` (already there before this editor) or `orcadeck` (installed here).
    pub origin: &'static str,
    /// Installed through this editor, so it is the only kind this API may delete.
    pub removable: bool,
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
    fn image(
        &self,
        scene: Option<&str>,
        plugin: Option<&str>,
        img: &str,
    ) -> Option<(Vec<u8>, &'static str)>;
    fn device(&self) -> Result<Value>;
    fn set_device(&self, patch: &Value) -> Result<Value>;
    /// What a package in the inbox folder contains. Read-only: nothing is written.
    fn plugin_inspect(&self, path: &str) -> Result<Value>;
    /// Installs a package from the inbox folder. `force` replaces an existing install, keeping a backup.
    fn plugin_install(&self, path: &str, force: bool) -> Result<Value>;
    /// What a package staged from the shell contains. Token-addressed, never path-addressed.
    fn plugin_staged(&self, token: &str) -> Result<Value>;
    /// Installs a staged package; the token is spent only when the install succeeds.
    fn plugin_install_staged(&self, token: &str, force: bool) -> Result<Value>;
    /// Deletes a plugin, only when this editor installed it.
    fn plugin_remove(&self, uuid: &str) -> Result<Value>;
    /// One-shot scan of the inbox folder, newest first. Never runs in the background.
    fn plugin_inbox(&self) -> Result<Value>;
    /// Folder of an installed plugin, without scanning every manifest. Used to serve its inspector.
    fn plugin_dir(&self, uuid: &str) -> Option<PathBuf>;
    /// Live plugin host, when one is attached. `None` means the inspector endpoint answers 501.
    fn inspector(&self) -> Option<&dyn crate::InspectorBridge> {
        None
    }
}

/// Scene and plugin ids are used as folder names: allow only `[0-9A-Za-z._-]`, no `..`.
pub fn safe_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && !id.contains("..")
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
}

pub fn parse_json(bytes: &[u8]) -> Option<Value> {
    let s = String::from_utf8_lossy(bytes);
    serde_json::from_str(s.trim_start_matches('\u{feff}')).ok()
}

/// Keys sorted at every level. Explicit because another crate in the workspace enables serde_json's
/// `preserve_order`, which makes the map keep insertion order instead of sorting.
fn sorted(v: &Value) -> Value {
    match v {
        Value::Object(m) => {
            let mut keys: Vec<&String> = m.keys().collect();
            keys.sort();
            Value::Object(
                keys.into_iter()
                    .map(|k| (k.clone(), sorted(&m[k])))
                    .collect(),
            )
        }
        Value::Array(a) => Value::Array(a.iter().map(sorted).collect()),
        _ => v.clone(),
    }
}

/// Stream Dock / Qt layout: 4-space indent, keys sorted.
pub fn to_pretty(v: &Value) -> Vec<u8> {
    let mut buf = Vec::new();
    let mut ser = serde_json::Serializer::with_formatter(
        &mut buf,
        serde_json::ser::PrettyFormatter::with_indent(b"    "),
    );
    sorted(v)
        .serialize(&mut ser)
        .expect("Value always serializes");
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
    /// Installed plugins (`<uuid>.sdPlugin`). Ours (and Stream Dock's) installs land here.
    pub plugins: PathBuf,
    /// Extra read-only plugin folders, e.g. Stream Dock's `<install>\plugins` built-ins.
    pub builtin_plugins: Vec<PathBuf>,
    /// Own state: `state.json` (order, current, brightness, installed ids) and `manifests/<uuid>.json`
    /// overlays.
    pub data: PathBuf,
    /// Stream Dock `config` folder, read only: seeds order (DataCache.db) and current scene/brightness (ini)
    /// until `state.json` has its own values.
    pub legacy_config: Option<PathBuf>,
    /// Folder scanned by `plugin_inbox`; `None` means `%USERPROFILE%\Downloads`.
    pub downloads: Option<PathBuf>,
    /// Live plugin host, when the daemon is running; `None` in the standalone editor.
    pub inspector: Option<Arc<dyn crate::InspectorBridge>>,
}

struct Legacy {
    model: String,
    serial: String,
    ini: Vec<(String, String, String)>,
}

impl Legacy {
    fn get(&self, section: &str, key: &str) -> Option<&str> {
        self.ini
            .iter()
            .find(|(s, k, _)| s == section && k == key)
            .map(|(_, _, v)| v.as_str())
    }
}

fn parse_ini(txt: &str) -> Vec<(String, String, String)> {
    let mut sec = String::new();
    let mut out = vec![];
    for l in txt.lines().map(str::trim) {
        if let Some(s) = l.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            sec = s.to_string();
        } else if let Some((k, v)) = l.split_once('=') {
            out.push((
                sec.clone(),
                k.trim().to_string(),
                v.trim().trim_matches('"').to_string(),
            ));
        }
    }
    out
}

const IMG_EXT: [&str; 7] = ["", ".png", "@2x.png", ".svg", ".gif", ".jpg", ".jpeg"];

fn mime(p: &Path) -> &'static str {
    match p
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .as_deref()
    {
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
    if rel.is_empty()
        || rel.starts_with('/')
        || rel.contains(':')
        || rel.split('/').any(|s| s == "..")
    {
        return None;
    }
    let root = base.canonicalize().ok()?;
    IMG_EXT
        .iter()
        .map(|e| base.join(format!("{rel}{e}")))
        .find(|p| p.is_file())
        .and_then(|p| p.canonicalize().ok())
        .filter(|p| p.starts_with(&root))
}

/// Applies `en.json` (Elgato locale layout: top-level `Name`/`Description`/`Category`, per-action objects).
fn localize(m: &mut Value, dir: &Path) {
    let Some(Value::Object(loc)) = fs::read(dir.join("en.json"))
        .ok()
        .and_then(|b| parse_json(&b))
    else {
        return;
    };
    for k in ["Name", "Description", "Category"] {
        if let Some(v @ Value::String(_)) = loc.get(k) {
            m[k] = v.clone();
        }
    }
    if let Some(Value::Array(actions)) = m.get_mut("Actions") {
        for a in actions {
            let Some(l) = a
                .get("UUID")
                .and_then(Value::as_str)
                .and_then(|u| loc.get(u))
                .cloned()
            else {
                continue;
            };
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
        match fs::read(self.data.join("state.json"))
            .ok()
            .and_then(|b| parse_json(&b))
        {
            Some(Value::Object(m)) => m,
            _ => Map::new(),
        }
    }

    fn save_state(&self, m: Map<String, Value>) -> Result<()> {
        fs::create_dir_all(&self.data)?;
        Ok(atomic_write(
            &self.data.join("state.json"),
            &to_pretty(&Value::Object(m)),
            false,
        )?)
    }

    fn legacy(&self) -> Option<Legacy> {
        let dir = self.legacy_config.as_ref()?;
        let ini = parse_ini(&String::from_utf8_lossy(
            &fs::read(dir.join("StreamDockConfig.ini")).ok()?,
        ));
        let mut l = Legacy {
            model: "AKP153E".into(),
            serial: String::new(),
            ini,
        };
        if let Some(m) = l.get("StreamDockCurrentDevice", "Device") {
            l.model = m.to_string();
        }
        l.serial = l
            .get(
                "StreamDockCurrentDevice",
                &format!("{}\\SerialNumber", l.model),
            )
            .unwrap_or("")
            .to_string();
        Some(l)
    }

    fn legacy_order(&self) -> Vec<String> {
        let (Some(dir), Some(l)) = (&self.legacy_config, self.legacy()) else {
            return vec![];
        };
        fs::read(dir.join("DataCache.db"))
            .ok()
            .and_then(|db| crate::sqlite::text_column(&db, &l.model, 0))
            .unwrap_or_default()
            .into_iter()
            .map(|p| p.trim_end_matches(".sdProfile").to_string())
            .collect()
    }

    fn legacy_current(&self) -> Option<String> {
        let l = self.legacy()?;
        let v = l
            .get(
                &format!("{}{}DeviceConfig", l.model, l.serial),
                "sdProfileName",
            )
            .or_else(|| l.get(&format!("{}DeviceConfig", l.model), "sdProfileName"))?;
        Some(v.trim_end_matches(".sdProfile").to_string())
    }

    fn manifest_path(&self, id: &str) -> Result<PathBuf> {
        if !safe_id(id) {
            return Err(Error::new(400, "bad scene id"));
        }
        Ok(self
            .profiles
            .join(format!("{id}.sdProfile"))
            .join("manifest.json"))
    }

    /// Plugin UUIDs installed through this editor, newest last. `state.json` is our own file; if it is lost,
    /// previously installed plugins simply become non-removable (the safe direction).
    fn installed_ids(&self) -> Vec<String> {
        match self.state().get("installed") {
            Some(Value::Array(a)) => a
                .iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect(),
            _ => Vec::new(),
        }
    }

    fn set_installed(&self, ids: &[String]) -> Result<()> {
        let mut st = self.state();
        st.insert("installed".into(), json!(ids));
        self.save_state(st)
    }

    fn downloads_dir(&self) -> Result<PathBuf> {
        if let Some(d) = &self.downloads {
            return Ok(d.clone());
        }
        std::env::var_os("USERPROFILE")
            .or_else(|| std::env::var_os("HOME"))
            .map(PathBuf::from)
            .map(|home| home.join("Downloads"))
            .ok_or_else(|| Error::new(404, "no se encontró la carpeta de Descargas"))
    }

    /// Canonical package path, required to stay inside the Downloads folder. `canonicalize` resolves `..`
    /// and symlinks, so anything that escapes the folder is refused before plugman sees it.
    fn package_in_downloads(&self, raw: &str) -> Result<PathBuf> {
        let path = Path::new(raw)
            .canonicalize()
            .map_err(|_| Error::new(404, format!("no existe: {raw}")))?;
        let root = self
            .downloads_dir()?
            .canonicalize()
            .map_err(|_| Error::new(404, "no se encontró la carpeta de Descargas"))?;
        if !path.starts_with(&root) {
            return Err(Error::new(400, "el paquete debe estar dentro de Descargas"));
        }
        match crate::staged::package_kind_error(&path) {
            Some(e) => Err(Error::new(400, e)),
            None => Ok(path),
        }
    }

    /// Read-only package summary. `pkg` is already canonical and validated.
    fn inspect_at(&self, pkg: &Path) -> Result<Value> {
        let rec = inspect_package(pkg).map_err(package_error)?;
        if !safe_id(&rec.uuid) {
            return Err(Error::new(
                400,
                "el paquete no tiene un UUID de plugin válido",
            ));
        }
        let text = |k: &str| {
            rec.manifest
                .as_ref()
                .and_then(|m| m.get(k))
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim()
                .to_string()
        };
        let installed = self.plugins.join(format!("{}.sdPlugin", rec.uuid)).is_dir();
        Ok(json!({
            "path": pkg.to_string_lossy(),
            "kind": if pkg.is_dir() { "folder" } else { "archive" },
            "uuid": rec.uuid, "name": text("Name"), "version": text("Version"),
            "author": text("Author"), "description": text("Description"),
            "actions": rec.action_count, "encrypted": rec.encrypted,
            "confidence": rec.confidence.as_str(),
            "needsManifest": matches!(rec.confidence, Confidence::Partial | Confidence::None),
            "installed": installed,
        }))
    }

    /// Copies `pkg` through the existing plugman path. The caller validated `pkg`.
    fn install_at(&self, pkg: &Path, force: bool) -> Result<Value> {
        // Read-only pass first: validate the derived UUID before writing anything.
        let rec = inspect_package(pkg).map_err(package_error)?;
        if !safe_id(&rec.uuid) {
            return Err(Error::new(
                400,
                "el paquete no tiene un UUID de plugin válido",
            ));
        }
        let replaced = self.plugins.join(format!("{}.sdPlugin", rec.uuid)).exists();
        if replaced && !force {
            return Err(Error::new(
                409,
                format!("{} ya está instalado; confirma para reemplazarlo", rec.uuid),
            ));
        }
        let opts = InstallOptions {
            plugins_root: self.plugins.clone(),
            data_root: Some(self.data.clone()),
            force,
            manifest_path: None,
        };
        let r = install_package(pkg, &opts).map_err(package_error)?;
        let mut ids = self.installed_ids();
        if !ids.iter().any(|i| i == &r.reconstruction.uuid) {
            ids.push(r.reconstruction.uuid.clone());
            self.set_installed(&ids)?;
        }
        Ok(json!({
            "uuid": r.reconstruction.uuid, "dir": r.dest.to_string_lossy(),
            "overlay": r.overlay.is_some(), "needsManifest": r.needs_manifest, "replaced": replaced,
        }))
    }

    fn load_plugin(&self, dir: PathBuf, mut builtin: bool) -> Option<Plugin> {
        let uuid = dir
            .file_name()?
            .to_str()?
            .strip_suffix(".sdPlugin")?
            .to_string();
        if uuid == "com.miguelpicado.orcadock" {
            builtin = true;
        }
        let raw = fs::read(dir.join("manifest.json")).ok()?;
        let encrypted = raw.starts_with(b"ELGATO");
        let overlay = encrypted
            .then(|| fs::read(self.data.join("manifests").join(format!("{uuid}.json"))).ok())
            .flatten();
        let (status, mut manifest) = match (
            encrypted,
            overlay.as_deref().or((!encrypted).then_some(&raw[..])),
        ) {
            (_, Some(b)) => parse_json(b)
                .filter(Value::is_object)
                .map_or(("invalid", Value::Null), |m| ("ok", m)),
            (_, None) => ("needs-manifest", Value::Null),
        };
        if manifest.is_object() {
            localize(&mut manifest, &dir);
        }
        Some(Plugin {
            uuid,
            dir,
            status,
            encrypted,
            overlay: overlay.is_some(),
            builtin,
            origin: if builtin { "builtin" } else { "streamdock" },
            removable: false,
            manifest,
        })
    }
}

impl Store for FsStore {
    fn scenes(&self) -> Result<Vec<SceneInfo>> {
        let st = self.state();
        let order: Vec<String> = match st.get("order") {
            Some(Value::Array(a)) => a
                .iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect(),
            _ => self.legacy_order(),
        };
        let mut out = vec![];
        for e in fs::read_dir(&self.profiles)?.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            let Some(id) = name.strip_suffix(".sdProfile") else {
                continue;
            };
            let Some(m) = fs::read(e.path().join("manifest.json"))
                .ok()
                .and_then(|b| parse_json(&b))
            else {
                continue;
            };
            out.push(SceneInfo {
                id: id.to_string(),
                name: m
                    .get("Name")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .trim()
                    .to_string(),
                device: m
                    .get("DeviceUUID")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                keys: m
                    .get("Actions")
                    .and_then(Value::as_object)
                    .map_or(0, |a| a.len()),
                listed: order.iter().any(|o| o == id),
                current: false,
            });
        }
        let pos = |id: &str| order.iter().position(|o| o == id).unwrap_or(usize::MAX);
        out.sort_by(|a, b| {
            pos(&a.id)
                .cmp(&pos(&b.id))
                .then_with(|| a.name.cmp(&b.name))
        });
        let current = st
            .get("current")
            .and_then(Value::as_str)
            .map(String::from)
            .or_else(|| self.legacy_current());
        let idx = current
            .and_then(|c| out.iter().position(|s| s.id == c))
            .or_else(|| out.iter().position(|s| s.listed));
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
        let installed = self.installed_ids();
        let mut out = vec![];
        let dirs = std::iter::once((&self.plugins, false))
            .chain(self.builtin_plugins.iter().map(|d| (d, true)));
        for (dir, builtin) in dirs {
            let Ok(rd) = fs::read_dir(dir) else { continue };
            out.extend(
                rd.flatten()
                    .filter_map(|e| self.load_plugin(e.path(), builtin)),
            );
        }
        for p in &mut out {
            if !p.builtin && installed.iter().any(|i| i == &p.uuid) {
                p.origin = "orcadeck";
                p.removable = true;
            }
        }
        out.sort_by(|a, b| a.uuid.cmp(&b.uuid));
        Ok(out)
    }

    fn image(
        &self,
        scene: Option<&str>,
        plugin: Option<&str>,
        img: &str,
    ) -> Option<(Vec<u8>, &'static str)> {
        let bare = !img.contains(['/', '\\']);
        let in_scene = scene.filter(|s| bare && safe_id(s)).and_then(|s| {
            probe(
                &self.profiles.join(format!("{s}.sdProfile")).join("Images"),
                img,
            )
        });
        let p = in_scene.or_else(|| {
            let uuid = plugin.filter(|p| safe_id(p))?;
            let base = std::iter::once(&self.plugins)
                .chain(&self.builtin_plugins)
                .map(|d| d.join(format!("{uuid}.sdPlugin")))
                .find(|d| d.is_dir())?;
            probe(&base, img)
        })?;
        Some((fs::read(&p).ok()?, mime(&p)))
    }

    fn device(&self) -> Result<Value> {
        let st = self.state();
        let l = self.legacy();
        let (model, serial) = l.as_ref().map_or(("AKP153E".into(), String::new()), |l| {
            (l.model.clone(), l.serial.clone())
        });
        let legacy_bright = l.as_ref().and_then(|l| {
            l.get("DeviceBrightness", &format!("{model}{serial}"))
                .or_else(|| l.get("DeviceBrightness", &model))
                .and_then(|v| v.parse::<u64>().ok())
        });
        let brightness = st
            .get("brightness")
            .and_then(Value::as_u64)
            .or(legacy_bright)
            .unwrap_or(100);
        // ponytail: no HID here; the daemon's own Store reports the live device.
        Ok(
            json!({ "connected": false, "model": model, "serial": serial, "brightness": brightness, "stub": true }),
        )
    }

    fn set_device(&self, patch: &Value) -> Result<Value> {
        let mut st = self.state();
        if let Some(b) = patch.get("brightness") {
            let b = b
                .as_u64()
                .filter(|b| *b <= 100)
                .ok_or_else(|| Error::new(400, "brightness must be 0..100"))?;
            st.insert("brightness".into(), json!(b));
            self.save_state(st)?;
        }
        self.device()
    }

    fn plugin_inspect(&self, path: &str) -> Result<Value> {
        let pkg = self.package_in_downloads(path)?;
        self.inspect_at(&pkg)
    }

    fn plugin_install(&self, path: &str, force: bool) -> Result<Value> {
        let pkg = self.package_in_downloads(path)?;
        self.install_at(&pkg, force)
    }

    fn plugin_staged(&self, token: &str) -> Result<Value> {
        let staged = crate::staged::peek(token)
            .ok_or_else(|| Error::new(404, "el enlace de instalación ya se usó o ha caducado"))?;
        let mut v = self.inspect_at(&staged.path)?;
        if let Value::Object(m) = &mut v {
            // Never hand the absolute path back to the page: only the token identifies this file.
            m.remove("path");
            m.insert("token".into(), json!(token));
            m.insert("file".into(), json!(staged.name));
        }
        Ok(v)
    }

    fn plugin_install_staged(&self, token: &str, force: bool) -> Result<Value> {
        let staged = crate::staged::peek(token)
            .ok_or_else(|| Error::new(404, "el enlace de instalación ya se usó o ha caducado"))?;
        // Spend the token only on success: a 409 (already installed) can be retried with force.
        let r = self.install_at(&staged.path, force)?;
        crate::staged::consume(token);
        Ok(r)
    }

    fn plugin_remove(&self, uuid: &str) -> Result<Value> {
        if !safe_id(uuid) {
            return Err(Error::new(400, "bad plugin id"));
        }
        let name = format!("{uuid}.sdPlugin");
        if self.builtin_plugins.iter().any(|d| d.join(&name).is_dir()) {
            return Err(Error::new(
                403,
                "los plugins integrados no se pueden quitar",
            ));
        }
        let mut ids = self.installed_ids();
        if !ids.iter().any(|i| i == uuid) {
            return Err(Error::new(
                403,
                "solo se pueden quitar plugins instalados por OrcaDeck",
            ));
        }
        let root = self
            .plugins
            .canonicalize()
            .map_err(|_| Error::new(404, "no existe la carpeta de plugins"))?;
        let target = self
            .plugins
            .join(&name)
            .canonicalize()
            .map_err(|_| Error::new(404, "el plugin ya no está instalado"))?;
        // Direct child of our plugins folder, after resolving links: nothing else is ever deleted.
        if target.parent() != Some(root.as_path()) {
            return Err(Error::new(
                400,
                "el plugin no está en la carpeta de plugins",
            ));
        }
        fs::remove_dir_all(&target)?;
        let overlay = self.data.join("manifests").join(format!("{uuid}.json"));
        if overlay.is_file() {
            fs::remove_file(overlay)?;
        }
        ids.retain(|i| i != uuid);
        self.set_installed(&ids)?;
        Ok(json!({ "uuid": uuid, "removed": true }))
    }

    fn plugin_inbox(&self) -> Result<Value> {
        const RECENT: u64 = 30 * 86_400;
        const MAX: usize = 20;
        let dir = self.downloads_dir()?;
        let rd = fs::read_dir(&dir)
            .map_err(|_| Error::new(404, format!("no se encontró {}", dir.display())))?;
        let cutoff = SystemTime::now()
            .checked_sub(Duration::from_secs(RECENT))
            .unwrap_or(UNIX_EPOCH);
        let mut items = Vec::new();
        for e in rd.flatten() {
            let path = e.path();
            let Ok(meta) = e.metadata() else { continue };
            let name = e.file_name().to_string_lossy().into_owned();
            let lower = name.to_ascii_lowercase();
            if meta.is_dir() {
                // A folder package is installable only if plugman will find its manifest.
                if !lower.ends_with(".sdplugin") || !path.join("manifest.json").is_file() {
                    continue;
                }
            } else if !meta.is_file()
                || !["streamdeckplugin", "zip"]
                    .iter()
                    .any(|x| lower.ends_with(&format!(".{x}")))
            {
                continue;
            }
            let Ok(modified) = meta.modified() else {
                continue;
            };
            if modified < cutoff {
                continue;
            }
            let ms = modified
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis();
            items.push((
                ms,
                json!({
                    "name": name, "path": path.to_string_lossy(),
                    "kind": if meta.is_dir() { "folder" } else { "archive" },
                    "size": meta.len(), "modified": ms,
                }),
            ));
        }
        items.sort_by_key(|i| std::cmp::Reverse(i.0));
        items.truncate(MAX);
        Ok(json!({
            "dir": dir.to_string_lossy(),
            "items": items.into_iter().map(|(_, v)| v).collect::<Vec<_>>(),
        }))
    }

    fn plugin_dir(&self, uuid: &str) -> Option<PathBuf> {
        if !safe_id(uuid) {
            return None;
        }
        let name = format!("{uuid}.sdPlugin");
        std::iter::once(&self.plugins)
            .chain(&self.builtin_plugins)
            .map(|root| root.join(&name))
            .find(|dir| dir.is_dir())
    }

    fn inspector(&self) -> Option<&dyn crate::InspectorBridge> {
        self.inspector.as_deref()
    }
}

/// `plugman` errors: a bad package is the caller's fault (400), not a server failure.
fn package_error(e: io::Error) -> Error {
    let status = match e.kind() {
        io::ErrorKind::NotFound => 404,
        io::ErrorKind::AlreadyExists => 409,
        io::ErrorKind::InvalidData | io::ErrorKind::InvalidInput => 400,
        _ => 500,
    };
    Error::new(status, e.to_string())
}
