//! Native Stream Deck plugin installer for Orca Deck.
//!
//! Rust port of `tools/plugin-install/install.mjs`; the contract is
//! `docs/design/plugin-install-handoff.md`.
//!
//! Installs a `.streamDeckPlugin` (zip) or `.sdPlugin` folder without the Elgato app.
//! Marketplace manifests that start with the `ELGATO` magic are never decrypted: the host
//! manifest is rebuilt from the plugin's own plaintext files and written as an overlay to
//! `<dataRoot>/manifests/<uuid>.json`. The plugin folder itself is never modified after it
//! is copied into `pluginsRoot`.

use serde_json::{json, Map, Value};
use std::collections::{BTreeSet, HashMap, VecDeque};
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const ELGATO_MAGIC: &[u8] = b"ELGATO";
const SKIP_DIRS: &[&str] = &["node_modules", ".git", "logs", "archive", "storecache"];
const MAX_CODE_FILE: u64 = 20 * 1024 * 1024;
/// Zip-bomb limits for packages from untrusted sources.
const MAX_ENTRIES: usize = 20_000;
const MAX_ENTRY_BYTES: u64 = 256 * 1024 * 1024;
const MAX_TOTAL_UNCOMPRESSED_BYTES: u64 = 512 * 1024 * 1024;
const MAX_COMPRESSION_RATIO: u64 = 200;
/// The ratio is only enforced once an entry is big enough for it to mean anything.
const RATIO_CHECK_ABOVE_BYTES: u64 = 1024 * 1024;
const IMAGE_EXTS: &[&str] = &["png", "jpg", "jpeg", "svg", "gif"];
const LOCALE_SKIP_EXTS: &[&str] = &["png", "jpg", "jpeg", "svg", "gif", "json", "js", "html", "css"];
const LITERAL_SKIP_EXTS: &[&str] = &[
    "png", "jpg", "jpeg", "svg", "gif", "json", "js", "html", "css", "exe", "dll", "pdb", "log",
    "sdplugin",
];
const CODE_CANDIDATES: &[&str] = &[
    "bin/plugin.js", "bin/plugin.mjs", "bin/plugin.cjs",
    "plugin.js", "plugin.mjs", "plugin.cjs",
    "index.html", "source/main.html", "main.html",
];
const ICON_ROOTS: &[&str] = &[
    "imgs/actions", "images/actions", "imgs", "images", "Images", "resources/actions",
    "resources/assets", "assets",
];

// ---------------------------------------------------------------------------------------------
// Public types

/// What `manifest.json` in a plugin folder is.
#[derive(Debug, Clone, PartialEq)]
pub enum ManifestState {
    Plain(Value),
    /// Starts with the `ELGATO` magic. Only the size is reported; the content is never parsed.
    Encrypted { size: u64 },
    Missing,
    Invalid { size: u64 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Confidence {
    /// Original manifest is plain JSON: use it as is.
    Plain,
    /// Encrypted, rebuilt with a `CodePath` and at least one action: overlay written.
    Full,
    /// Encrypted, only one of `CodePath`/actions found: no overlay, needs a manifest.
    Partial,
    /// Nothing usable: no overlay, needs a manifest.
    None,
    /// Vendor manifest supplied by the caller: written as the overlay.
    Provided,
}

impl Confidence {
    pub fn as_str(self) -> &'static str {
        match self {
            Confidence::Plain => "plain",
            Confidence::Full => "full",
            Confidence::Partial => "partial",
            Confidence::None => "none",
            Confidence::Provided => "provided",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Reconstruction {
    pub uuid: String,
    pub encrypted: bool,
    pub confidence: Confidence,
    pub manifest: Option<Value>,
    pub code_path: Option<String>,
    pub action_count: usize,
}

impl Reconstruction {
    fn new(uuid: String, encrypted: bool, confidence: Confidence, manifest: Option<Value>) -> Self {
        let field = |k: &str| manifest.as_ref().and_then(|m| m.get(k));
        let code_path = field("CodePath").and_then(Value::as_str).map(String::from);
        let action_count = field("Actions").and_then(Value::as_array).map_or(0, Vec::len);
        Reconstruction { uuid, encrypted, confidence, manifest, code_path, action_count }
    }

    pub fn to_json(&self) -> Value {
        json!({
            "uuid": self.uuid,
            "encrypted": self.encrypted,
            "confidence": self.confidence.as_str(),
            "manifest": self.manifest,
            "codePath": self.code_path,
            "actionCount": self.action_count,
        })
    }
}

#[derive(Debug, Clone, Default)]
pub struct InstallOptions {
    pub plugins_root: PathBuf,
    /// Defaults to the parent of `plugins_root`.
    pub data_root: Option<PathBuf>,
    /// Replace an existing install, keeping it as `<uuid>.sdPlugin.bak-<timestamp>`.
    pub force: bool,
    /// Plain vendor manifest to use instead of the reconstruction.
    pub manifest_path: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct InstallResult {
    pub dest: PathBuf,
    pub overlay: Option<PathBuf>,
    /// Previous overlay deleted because the new install did not produce one.
    pub removed_overlay: Option<PathBuf>,
    /// Encrypted (or unreadable) manifest that could not be rebuilt to `full`: do not launch.
    pub needs_manifest: bool,
    pub reconstruction: Reconstruction,
}

// ---------------------------------------------------------------------------------------------
// Manifest state and reconstruction

pub fn read_manifest_state(plugin_dir: &Path) -> ManifestState {
    let Ok(buf) = fs::read(plugin_dir.join("manifest.json")) else {
        return ManifestState::Missing;
    };
    let size = buf.len() as u64;
    // Detection only: the rest of an ELGATO file is never interpreted or decrypted.
    if buf.starts_with(ELGATO_MAGIC) {
        return ManifestState::Encrypted { size };
    }
    match parse_json_bytes(&buf) {
        Some(manifest) => ManifestState::Plain(manifest),
        None => ManifestState::Invalid { size },
    }
}

/// Rebuild the host manifest of a plugin folder from its plaintext files. Read-only.
pub fn reconstruct_manifest(plugin_dir: &Path) -> Reconstruction {
    let dir = std::path::absolute(plugin_dir).unwrap_or_else(|_| plugin_dir.to_path_buf());
    let uuid = uuid_from_dir(&dir);
    match read_manifest_state(&dir) {
        ManifestState::Plain(m) => return Reconstruction::new(uuid, false, Confidence::Plain, Some(m)),
        ManifestState::Missing | ManifestState::Invalid { .. } => {
            return Reconstruction::new(uuid, false, Confidence::None, None)
        }
        ManifestState::Encrypted { .. } => {}
    }

    let locale = load_locale(&dir);
    let code_path = find_code_path(&dir);
    let actions: Vec<Value> = collect_action_uuids(&dir, &uuid, &locale)
        .iter()
        .map(|action| build_action(&dir, &uuid, action, &locale))
        .collect();

    let mut m = Map::new();
    m.insert("UUID".into(), json!(uuid));
    m.insert("Name".into(), json!(text(locale.get("Name")).unwrap_or_else(|| humanize(last_segment(&uuid)))));
    for key in ["Description", "Author", "Category"] {
        if let Some(t) = text(locale.get(key)) {
            m.insert(key.into(), json!(t));
        }
    }
    m.insert("Version".into(), json!(package_version(&dir).unwrap_or_else(|| "1.0.0".into())));
    m.insert("SDKVersion".into(), json!(2));
    if let Some(cp) = &code_path {
        m.insert("CodePath".into(), json!(cp));
        if has_ext(cp, &["js", "mjs", "cjs"]) {
            m.insert("Nodejs".into(), json!({ "Version": "20" }));
        }
    }
    if let Some(icon) = find_plugin_icon(&dir) {
        m.insert("Icon".into(), json!(icon));
    }
    if let Some(pi) = find_top_level_pi(&dir) {
        m.insert("PropertyInspectorPath".into(), json!(pi));
    }
    let confidence = match (code_path.is_some(), !actions.is_empty()) {
        (true, true) => Confidence::Full,
        (false, false) => Confidence::None,
        _ => Confidence::Partial,
    };
    m.insert("Actions".into(), Value::Array(actions));
    Reconstruction::new(uuid, true, confidence, Some(Value::Object(m)))
}

fn build_action(dir: &Path, uuid: &str, action: &str, locale: &Map<String, Value>) -> Value {
    let meta = locale.get(action).and_then(Value::as_object);
    let meta_field = |k: &str| meta.and_then(|m| m.get(k));
    let short = if action == uuid {
        ""
    } else {
        action.strip_prefix(&format!("{uuid}.")).unwrap_or_else(|| last_segment(action))
    };
    let icon = if short.is_empty() { find_plugin_icon(dir) } else { find_icon(dir, short) };
    let pi = if short.is_empty() {
        find_property_inspector(dir, "launcher").or_else(|| find_top_level_pi(dir))
    } else {
        find_property_inspector(dir, short)
            .or_else(|| parent_short(short).and_then(|p| find_property_inspector(dir, p)))
    };
    let pi_name = pi
        .as_deref()
        .and_then(|p| Path::new(p).file_stem())
        .map(|s| humanize(&strip_pi_suffix(&s.to_string_lossy())))
        .filter(|s| !s.is_empty());
    let states: Vec<Value> = match meta_field("States").and_then(Value::as_array) {
        Some(a) if !a.is_empty() => {
            a.iter().map(|s| if s.is_object() { s.clone() } else { json!({}) }).collect()
        }
        _ => vec![json!({})],
    };

    let mut a = Map::new();
    a.insert("UUID".into(), json!(action));
    let fallback = || humanize(if short.is_empty() { last_segment(uuid) } else { short });
    a.insert("Name".into(), json!(text(meta_field("Name")).or(pi_name).unwrap_or_else(fallback)));
    if let Some(t) = text(meta_field("Tooltip")) {
        a.insert("Tooltip".into(), json!(t));
    }
    if let Some(i) = icon {
        a.insert("Icon".into(), json!(i));
    }
    if let Some(p) = pi {
        a.insert("PropertyInspectorPath".into(), json!(p));
    }
    a.insert("States".into(), Value::Array(states));
    let lower = short.to_lowercase();
    let knob = ["dial", "knob", "encoder"].iter().any(|w| lower.contains(w));
    a.insert("Controllers".into(), if knob { json!(["Keypad", "Knob"]) } else { json!(["Keypad"]) });
    Value::Object(a)
}

fn load_locale(dir: &Path) -> Map<String, Value> {
    for name in ["en.json", "en-US.json", "en_GB.json", "en-GB.json"] {
        let path = dir.join(name);
        if path.exists() {
            return as_object(parse_json(&path));
        }
    }
    for entry in entries(dir) {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !is_file(&entry) || !has_ext(&name, &["json"]) || name == "manifest.json" || name == "package.json" {
            continue;
        }
        let parsed = as_object(parse_json(&entry.path()));
        if parsed.keys().any(|k| k.contains('.')) {
            return parsed;
        }
    }
    Map::new()
}

fn collect_action_uuids(dir: &Path, uuid: &str, locale: &Map<String, Value>) -> BTreeSet<String> {
    let prefix = format!("{uuid}.");
    let mut found = BTreeSet::new();
    for (key, value) in locale {
        let Some(obj) = value.as_object() else { continue };
        if !key.starts_with(&prefix) || has_ext(key, LOCALE_SKIP_EXTS) {
            continue;
        }
        if ["Name", "Tooltip", "States"].iter().any(|f| obj.get(*f).is_some_and(truthy)) {
            found.insert(key.clone());
        }
    }
    found.extend(read_registry_actions(uuid));
    // Locale and the Stream Dock registry are authoritative when present; code literals are a
    // fallback for plugins neither of them has seen.
    if !found.is_empty() {
        return found;
    }
    let vendor: Vec<&str> = uuid.split('.').take(2).collect();
    let pattern = format!("{}.", vendor.join("."));
    for file in walk(dir) {
        let name = file.to_string_lossy();
        let binary = has_ext(&name, &["exe", "dll"]);
        if !binary && !has_ext(&name, &["js", "mjs", "cjs", "html", "htm"]) {
            continue;
        }
        if fs::metadata(&file).map_or(true, |m| m.len() > MAX_CODE_FILE) {
            continue;
        }
        let Ok(buf) = fs::read(&file) else { continue };
        scan_literals(&buf, pattern.as_bytes(), &mut found);
        if binary {
            // UTF-16LE view at even offsets, as Node's buf.toString('utf16le') decodes it.
            // Non-ASCII code units become a byte that never matches.
            let ascii: Vec<u8> = buf
                .chunks_exact(2)
                .map(|c| match u16::from_le_bytes([c[0], c[1]]) { u @ 0..=0x7f => u as u8, _ => 0xff })
                .collect();
            scan_literals(&ascii, pattern.as_bytes(), &mut found);
        }
    }
    found
}

/// Byte-level equivalent of the JS regex `/<vendor>\.[a-z0-9][a-z0-9._-]*/gi`, with the
/// trailing `[._-]+` trimmed and file-like values dropped.
fn scan_literals(buf: &[u8], pattern: &[u8], out: &mut BTreeSet<String>) {
    let n = pattern.len();
    let mut i = 0;
    while i + n < buf.len() {
        if !(buf[i..i + n].eq_ignore_ascii_case(pattern) && buf[i + n].is_ascii_alphanumeric()) {
            i += 1;
            continue;
        }
        let mut end = i + n + 1;
        while end < buf.len() && (buf[end].is_ascii_alphanumeric() || b"._-".contains(&buf[end])) {
            end += 1;
        }
        // Only ASCII bytes were accepted, so this is valid UTF-8.
        let value = String::from_utf8_lossy(&buf[i..end]);
        let value = value.trim_end_matches(['.', '_', '-']);
        if !has_ext(value, LITERAL_SKIP_EXTS) {
            out.insert(value.to_string());
        }
        i = end;
    }
}

/// Action UUIDs Stream Dock cached under `HKCU\Software\HotSpot\StreamDock\<uuid>.sdPlugin`.
/// Read-only; any failure (no key, not Windows, no reg.exe) yields an empty list.
fn read_registry_actions(uuid: &str) -> Vec<String> {
    if !cfg!(windows) {
        return Vec::new();
    }
    let mut cmd = std::process::Command::new("reg");
    cmd.args(["query", &format!(r"HKCU\Software\HotSpot\StreamDock\{uuid}.sdPlugin")])
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let Ok(out) = cmd.output() else { return Vec::new() };
    if !out.status.success() {
        return Vec::new();
    }
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            let b = line.as_bytes();
            // Value name is the text before the first run of 2+ whitespace characters.
            let cut = (1..b.len())
                .find(|&i| b[i].is_ascii_whitespace() && b[i - 1].is_ascii_whitespace())
                .map_or(b.len(), |i| i - 1);
            let name = &line[..cut];
            is_action_uuid(name).then(|| name.to_string())
        })
        .collect()
}

fn is_action_uuid(s: &str) -> bool {
    let parts: Vec<&str> = s.split('.').collect();
    parts.len() >= 3
        && !parts[0].is_empty()
        && parts[0].bytes().all(|c| c.is_ascii_alphanumeric())
        && parts[1..]
            .iter()
            .all(|p| !p.is_empty() && p.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-'))
}

fn find_code_path(dir: &Path) -> Option<String> {
    if let Some(c) = CODE_CANDIDATES.iter().find(|c| dir.join(c).exists()) {
        return Some(c.to_string());
    }
    let root = entries(dir);
    for e in &root {
        let name = e.file_name().to_string_lossy().into_owned();
        if is_file(e) && has_ext(&name, &["exe"]) {
            return Some(name);
        }
    }
    for e in &root {
        let name = e.file_name().to_string_lossy().into_owned();
        if !is_dir(e) || SKIP_DIRS.contains(&name.as_str()) {
            continue;
        }
        for c in ["main.html", "index.html", "plugin.js"] {
            if e.path().join(c).exists() {
                return Some(format!("{name}/{c}"));
            }
        }
    }
    None
}

fn find_icon(dir: &Path, short: &str) -> Option<String> {
    let wanted = squash(short);
    let last = squash(last_segment(short));
    for root in ICON_ROOTS {
        let base = dir.join(root);
        if !base.exists() {
            continue;
        }
        for file in walk(&base) {
            if !has_ext(&file.to_string_lossy(), IMAGE_EXTS) {
                continue;
            }
            let name = squash(&stem(&file));
            if name == wanted || name == last || name.starts_with(&wanted) || name.contains(&last) {
                let rel = relative(dir, &file);
                // Without extension: the host probes extensions itself.
                return Some(rel.rsplit_once('.').map_or(rel.clone(), |(head, _)| head.to_string()));
            }
        }
    }
    None
}

fn find_property_inspector(dir: &Path, short: &str) -> Option<String> {
    if short.is_empty() {
        return None;
    }
    let wanted = squash(short);
    let last = squash(last_segment(short));
    let roots = [
        "pi".to_string(), "propertyinspector".into(), "ui".into(),
        "source/actions".into(), "resources/actions".into(),
        format!("source/actions/{last}"), format!("resources/actions/{last}"),
    ];
    let mut best: Option<PathBuf> = None;
    let mut best_score = 0;
    for root in &roots {
        let base = dir.join(root);
        if !base.exists() {
            continue;
        }
        for file in walk(&base) {
            if !has_ext(&file.to_string_lossy(), &["html", "htm"]) {
                continue;
            }
            let name = squash(&stem(&file));
            let parent = squash(&file.parent().and_then(Path::file_name).unwrap_or_default().to_string_lossy());
            let mut score = if name == wanted || name == last {
                4
            } else if name.contains(&format!("action{wanted}")) || name.contains(&format!("action{last}")) {
                3
            } else if name.contains(&wanted) || name.contains(&last) {
                2
            } else {
                0
            };
            if parent == wanted || parent == last {
                score = score.max(3);
            } else if parent.contains(&wanted) || parent.contains(&last) {
                score = score.max(1);
            }
            if score > best_score {
                best = Some(file);
                best_score = score;
            }
        }
    }
    best.map(|f| relative(dir, &f))
}

fn find_top_level_pi(dir: &Path) -> Option<String> {
    ["pi/index.html", "propertyinspector/index.html", "ui/index.html", "inspector.html"]
        .into_iter()
        .find(|c| dir.join(c).exists())
        .map(String::from)
}

fn find_plugin_icon(dir: &Path) -> Option<String> {
    for c in ["imgs/plugin", "images/plugin", "imgs/pluginIcon", "images/pluginIcon", "Images/pluginIcon", "Images/icon"] {
        if dir.join(c).is_file() {
            return Some(c.into());
        }
        if dir.join(format!("{c}.png")).exists() {
            return Some(format!("{c}.png"));
        }
    }
    None
}

fn package_version(dir: &Path) -> Option<String> {
    let pkg = parse_json(&dir.join("package.json"))
        .filter(truthy)
        .or_else(|| parse_json(&dir.join("bin").join("package.json")))?;
    text(pkg.get("version"))
}

// ---------------------------------------------------------------------------------------------
// Packages: inspect and install

/// Reconstruct a plugin folder or a `.streamDeckPlugin` zip (extracted to a temp dir).
pub fn inspect_package(package: &Path) -> io::Result<Reconstruction> {
    let pkg = std::path::absolute(package)?;
    if pkg.is_dir() {
        return Ok(reconstruct_manifest(&pkg));
    }
    let tmp = TempDir::new("inspect")?;
    extract_package(&pkg, &tmp.0)?;
    Ok(reconstruct_manifest(&find_plugin_dir(&tmp.0)?))
}

pub fn install_package(package: &Path, opts: &InstallOptions) -> io::Result<InstallResult> {
    let pkg = std::path::absolute(package)?;
    let plugins = std::path::absolute(&opts.plugins_root)?;
    let data = match &opts.data_root {
        Some(d) => std::path::absolute(d)?,
        None => plugins.parent().unwrap_or(&plugins).to_path_buf(),
    };
    if !pkg.exists() {
        return Err(err(io::ErrorKind::NotFound, format!("Package not found: {}", pkg.display())));
    }
    let provided = opts.manifest_path.as_deref().map(read_overlay).transpose()?;

    let mut _tmp = None;
    let source = if pkg.is_dir() {
        pkg
    } else {
        let tmp = TempDir::new("install")?;
        extract_package(&pkg, &tmp.0)?;
        let dir = find_plugin_dir(&tmp.0)?;
        _tmp = Some(tmp); // removed on drop, after the copy
        dir
    };

    let uuid = uuid_from_dir(&source);
    if uuid.is_empty() {
        return Err(err(io::ErrorKind::InvalidInput, format!("Cannot derive a plugin UUID from {}", source.display())));
    }
    let dest = plugins.join(format!("{uuid}.sdPlugin"));
    if source == dest {
        return Err(err(io::ErrorKind::InvalidInput, "Source and destination are the same folder".into()));
    }
    fs::create_dir_all(&plugins)?;
    if dest.exists() {
        if !opts.force {
            return Err(err(
                io::ErrorKind::AlreadyExists,
                format!("{} already exists (use --force to replace)", dest.display()),
            ));
        }
        let mut bak = dest.clone().into_os_string();
        bak.push(format!(".bak-{}", timestamp()));
        fs::rename(&dest, bak)?;
    }
    copy_dir(&source, &dest)?;

    let rec = reconstruct_manifest(&dest);
    let overlay = data.join("manifests").join(format!("{uuid}.json"));
    if let Some(manifest) = provided {
        write_json(&overlay, &manifest)?;
        let reconstruction = Reconstruction::new(uuid, rec.encrypted, Confidence::Provided, Some(manifest));
        return Ok(InstallResult {
            dest,
            overlay: Some(overlay),
            removed_overlay: None,
            needs_manifest: false,
            reconstruction,
        });
    }
    let write = rec.encrypted && rec.confidence == Confidence::Full;
    if write {
        write_json(&overlay, rec.manifest.as_ref().expect("full reconstruction has a manifest"))?;
    }
    // An overlay from the previous install no longer describes this plugin.
    let removed_overlay = if write { None } else { remove_stale_overlay(&overlay)? };
    Ok(InstallResult {
        dest,
        overlay: write.then_some(overlay),
        removed_overlay,
        needs_manifest: matches!(rec.confidence, Confidence::Partial | Confidence::None),
        reconstruction: rec,
    })
}

/// Remove the overlay of a replaced install. Not finding one is fine.
fn remove_stale_overlay(path: &Path) -> io::Result<Option<PathBuf>> {
    match fs::remove_file(path) {
        Ok(()) => Ok(Some(path.to_path_buf())),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

/// Reject zip entry names that could escape the extraction folder: absolute paths, any `:`
/// (drive letters and NTFS alternate data streams) and `..` components.
pub fn check_entry_name(name: &str) -> io::Result<()> {
    let unsafe_name = name.starts_with(['/', '\\'])
        || name.contains(':')
        || name.split(['/', '\\']).any(|part| part == "..");
    if unsafe_name {
        return Err(err(io::ErrorKind::InvalidData, format!("Unsafe path in package: {name}")));
    }
    Ok(())
}

fn extract_package(pkg: &Path, dest: &Path) -> io::Result<()> {
    match extract_zip(pkg, dest) {
        Ok(()) => Ok(()),
        Err(e) => {
            // Callers pass a fresh temp dir; never leave a rejected package's partial output behind.
            let _ = fs::remove_dir_all(dest);
            Err(e)
        }
    }
}

fn extract_zip(pkg: &Path, dest: &Path) -> io::Result<()> {
    let mut zip = zip::ZipArchive::new(fs::File::open(pkg)?).map_err(io::Error::other)?;
    if zip.len() > MAX_ENTRIES {
        return Err(rejected(format!("{} entries (limit {MAX_ENTRIES})", zip.len())));
    }
    // Validate every entry before writing anything.
    for name in zip.file_names() {
        check_entry_name(&name.map_err(io::Error::other)?)?;
    }
    let mut buf = [0u8; 64 * 1024];
    let mut total = 0u64;
    for i in 0..zip.len() {
        let mut file = zip.by_index(i).map_err(io::Error::other)?;
        let name = file.name().map_err(io::Error::other)?.into_owned();
        let out = dest.join(&name);
        if file.is_dir() {
            fs::create_dir_all(&out)?;
            continue;
        }
        if let Some(parent) = out.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut writer = fs::File::create(&out)?;
        let mut entry = 0u64;
        // Count what is actually written: the sizes declared in the zip header are not trusted.
        loop {
            let n = file.read(&mut buf).map_err(io::Error::other)?;
            if n == 0 {
                break;
            }
            entry += n as u64;
            total += n as u64;
            if entry > MAX_ENTRY_BYTES {
                return Err(rejected(format!("{name} is over {MAX_ENTRY_BYTES} bytes")));
            }
            if total > MAX_TOTAL_UNCOMPRESSED_BYTES {
                return Err(rejected(format!("over {MAX_TOTAL_UNCOMPRESSED_BYTES} bytes uncompressed")));
            }
            if entry > RATIO_CHECK_ABOVE_BYTES && entry > file.compressed_size().saturating_mul(MAX_COMPRESSION_RATIO) {
                return Err(rejected(format!("{name} expands over {MAX_COMPRESSION_RATIO}:1")));
            }
            writer.write_all(&buf[..n])?;
        }
    }
    Ok(())
}

fn rejected(reason: String) -> io::Error {
    err(io::ErrorKind::InvalidData, format!("Package rejected: {reason}"))
}

fn find_plugin_dir(root: &Path) -> io::Result<PathBuf> {
    let mut queue = VecDeque::from([root.to_path_buf()]);
    while let Some(current) = queue.pop_front() {
        for e in entries(&current) {
            if !is_dir(&e) {
                continue;
            }
            let path = e.path();
            if has_ext(&e.file_name().to_string_lossy(), &["sdplugin"]) && path.join("manifest.json").exists() {
                return Ok(path);
            }
            queue.push_back(path);
        }
    }
    Err(err(
        io::ErrorKind::NotFound,
        format!("No .sdPlugin folder with manifest.json found under {}", root.display()),
    ))
}

// ---------------------------------------------------------------------------------------------
// Watch an inbox folder

pub type WatchEvent = (PathBuf, io::Result<InstallResult>);

/// Polling watcher for `*.streamDeckPlugin` files. Drive it from your own loop with
/// [`Watcher::poll`] every ~500 ms, or use [`watch_packages`].
pub struct Watcher {
    inbox: PathBuf,
    opts: InstallOptions,
    /// Size and mtime at the last install attempt (success or failure): retried only on change.
    handled: HashMap<PathBuf, (u64, Option<SystemTime>)>,
    /// Last seen size and how many consecutive polls it stayed the same.
    pending: HashMap<PathBuf, (u64, u32)>,
}

impl Watcher {
    /// Installs always use `force`, so a re-download updates the plugin and leaves a backup.
    pub fn new(inbox: &Path, mut opts: InstallOptions) -> io::Result<Self> {
        let inbox = std::path::absolute(inbox)?;
        if !inbox.is_dir() {
            return Err(err(io::ErrorKind::NotFound, format!("Watch folder not found: {}", inbox.display())));
        }
        opts.force = true;
        Ok(Watcher { inbox, opts, handled: HashMap::new(), pending: HashMap::new() })
    }

    /// Install every new or changed package now, without waiting for it to settle.
    pub fn scan(&mut self) -> Vec<WatchEvent> {
        self.step(0)
    }

    /// Install new or changed packages whose size held still for two consecutive polls.
    pub fn poll(&mut self) -> Vec<WatchEvent> {
        self.step(2)
    }

    fn step(&mut self, stable_polls: u32) -> Vec<WatchEvent> {
        let mut events = Vec::new();
        for e in entries(&self.inbox) {
            if !is_file(&e) || !has_ext(&e.file_name().to_string_lossy(), &["streamdeckplugin"]) {
                continue;
            }
            let Ok(meta) = e.metadata() else { continue };
            let (path, size, mtime) = (e.path(), meta.len(), meta.modified().ok());
            if self.handled.get(&path) == Some(&(size, mtime)) {
                continue;
            }
            let stable = match self.pending.get(&path) {
                Some(&(last, n)) if last == size => n + 1,
                _ => 0,
            };
            if stable < stable_polls {
                self.pending.insert(path, (size, stable));
                continue;
            }
            self.pending.remove(&path);
            self.handled.insert(path.clone(), (size, mtime));
            let result = install_package(&path, &self.opts);
            events.push((path, result));
        }
        events
    }
}

/// Install what is already in `inbox`, then (unless `once`) poll it forever.
pub fn watch_packages(inbox: &Path, opts: InstallOptions, once: bool) -> io::Result<()> {
    let mut watcher = Watcher::new(inbox, opts)?;
    report(watcher.scan());
    if once {
        return Ok(());
    }
    println!("Watching {} for .streamDeckPlugin files...", watcher.inbox.display());
    loop {
        std::thread::sleep(Duration::from_millis(500));
        report(watcher.poll());
    }
}

fn report(events: Vec<WatchEvent>) {
    for (path, result) in events {
        match result {
            Ok(r) => {
                println!("Installed {} -> {}", r.reconstruction.uuid, r.dest.display());
                if let Some(o) = &r.overlay {
                    println!("  manifest overlay: {} ({})", o.display(), r.reconstruction.confidence.as_str());
                } else if r.needs_manifest {
                    println!("  encrypted manifest needs --manifest <json>");
                }
                if let Some(o) = &r.removed_overlay {
                    println!("  removed stale manifest overlay: {}", o.display());
                }
            }
            Err(e) => eprintln!("{}: {e}", path.display()),
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Overlay export/import

pub fn export_overlays(data_root: &Path, out_dir: &Path) -> io::Result<usize> {
    let src = data_root.join("manifests");
    if !src.is_dir() {
        return Err(err(io::ErrorKind::NotFound, format!("No overlays found in {}", src.display())));
    }
    fs::create_dir_all(out_dir)?;
    let files = json_files(&src);
    for f in &files {
        fs::copy(f, out_dir.join(f.file_name().unwrap_or_default()))?;
    }
    Ok(files.len())
}

/// Copy overlays into `<dataRoot>/manifests`. Every file is validated before any is copied.
pub fn import_overlays(in_dir: &Path, data_root: &Path) -> io::Result<usize> {
    if !in_dir.is_dir() {
        return Err(err(io::ErrorKind::NotFound, format!("Overlay folder not found: {}", in_dir.display())));
    }
    let files = json_files(in_dir);
    for f in &files {
        read_overlay(f)?;
    }
    let dest = data_root.join("manifests");
    fs::create_dir_all(&dest)?;
    for f in &files {
        fs::copy(f, dest.join(f.file_name().unwrap_or_default()))?;
    }
    Ok(files.len())
}

/// Parse an overlay or vendor manifest; it must have a `UUID` and an `Actions` array.
fn read_overlay(path: &Path) -> io::Result<Value> {
    let invalid = || err(io::ErrorKind::InvalidData, format!("Invalid overlay: {}", path.display()));
    let manifest = parse_json_bytes(&fs::read(path)?).ok_or_else(invalid)?;
    if !manifest.get("UUID").is_some_and(truthy) || !manifest.get("Actions").is_some_and(Value::is_array) {
        return Err(invalid());
    }
    Ok(manifest)
}

fn json_files(dir: &Path) -> Vec<PathBuf> {
    entries(dir)
        .into_iter()
        .filter(|e| is_file(e) && has_ext(&e.file_name().to_string_lossy(), &["json"]))
        .map(|e| e.path())
        .collect()
}

// ---------------------------------------------------------------------------------------------
// Small helpers

fn err(kind: io::ErrorKind, msg: String) -> io::Error {
    io::Error::new(kind, msg)
}

fn parse_json_bytes(buf: &[u8]) -> Option<Value> {
    let s = String::from_utf8_lossy(buf);
    serde_json::from_str(s.strip_prefix('\u{feff}').unwrap_or(&s)).ok()
}

fn parse_json(path: &Path) -> Option<Value> {
    parse_json_bytes(&fs::read(path).ok()?)
}

fn as_object(v: Option<Value>) -> Map<String, Value> {
    match v {
        Some(Value::Object(m)) => m,
        _ => Map::new(),
    }
}

fn write_json(path: &Path, value: &Value) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut s = serde_json::to_string_pretty(value).map_err(io::Error::other)?;
    s.push('\n');
    fs::write(path, s)
}

/// JavaScript truthiness, for parity with the reference implementation.
fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|f| f != 0.0),
        Value::String(s) => !s.is_empty(),
        Value::Array(_) | Value::Object(_) => true,
    }
}

/// Non-empty string value.
fn text(v: Option<&Value>) -> Option<String> {
    v.and_then(Value::as_str).filter(|s| !s.is_empty()).map(String::from)
}

/// `camelCase.with-dots` -> `Camel Case With Dots`, matching the JS `humanize`.
fn humanize(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut spaced = String::new();
    for (i, &c) in chars.iter().enumerate() {
        if i > 0 && c.is_ascii_uppercase() && (chars[i - 1].is_ascii_lowercase() || chars[i - 1].is_ascii_digit()) {
            spaced.push(' ');
        }
        spaced.push(c);
    }
    let mut out = String::new();
    let (mut in_sep, mut prev_word) = (false, false);
    for c in spaced.chars() {
        if matches!(c, '.' | '_' | '-') {
            if !in_sep {
                out.push(' ');
            }
            in_sep = true;
            prev_word = false;
            continue;
        }
        in_sep = false;
        let word = c.is_ascii_alphanumeric();
        out.push(if word && !prev_word { c.to_ascii_uppercase() } else { c });
        prev_word = word;
    }
    out
}

/// Drop a trailing `inspector`/`pi` (any case) and the separators before it.
fn strip_pi_suffix(s: &str) -> String {
    let lower = s.to_ascii_lowercase();
    let cut = if lower.ends_with("inspector") {
        s.len() - 9
    } else if lower.ends_with("pi") {
        s.len() - 2
    } else {
        return s.to_string();
    };
    s[..cut].trim_end_matches(['.', '_', '-']).to_string()
}

fn squash(s: &str) -> String {
    s.chars().filter(|c| !matches!(c, '.' | '_' | '-')).collect::<String>().to_lowercase()
}

fn last_segment(s: &str) -> &str {
    s.rsplit('.').next().unwrap_or(s)
}

fn parent_short(short: &str) -> Option<&str> {
    short.rfind('.').filter(|&i| i > 0).map(|i| &short[..i])
}

fn has_ext(name: &str, exts: &[&str]) -> bool {
    let lower = name.to_ascii_lowercase();
    exts.iter().any(|e| lower.len() > e.len() && lower.ends_with(e) && lower.as_bytes()[lower.len() - e.len() - 1] == b'.')
}

fn stem(path: &Path) -> String {
    path.file_stem().unwrap_or_default().to_string_lossy().into_owned()
}

fn uuid_from_dir(dir: &Path) -> String {
    let name = dir.file_name().unwrap_or_default().to_string_lossy().into_owned();
    if has_ext(&name, &["sdplugin"]) {
        name[..name.len() - ".sdPlugin".len()].to_string()
    } else {
        name
    }
}

/// `file` relative to `dir`, with `/` separators.
fn relative(dir: &Path, file: &Path) -> String {
    let rel = file.strip_prefix(dir).unwrap_or(file);
    rel.components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/")
}

/// Directory entries in OS order (same order Node's readdirSync sees on Windows); empty on error.
fn entries(dir: &Path) -> Vec<fs::DirEntry> {
    fs::read_dir(dir).map(|rd| rd.flatten().collect()).unwrap_or_default()
}

fn is_file(e: &fs::DirEntry) -> bool {
    e.file_type().is_ok_and(|t| t.is_file())
}

fn is_dir(e: &fs::DirEntry) -> bool {
    e.file_type().is_ok_and(|t| t.is_dir())
}

/// All regular files under `dir`, depth first, skipping `SKIP_DIRS`.
fn walk(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for e in entries(dir) {
        if is_dir(&e) {
            if !SKIP_DIRS.contains(&e.file_name().to_string_lossy().as_ref()) {
                out.extend(walk(&e.path()));
            }
        } else if is_file(&e) {
            out.push(e.path());
        }
    }
    out
}

fn copy_dir(src: &Path, dst: &Path) -> io::Result<()> {
    fs::create_dir_all(dst)?;
    for e in fs::read_dir(src)? {
        let e = e?;
        let to = dst.join(e.file_name());
        if e.file_type()?.is_dir() {
            copy_dir(&e.path(), &to)?;
        } else {
            fs::copy(e.path(), to)?;
        }
    }
    Ok(())
}

/// UTC time like JS `toISOString()` with `:` and `.` replaced by `-`: `2026-11-09T12-34-56-789Z`.
fn timestamp() -> String {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    let secs = now.as_secs();
    let (days, rem) = ((secs / 86_400) as i64, secs % 86_400);
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}-{:02}-{:02}-{:03}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60,
        now.subsec_millis()
    )
}

/// Temp folder removed on drop.
struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> io::Result<Self> {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().subsec_nanos();
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("orca-sd-{tag}-{}-{nanos}-{n}", std::process::id()));
        fs::create_dir(&path)?;
        Ok(TempDir(path))
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn helpers_match_js() {
        assert_eq!(humanize("selectWebcam"), "Select Webcam");
        assert_eq!(humanize("open_url.v2"), "Open Url V2");
        assert_eq!(humanize("x--y"), "X Y");
        assert_eq!(strip_pi_suffix("Launcher_PI"), "Launcher");
        assert_eq!(strip_pi_suffix("volume.inspector"), "volume");
        assert!(has_ext("A.JPEG", IMAGE_EXTS) && !has_ext("png", IMAGE_EXTS) && !has_ext("a.mjs", &["js"]));
        assert!(is_action_uuid("com.vivremotion.windowscockpit.plugin") && !is_action_uuid("HKEY_CURRENT_USER\\x"));
        let t = timestamp();
        assert_eq!(t.len(), 24, "{t}");
        assert!(t.ends_with('Z') && !t.contains(':'));
    }

    #[test]
    fn a_rejected_package_leaves_no_partial_extraction() {
        let src = TempDir::new("bomb-src").unwrap();
        let zip_path = src.0.join("bomb.zip");
        let mut zip = zip::ZipWriter::new(fs::File::create(&zip_path).unwrap());
        let o = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        zip.start_file("com.example.demo.sdPlugin/manifest.json", o).unwrap();
        zip.write_all(b"{}").unwrap();
        zip.start_file("com.example.demo.sdPlugin/bomb.bin", o).unwrap();
        zip.write_all(&vec![0u8; 4 * 1024 * 1024]).unwrap();
        zip.finish().unwrap();

        let dest = TempDir::new("bomb-dst").unwrap();
        let e = extract_package(&zip_path, &dest.0).unwrap_err();
        assert!(e.to_string().contains("expands over"), "{e}");
        assert!(!dest.0.exists(), "partial extraction must be deleted");
    }
}
