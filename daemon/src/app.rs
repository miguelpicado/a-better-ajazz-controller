//! `orcadeck run <profile>`: HID driver + plugin host + built-in Orca Deck actions in one process.
//!
//! Threads: main (owns the HID device: input, paint, heartbeat), `builtin` (owns the `Registry`,
//! ticks every 400 ms), host accept/socket threads (plugins). Everything that paints sends
//! `(key, image)` over one channel; the main loop keeps the latest image per key and writes it at most
//! as one batch at most once per `batch_gap()`, so a chatty plugin cannot flood the device.
//! Titles and ok/alert overlays are composed over the key's last image before sending; a feedback is
//! two sends (overlay, then the current image ~600 ms later), never a direct device write.
//! The AKP153E reports a press only on release, so a press is delivered as a click (down + up).
use crate::{
    ctl,
    load_img,
    profile::{self, Scene},
    proto::*,
    render,
    scenes::{self, SceneSet, SceneTarget},
    Dock,
};
use base64::Engine;
use hidapi::{HidApi, HidDevice};
use image::{imageops, DynamicImage};
use orcadeck_builtin as bi;
use orcadeck_host as host;
use serde_json::Value;
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc, Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};

pub const BATCH_GAP_MS: u64 = 250;

/// Minimum time between paint batches (all pending keys go out together). The firmware misses key
/// presses while busy with images, so paint traffic must leave idle gaps. Default is 250 ms (smooth 4 fps
/// spinner); ORCADECK_GAP_MS=400 restores the conservative 400 ms gap if needed.
fn batch_gap() -> Duration {
    Duration::from_millis(std::env::var("ORCADECK_GAP_MS").ok().and_then(|v| v.parse().ok()).unwrap_or(BATCH_GAP_MS))
}
const DEVICE: &str = "AKP153E";

fn pct_decode(s: &str) -> Vec<u8> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        let hex = |x: u8| (x as char).to_digit(16);
        match (b[i], b.get(i + 1).and_then(|&x| hex(x)), b.get(i + 2).and_then(|&x| hex(x))) {
            (b'%', Some(h), Some(l)) => {
                out.push((h * 16 + l) as u8);
                i += 3;
            }
            (c, ..) => {
                out.push(c);
                i += 1;
            }
        }
    }
    out
}

fn svg_image(svg: &str, px: u32) -> Option<DynamicImage> {
    let rgba = bi::raster::svg_to_rgba(svg, px).ok()?;
    image::RgbaImage::from_raw(px, px, rgba).map(DynamicImage::ImageRgba8)
}

/// `data:image/...;base64,...` or percent-encoded SVG, as plugins send in `setImage`.
fn decode_uri(uri: &str, px: u32) -> Option<DynamicImage> {
    let (meta, data) = uri.strip_prefix("data:")?.split_once(',')?;
    let bytes = if meta.ends_with(";base64") {
        base64::engine::general_purpose::STANDARD.decode(data.trim()).ok()?
    } else {
        pct_decode(data)
    };
    if meta.starts_with("image/svg") {
        svg_image(std::str::from_utf8(&bytes).ok()?, px)
    } else {
        image::load_from_memory(&bytes).ok().map(|img| render::fit_px(img, px))
    }
}

const SANS: &str = r#"font-family="Segoe UI, Arial, sans-serif""#;
/// How long an ok/alert overlay stays before the current image is sent again.
const FEEDBACK_MS: u64 = 600;
/// A plugin the new scene dropped is stopped this long after the switch; a scene that needs it
/// again cancels the deadline first.
const RETIRE_MS: u64 = 2000;

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// Transparent SVG with the title bar and text, sized for one key image.
fn title_svg(title: &str, px: u32) -> String {
    let mut label: String = title.chars().take(14).collect();
    if label.len() < title.len() {
        label.push('…');
    }
    let label = xml_escape(&label);
    let (bar, font) = (px * 18 / 85, px * 12 / 85);
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{px}" height="{px}" viewBox="0 0 {px} {px}"><rect y="{}" width="{px}" height="{bar}" fill="#000" fill-opacity=".62"/><text x="{}" y="{}" text-anchor="middle" {SANS} font-weight="600" font-size="{font}" fill="#fff">{label}</text></svg>"##,
        px - bar,
        px / 2,
        px - font / 3,
    )
}

/// Transparent SVG with the ok (green check) or alert (red cross) badge.
fn feedback_svg(ok: bool, px: u32) -> String {
    let (fill, glyph) = if ok {
        ("#16a34a", r#"<path d="M34 51 l11 11 l22 -25" />"#)
    } else {
        ("#dc2626", r#"<path d="M37 37 l26 26 M63 37 l-26 26" />"#)
    };
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{px}" height="{px}" viewBox="0 0 100 100"><circle cx="50" cy="50" r="32" fill="{fill}" fill-opacity=".92" stroke="#fff" stroke-width="3"/><g fill="none" stroke="#fff" stroke-width="8" stroke-linecap="round" stroke-linejoin="round">{glyph}</g></svg>"##
    )
}

/// `svg` (transparent background) rasterised at `px`.
fn raster(svg: &str, px: u32) -> Option<image::RgbaImage> {
    image::RgbaImage::from_raw(px, px, bi::raster::svg_to_rgba(svg, px).ok()?)
}

/// `top` composited over `base` at key size.
fn blend(base: &DynamicImage, top: &image::RgbaImage, px: u32) -> DynamicImage {
    let mut out = if (base.width(), base.height()) == (px, px) {
        base.to_rgba8()
    } else {
        base.resize_exact(px, px, imageops::FilterType::Lanczos3).to_rgba8()
    };
    imageops::overlay(&mut out, top, 0, 0);
    DynamicImage::ImageRgba8(out)
}

fn compose_feedback(base: &DynamicImage, ok: bool, px: u32) -> DynamicImage {
    match raster(&feedback_svg(ok, px), px) {
        Some(top) => blend(base, &top, px),
        None => base.clone(),
    }
}

/// Image of one manifest state, resolved exactly like the scene placeholders (`Images/` of the
/// profile first, then the owning plugin).
fn state_path(scene: &Scene, a: &profile::Action, plugins: &Path, state: usize) -> Option<PathBuf> {
    let image = a.states.get(state)?.image.clone();
    let probe = profile::Action {
        uuid: a.uuid.clone(),
        name: a.name.clone(),
        state: 0,
        states: vec![profile::State { image }],
        settings: a.settings.clone(),
    };
    scene.image_of(&probe, plugins)
}

/// Last image per key, its cached title bar, and the per-state images from the profile.
#[derive(Default)]
struct ViewState {
    base: HashMap<u8, DynamicImage>,
    /// Rasterised title bar per key; only rebuilt when the plugin sends a new `Title`.
    title: HashMap<u8, image::RgbaImage>,
    states: HashMap<u8, Vec<Option<PathBuf>>>,
    /// Last state handled per key, so a repeated `setState` is not painted again.
    state: HashMap<u8, u32>,
}

type View = Arc<Mutex<ViewState>>;

impl ViewState {
    /// Base image with the title bar drawn on it (black if the key has no image yet), always at key
    /// size so `key_jpeg` has nothing left to resize.
    fn effective(&self, key: u8) -> DynamicImage {
        let px = image_px(key);
        let base = self.base.get(&key).cloned().unwrap_or_else(|| DynamicImage::new_rgb8(px, px));
        let base = render::fit_px(base, px);
        match self.title.get(&key) {
            Some(top) => blend(&base, top, px),
            None => base,
        }
    }
}

fn out_key(out: &host::Output) -> Option<&host::Key> {
    match out {
        host::Output::Image { key, .. }
        | host::Output::Title { key, .. }
        | host::Output::State { key, .. }
        | host::Output::ShowOk(key)
        | host::Output::ShowAlert(key) => Some(key),
        _ => None,
    }
}

/// Settings writes are coalesced: a burst from one key causes at most one manifest write per second.
const SETTINGS_WRITE_MS: u64 = 1000;

/// One key image, tagged with the scene generation it belongs to and the context that produced
/// it, so the device loop can drop paints from a scene that is already gone.
struct Paint {
    key: u8,
    img: DynamicImage,
    gen: u64,
    ctx: String,
}

/// An editor save the deck has to reflect. Only the scene currently on the deck is repainted;
/// edits to any other scene stay on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReloadCmd {
    Cell { scene: String, cell: (u8, u8) },
}

#[derive(Debug)]
pub enum ReaderMsg {
    Data(Vec<u8>),
    Wake,
    Error(String),
}

/// State shared by plugin socket threads, the builtin thread and the device loop. Plugin outputs
/// only touch this; the device loop drains it in `tick`, so nothing else writes the device.
struct Runtime {
    tx: mpsc::Sender<Paint>,
    /// Editor saves waiting for the device loop.
    reload_tx: mpsc::Sender<ReloadCmd>,
    /// Optional wake channel used in threaded HID mode to wake the device loop immediately on paints/reloads.
    wake_tx: Mutex<Option<mpsc::Sender<ReaderMsg>>>,
    view: View,
    /// Bumped on every scene switch; paints tagged with an older generation are stale.
    gen: Arc<AtomicU64>,
    /// Plugin contexts the current scene dropped; their late paints are ignored.
    retired: Mutex<HashSet<String>>,
    /// Key -> deadline of its ok/alert restore.
    restores: Mutex<HashMap<u8, Instant>>,
    /// Plugin id -> deadline to stop a process the current scene no longer uses.
    retire: Mutex<HashMap<String, Instant>>,
    /// Action settings waiting for the debounce window.
    settings: Mutex<SettingsPending>,
    /// Manifest of the scene now on the deck, replaced by a scene switch.
    manifest: Mutex<PathBuf>,
    /// `<APPDATA>/OrcaDeck`, holds `globals/<plugin>.json`.
    data: PathBuf,
}

#[derive(Default)]
struct SettingsPending {
    /// Plugin context -> scene cell, filled while placing actions.
    cells: HashMap<String, (u8, u8)>,
    /// Cell -> latest value not written yet.
    pending: HashMap<(u8, u8), Value>,
    /// Last write attempt, for the debounce.
    last: Option<Instant>,
}

impl Runtime {
    /// In threaded mode, registers the channel to wake the device loop when work arrives.
    fn set_wake_tx(&self, tx: Option<mpsc::Sender<ReaderMsg>>) {
        *self.wake_tx.lock().unwrap_or_else(|e| e.into_inner()) = tx;
    }

    fn wake(&self) {
        if let Some(ref tx) = *self.wake_tx.lock().unwrap_or_else(|e| e.into_inner()) {
            let _ = tx.send(ReaderMsg::Wake);
        }
    }

    /// Registers where a plugin instance's context lives in the scene.
    fn set_cell(&self, context: &str, cell: (u8, u8)) {
        self.settings.lock().unwrap_or_else(|e| e.into_inner()).cells.insert(context.to_string(), cell);
    }

    /// Forgets the old scene's plugin cells and any settings queued for them.
    fn clear_cells(&self) {
        let mut p = self.settings.lock().unwrap_or_else(|e| e.into_inner());
        p.cells.clear();
        p.pending.clear();
    }

    /// One paint, tagged so the device loop can drop it after a scene switch.
    fn send(&self, key: u8, img: DynamicImage, context: &str) {
        let _ = self.tx.send(Paint { key, img, gen: self.gen.load(Ordering::Relaxed), ctx: context.to_string() });
        self.wake();
    }

    /// Tells the device loop that the editor saved a cell of `scene` (see [`ReloadCmd`]).
    #[allow(dead_code)] // crates/web's mutation hook sends these once its merge lands
    fn reload(&self, scene: &str, cell: (u8, u8)) {
        let _ = self.reload_tx.send(ReloadCmd::Cell { scene: scene.to_string(), cell });
        self.wake();
    }

    /// Forgets a plugin context whose cell is about to hold another action.
    fn forget(&self, context: &str) {
        let mut p = self.settings.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(cell) = p.cells.remove(context) {
            p.pending.remove(&cell);
        }
    }

    /// Points settings writes at the scene now on the deck.
    fn set_manifest(&self, manifest: PathBuf) {
        *self.manifest.lock().unwrap_or_else(|e| e.into_inner()) = manifest;
    }

    fn retired(&self, context: &str) -> bool {
        self.retired.lock().unwrap_or_else(|e| e.into_inner()).contains(context)
    }

    /// Forgets a plugin context: whatever it paints from now on is a late paint.
    fn retire(&self, context: &str) {
        let mut retired = self.retired.lock().unwrap_or_else(|e| e.into_inner());
        if retired.len() >= 1024 {
            // shortcut: oldest contexts lose protection; upgrade to a ring buffer if a plugin keeps painting them.
            retired.clear();
        }
        retired.insert(context.to_string());
    }

    /// Deadlines for the plugins the switch left unused (`running - used`). A plugin already
    /// waiting keeps its first deadline, so chained switches cannot postpone it forever.
    fn retire_unused(&self, used: &HashMap<String, usize>, running: &HashSet<String>, now: Instant) {
        let mut retire = self.retire.lock().unwrap_or_else(|e| e.into_inner());
        retire.retain(|id, _| running.contains(id) && !used.contains_key(id));
        let deadline = now + Duration::from_millis(RETIRE_MS);
        for id in running {
            if !used.contains_key(id) {
                retire.entry(id.clone()).or_insert(deadline);
            }
        }
    }

    /// The scene now on the deck (or an editor reload) needs this plugin: drop any pending stop.
    fn cancel_retire(&self, plugin: &str) {
        self.retire.lock().unwrap_or_else(|e| e.into_inner()).remove(plugin);
    }

    /// Plugins whose grace period expired without another scene asking for them.
    fn due_retires(&self, now: Instant) -> Vec<String> {
        let mut retire = self.retire.lock().unwrap_or_else(|e| e.into_inner());
        let due = retire.iter().filter(|(_, t)| **t <= now).map(|(id, _)| id.clone()).collect();
        retire.retain(|_, t| *t > now);
        due
    }

    /// One plugin output, on a socket thread. Images go over the paint channel; settings, globals
    /// and URLs are side effects that never touch the device.
    fn output(&self, out: host::Output) {
        let Some(key) = out_key(&out) else {
            match out {
                host::Output::Settings { context, settings } => self.queue_settings(&context, settings),
                host::Output::GlobalSettings { plugin, settings } => save_global(&self.data, &plugin, &settings),
                host::Output::OpenUrl(url) => open_url(&url),
                _ => {}
            }
            return;
        };
        let Some(k) = key_index(key.column as u8, key.row as u8) else { return };
        if self.retired(&key.context) {
            return;
        }
        let ctx = key.context.clone();
        let (ok, px) = (matches!(&out, host::Output::ShowOk(_)), image_px(k));
        let mut v = self.view.lock().unwrap_or_else(|e| e.into_inner());
        let mut image = None;
        match out {
            host::Output::Image { image: uri, .. } => {
                if let Some(img) = decode_uri(&uri, px) {
                    v.base.insert(k, img);
                    image = Some(v.effective(k));
                }
            }
            host::Output::Title { title, .. } => {
                v.title.remove(&k);
                if !title.is_empty() {
                    if let Some(top) = raster(&title_svg(&title, px), px) {
                        v.title.insert(k, top);
                    }
                }
                image = Some(v.effective(k));
            }
            host::Output::State { state, .. } => {
                if v.state.insert(k, state) == Some(state) {
                    return;
                }
                let path = v.states.get(&k).and_then(|s| s.get(state as usize)).cloned().flatten();
                if let Some(img) = path.and_then(|p| load_img(&p).ok()) {
                    v.base.insert(k, render::fit_px(img, px));
                    image = Some(v.effective(k));
                }
            }
            host::Output::ShowOk(_) | host::Output::ShowAlert(_) => {
                let base = v.effective(k);
                drop(v);
                self.feedback(&ctx, k, ok, base);
                return;
            }
            _ => {}
        }
        drop(v);
        if let Some(img) = image {
            self.send(k, img, &ctx);
        }
    }

    /// Overlay now; the device loop restores the key after [`FEEDBACK_MS`] (no timer thread).
    fn feedback(&self, context: &str, key: u8, ok: bool, base: DynamicImage) {
        self.send(key, compose_feedback(&base, ok, image_px(key)), context);
        let deadline = Instant::now() + Duration::from_millis(FEEDBACK_MS);
        self.restores.lock().unwrap_or_else(|e| e.into_inner()).insert(key, deadline);
    }

    /// Builtin images go through the same channel and view as plugin images.
    fn paint(&self, context: &str, key: u8, img: DynamicImage) {
        if self.retired(context) {
            return;
        }
        let img = render::fit_px(img, image_px(key));
        self.view.lock().unwrap_or_else(|e| e.into_inner()).base.insert(key, img.clone());
        self.send(key, img, context);
    }

    /// Deadline work for the device loop, which already wakes every 30 ms for input: pending
    /// feedback restores and debounced settings writes.
    fn tick(&self) {
        self.tick_at(Instant::now());
    }

    fn tick_at(&self, now: Instant) {
        let due: Vec<u8> = {
            let mut m = self.restores.lock().unwrap_or_else(|e| e.into_inner());
            let due = m.iter().filter(|(_, t)| **t <= now).map(|(k, _)| *k).collect();
            m.retain(|_, t| *t > now);
            due
        };
        for k in due {
            let img = self.view.lock().unwrap_or_else(|e| e.into_inner()).effective(k);
            // Restores are dropped on a scene switch, so this is always the current scene.
            self.send(k, img, "");
        }
        self.flush_settings(now);
    }

    /// Computes the nearest deadline for pending feedback restores, retire grace periods,
    /// or debounced settings writes.
    fn next_deadline(&self, now: Instant) -> Option<Instant> {
        let mut nearest: Option<Instant> = None;
        let mut update = |cand: Instant| {
            nearest = Some(match nearest {
                Some(cur) => cur.min(cand),
                None => cand,
            });
        };

        // Feedback overlay restores
        {
            let restores = self.restores.lock().unwrap_or_else(|e| e.into_inner());
            for &t in restores.values() {
                update(t);
            }
        }

        // Plugin retire grace deadlines
        {
            let retire = self.retire.lock().unwrap_or_else(|e| e.into_inner());
            for &t in retire.values() {
                update(t);
            }
        }

        // Debounced settings flush
        {
            let settings = self.settings.lock().unwrap_or_else(|e| e.into_inner());
            if !settings.pending.is_empty() {
                let due = match settings.last {
                    Some(last) => last + Duration::from_millis(SETTINGS_WRITE_MS),
                    None => now,
                };
                update(due);
            }
        }

        nearest
    }

    fn queue_settings(&self, context: &str, settings: Value) {
        let mut p = self.settings.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(&cell) = p.cells.get(context) {
            p.pending.insert(cell, settings);
        }
    }

    /// Writes pending settings once the debounce window has passed. Errors are retried in the next
    /// window; a value the manifest already has is never rewritten.
    fn flush_settings(&self, now: Instant) {
        let mut p = self.settings.lock().unwrap_or_else(|e| e.into_inner());
        if p.pending.is_empty() || p.last.is_some_and(|t| now.duration_since(t) < Duration::from_millis(SETTINGS_WRITE_MS)) {
            return;
        }
        p.last = Some(now);
        let pending = std::mem::take(&mut p.pending);
        drop(p);
        let manifest = self.manifest.lock().unwrap_or_else(|e| e.into_inner()).clone();
        for (cell, settings) in pending {
            match profile::save_action_settings(&manifest, cell.0, cell.1, &settings) {
                Ok(true) => eprintln!("settings {},{} saved", cell.0, cell.1),
                Ok(false) => {}
                Err(e) => {
                    eprintln!("settings {},{}: {e}", cell.0, cell.1);
                    self.settings.lock().unwrap_or_else(|e| e.into_inner()).pending.entry(cell).or_insert(settings);
                }
            }
        }
    }

    /// Writes pending settings before the scene (and with it the manifest) changes; whatever cannot
    /// be written is dropped instead of landing in the next scene's cells.
    fn flush_now(&self) {
        self.settings.lock().unwrap_or_else(|e| e.into_inner()).last = None;
        self.flush_settings(Instant::now());
        self.settings.lock().unwrap_or_else(|e| e.into_inner()).pending.clear();
    }
}

/// Scene id of the manifest now on the deck (`<id>.sdProfile`), empty when unknown.
fn current_scene(rt: &Runtime) -> String {
    rt.manifest
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .parent()
        .and_then(Path::file_name)
        .and_then(|n| n.to_str())
        .unwrap_or_default()
        .trim_end_matches(".sdProfile")
        .to_string()
}

/// Directory of the scene now on the deck.
fn scene_dir(rt: &Runtime) -> Option<PathBuf> {
    rt.manifest.lock().unwrap_or_else(|e| e.into_inner()).parent().map(Path::to_path_buf)
}

/// Live bridge from the web editor to the plugin host: a one-use inspector token for the plugin
/// action on a cell of the scene now on the deck (`orcadeck_web::InspectorBridge`).
pub(crate) struct RunInspector {
    rt: Arc<Runtime>,
    host: host::Host,
    port: u16,
}

impl RunInspector {
    fn new(rt: Arc<Runtime>, host: host::Host) -> RunInspector {
        let port = host.port();
        RunInspector { rt, host, port }
    }

    /// `(token, host port, action context)` to bootstrap the PI, or why there is none.
    fn open(&self, scene: &str, key: &str) -> Result<(String, u16, String), String> {
        if current_scene(&self.rt) != scene {
            return Err(format!("scene {scene:?} is not the one on the deck"));
        }
        let cell: (u8, u8) = key
            .split_once(',')
            .and_then(|(c, r)| Some((c.parse().ok()?, r.parse().ok()?)))
            .ok_or_else(|| format!("bad cell {key:?}"))?;
        let ctx = self
            .rt
            .settings
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .cells
            .iter()
            .find(|(_, c)| **c == cell)
            .map(|(ctx, _)| ctx.clone())
            .ok_or_else(|| format!("no plugin action at {key}"))?;
        let token = self.host.open_inspector(&ctx).ok_or_else(|| format!("action at {key} is not live"))?;
        Ok((token, self.port, ctx))
    }
}

impl orcadeck_web::InspectorBridge for RunInspector {
    fn open(&self, scene: &str, key: &str) -> Result<orcadeck_web::InspectorSession, String> {
        let (token, port, context) = RunInspector::open(self, scene, key)?;
        Ok(orcadeck_web::InspectorSession { token, port, context })
    }
}

/// `%APPDATA%\OrcaDeck\globals\<plugin>.json`, or `None` for an id unsafe as a file name.
fn globals_path(data: &Path, plugin: &str) -> Option<PathBuf> {
    orcadeck_web::store::safe_id(plugin).then(|| data.join("globals").join(format!("{plugin}.json")))
}

fn save_global(data: &Path, plugin: &str, settings: &Value) {
    let Some(path) = globals_path(data, plugin) else { return };
    if let Some(dir) = path.parent() {
        if let Err(e) = std::fs::create_dir_all(dir) {
            return eprintln!("globals {plugin}: {e}");
        }
    }
    if let Err(e) = orcadeck_web::store::atomic_write(&path, &orcadeck_web::store::to_pretty(settings), false) {
        eprintln!("globals {plugin}: {e}");
    }
}

fn load_global(data: &Path, plugin: &str) -> Option<Value> {
    let bytes = std::fs::read(globals_path(data, plugin)?).ok()?;
    serde_json::from_slice(bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(&bytes)).ok()
}

/// `http(s)` only, and nothing that could break out of the quoting or be expanded by `cmd`
/// (`"`, `%`, whitespace, control characters).
fn is_web_url(url: &str) -> bool {
    let u = url.to_ascii_lowercase();
    (u.starts_with("http://") || u.starts_with("https://"))
        && !url.contains(['"', '%'])
        && !url.chars().any(|c| c.is_whitespace() || c.is_control())
}

/// Opens `url` in the default browser with `cmd /c start "" "<url>"`. The URL is validated and
/// passed pre-quoted (`CommandExt::raw_arg`) so `&` and friends stay inside the browser URL.
#[cfg(windows)]
fn open_url(url: &str) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    if !is_web_url(url) {
        return eprintln!("ignored url {url:?}");
    }
    if let Err(e) = std::process::Command::new("cmd")
        .args(["/c", "start", ""])
        .raw_arg(format!("\"{url}\""))
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
    {
        eprintln!("open {url}: {e}");
    }
}

#[cfg(not(windows))]
fn open_url(url: &str) {
    if is_web_url(url) {
        let _ = std::process::Command::new("cmd").args(["/c", "start", "", url]).spawn();
    }
}

/// Where a scene cell's action runs.
#[derive(Clone, Debug, PartialEq)]
enum Target {
    Builtin(String),
    Plugin(String),
    Scene(SceneTarget),
}

/// Work for the builtin thread: presses and scene replacements.
enum BuiltinMsg {
    Press(String),
    Scene(SceneCmd),
    /// One cell was re-placed by the editor: drop `old` and/or register `new` at `key`, leaving
    /// the rest of the scene's builtin keys untouched.
    Cell { key: u8, old: Option<String>, new: Option<bi::Key> },
}

/// Builtin half of a scene switch: the registry stays on its thread, only its keys change.
enum SceneCmd {
    Replace { old: Vec<String>, new: Vec<bi::Key>, ctx_keys: HashMap<String, u8> },
}

/// Result of putting a scene on the deck.
#[derive(Default)]
struct Placed {
    by_key: HashMap<u8, Target>,
    builtin_keys: Vec<bi::Key>,
    /// Context -> key of every placed action; the builtin thread routes presses with this.
    ctx_keys: HashMap<String, u8>,
    /// Placeholder image per key, until the action paints.
    latest: HashMap<u8, DynamicImage>,
    /// Plugin id -> instances in this scene.
    used: HashMap<String, usize>,
}

/// Puts a scene's actions on the deck: scene keys stay in the device loop, built-ins go to the
/// builtin thread, the rest to the plugin host. Replaces the per-key view and key ownership.
fn place_scene(
    scene: &Scene,
    plugins: &Path,
    found: &[host::Plugin],
    host: &host::Host,
    rt: &Runtime,
    builtins: &HashSet<&'static str>,
) -> Placed {
    let mut p = Placed::default();
    let mut view = ViewState::default();
    let mut appearing: Vec<((u8, u8), &profile::Action, &host::Plugin)> = Vec::new();
    for ((c, r), a) in &scene.actions {
        let Some(key) = key_index(*c, *r) else { continue };
        if let Some(img) = scene.image_of(a, plugins).and_then(|path| load_img(&path).ok()) {
            view.base.insert(key, render::fit_px(img.clone(), image_px(key)));
            p.latest.insert(key, img);
        }
        view.states.insert(key, (0..a.states.len()).map(|i| state_path(scene, a, plugins, i)).collect());
        view.state.insert(key, a.state as u32);
        let ctx = format!("{c},{r}");
        if let Some(target) = scenes::target_of(a) {
            p.by_key.insert(key, Target::Scene(target));
        } else if builtins.contains(a.uuid.as_str()) {
            p.ctx_keys.insert(ctx.clone(), key);
            p.builtin_keys.push(bi::Key { uuid: a.uuid.clone(), context: ctx.clone(), device: DEVICE.into(), row: *r as u32, col: *c as u32 });
            p.by_key.insert(key, Target::Builtin(ctx));
        } else if let Some(plugin) = found.iter().filter(|p| a.uuid.starts_with(&format!("{}.", p.id))).max_by_key(|p| p.id.len()) {
            appearing.push(((*c, *r), a, plugin));
        } else {
            eprintln!("no handler for {} at {c},{r} ({})", a.uuid, a.name);
        }
    }
    // Publish the new view before the plugins appear: their first paint has to land in it.
    rt.set_manifest(scene.dir.join("manifest.json"));
    *rt.view.lock().unwrap_or_else(|e| e.into_inner()) = view;
    for ((c, r), a, plugin) in appearing {
        let Some(key) = key_index(c, r) else { continue };
        let mut inst = host::Instance::new(&plugin.id, &a.uuid, DEVICE, c as u32, r as u32);
        inst.settings = a.settings.clone();
        inst.state = a.state as u32;
        let ctx = host.appear(inst);
        rt.set_cell(&ctx, (c, r));
        p.ctx_keys.insert(ctx.clone(), key);
        p.by_key.insert(key, Target::Plugin(ctx));
        *p.used.entry(plugin.id.clone()).or_insert(0) += 1;
    }
    p
}

/// Stops the plugin processes whose grace period expired. Pending settings are flushed first so a
/// kill cannot lose a queued `setSettings`; `running` forgets them, so a later scene that needs one
/// launches it again. Called from the device loop's existing 30 ms wake, never from a timer.
fn retire_due(rt: &Runtime, host: &host::Host, running: &mut HashSet<String>, now: Instant) {
    let due = rt.due_retires(now);
    if due.is_empty() {
        return;
    }
    rt.flush_now();
    for id in due {
        running.remove(&id);
        eprintln!("plugin {id} retired");
        host.stop(&id);
    }
}

/// Starts the plugins a scene needs that are not running yet, and cancels a pending stop for every
/// plugin the scene uses. Restarting a live plugin would lose its settings and state.
fn launch_plugins(
    used: &HashMap<String, usize>,
    found: &[host::Plugin],
    host: &host::Host,
    rt: &Runtime,
    data: &Path,
    running: &mut HashSet<String>,
) {
    for id in used.keys() {
        rt.cancel_retire(id);
        if running.contains(id) {
            continue;
        }
        let Some(plugin) = found.iter().find(|p| &p.id == id) else { continue };
        if let Some(settings) = load_global(data, id) {
            host.load_global_settings(id, settings);
        }
        match host.launch(plugin.clone()) {
            Ok(()) => eprintln!("plugin {id} started"),
            Err(e) => eprintln!("plugin {id}: {e}"),
        }
        running.insert(id.clone());
    }
}

/// One Scene Shift press: load the target scene, swap its actions in and repaint every key that
/// may differ, in one batch. Runs on the device thread: no extra thread, timer or HID writer.
#[allow(clippy::too_many_arguments)]
fn switch_scene(
    scenes: &mut SceneSet,
    target: &SceneTarget,
    plugins: &Path,
    by_key: &mut HashMap<u8, Target>,
    latest: &mut HashMap<u8, DynamicImage>,
    host: &host::Host,
    found: &[host::Plugin],
    running: &mut HashSet<String>,
    tx: &mpsc::Sender<BuiltinMsg>,
    rt: &Runtime,
    builtins: &HashSet<&'static str>,
) -> Result<Vec<(u8, DynamicImage)>, String> {
    let Some(id) = scenes.resolve(target) else {
        return Err(format!("no scene for {target:?}"));
    };
    if id == scenes.current() {
        return Ok(Vec::new());
    }
    let scene = scenes.switch(&id)?;
    // Everything queued before this point belongs to the old scene.
    rt.gen.fetch_add(1, Ordering::SeqCst);
    rt.flush_now();
    rt.clear_cells();
    rt.restores.lock().unwrap_or_else(|e| e.into_inner()).clear();
    let mut old = Vec::new();
    for t in by_key.values() {
        match t {
            Target::Builtin(ctx) => old.push(ctx.clone()),
            Target::Plugin(ctx) => {
                host.disappear(ctx);
                rt.retire(ctx);
            }
            Target::Scene(_) => {}
        }
    }
    let placed = place_scene(&scene, plugins, found, host, rt, builtins);
    let _ = tx.send(BuiltinMsg::Scene(SceneCmd::Replace { old, new: placed.builtin_keys, ctx_keys: placed.ctx_keys }));
    launch_plugins(&placed.used, found, host, rt, &rt.data, running);
    rt.retire_unused(&placed.used, running, Instant::now());
    let black = DynamicImage::new_rgb8(KEY_PX, KEY_PX);
    let mut keys: Vec<u8> = latest.keys().copied().chain(placed.latest.keys().copied()).collect();
    keys.sort_unstable();
    keys.dedup();
    let mut paints = Vec::with_capacity(keys.len());
    for key in keys {
        let img = placed.latest.get(&key).cloned().unwrap_or_else(|| black.clone());
        latest.insert(key, img.clone());
        paints.push((key, img));
    }
    *by_key = placed.by_key;
    Ok(paints)
}

/// One cell the editor saved: rebuild it from the file the editor just wrote and repaint only that
/// key. It runs on the device thread, so a Scene Shift cannot interleave, and the paint goes
/// through the single batch channel like every other image (batch gap included): no new thread, no
/// extra HID writer.
#[allow(clippy::too_many_arguments)]
fn reload_cell(
    scene_id: &str,
    cell: (u8, u8),
    rt: &Runtime,
    by_key: &mut HashMap<u8, Target>,
    host: &host::Host,
    found: &[host::Plugin],
    running: &mut HashSet<String>,
    msg_tx: &mpsc::Sender<BuiltinMsg>,
    builtins: &HashSet<&'static str>,
    plugins: &Path,
) -> Option<(u8, DynamicImage)> {
    let key = key_index(cell.0, cell.1)?;
    let dir = scene_dir(rt)?;
    if dir.file_name().and_then(|n| n.to_str()).unwrap_or_default().trim_end_matches(".sdProfile") != scene_id {
        return None; // another scene: only its file changed
    }
    let scene = match Scene::load(&dir) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("reload {scene_id}: {e}");
            return None;
        }
    };
    let action = scene.actions.iter().find(|((c, r), _)| (*c, *r) == cell).map(|(_, a)| a);

    // The action that was here goes away: a plugin instance is retired and re-created with the
    // settings the editor wrote (`willAppear` carries them), a builtin is re-registered below.
    let mut old_builtin = None;
    match by_key.get(&key) {
        Some(Target::Plugin(old)) => {
            host.disappear(old);
            rt.retire(old);
            rt.forget(old);
        }
        Some(Target::Builtin(old)) => old_builtin = Some(old.clone()),
        _ => {}
    }

    let mut new_builtin = None;
    match action {
        None => {
            by_key.remove(&key);
        }
        Some(a) => {
            if let Some(target) = scenes::target_of(a) {
                by_key.insert(key, Target::Scene(target));
            } else if builtins.contains(a.uuid.as_str()) {
                let ctx = format!("{},{}", cell.0, cell.1);
                new_builtin = Some(bi::Key {
                    uuid: a.uuid.clone(),
                    context: ctx.clone(),
                    device: DEVICE.into(),
                    row: cell.1 as u32,
                    col: cell.0 as u32,
                });
                by_key.insert(key, Target::Builtin(ctx));
            } else if let Some(plugin) = found.iter().filter(|p| a.uuid.starts_with(&format!("{}.", p.id))).max_by_key(|p| p.id.len()) {
                let mut inst = host::Instance::new(&plugin.id, &a.uuid, DEVICE, cell.0 as u32, cell.1 as u32);
                inst.settings = a.settings.clone();
                inst.state = a.state as u32;
                let ctx = host.appear(inst);
                rt.set_cell(&ctx, cell);
                let used: HashMap<String, usize> = HashMap::from([(plugin.id.clone(), 1)]);
                launch_plugins(&used, found, host, rt, &rt.data, running);
                by_key.insert(key, Target::Plugin(ctx));
            } else {
                eprintln!("reload {scene_id}: no handler for {} at {cell:?}", a.uuid);
                by_key.remove(&key);
            }
        }
    }
    if old_builtin.is_some() || new_builtin.is_some() {
        let _ = msg_tx.send(BuiltinMsg::Cell { key, old: old_builtin, new: new_builtin });
    }

    // Placeholder from the profile and the state the editor left selected; a plugin repaints on
    // its own right after `willAppear`.
    let mut v = rt.view.lock().unwrap_or_else(|e| e.into_inner());
    v.title.remove(&key);
    v.base.remove(&key);
    if let Some(img) = action.and_then(|a| scene.image_of(a, plugins)).and_then(|path| load_img(&path).ok()) {
        v.base.insert(key, render::fit_px(img, image_px(key)));
    }
    let states = action.map(|a| (0..a.states.len()).map(|i| state_path(&scene, a, plugins, i)).collect()).unwrap_or_default();
    v.states.insert(key, states);
    v.state.insert(key, action.map_or(0, |a| a.state as u32));
    Some((key, v.effective(key)))
}

/// `orcadeck edit` inside `run`: the web server lives exactly as long as the client stays
/// connected, and it uses the plugin host that already has the plugins running.
fn serve_edit(
    conn: &mut ctl::Conn,
    profile: &Path,
    plugins: &Path,
    data: &Path,
    host: &host::Host,
    inspector: Option<Arc<dyn orcadeck_web::InspectorBridge>>,
) {
    let Some(port) = conn.read_line().as_deref().and_then(ctl::parse_edit) else {
        conn.write_line("ERR expected: EDIT <port>");
        return;
    };
    // Same file layout as the standalone editor, but the profile `run` was started with wins over
    // the editor's default profiles directory.
    let parent = profile.parent().unwrap_or(Path::new(""));
    let profiles = if parent.as_os_str().is_empty() { PathBuf::from(".") } else { parent.to_path_buf() };
    let legacy_config = profiles.parent().filter(|p| !p.as_os_str().is_empty()).map(|p| p.join("config")).filter(|p| p.is_dir());
    let store = orcadeck_web::FsStore {
        profiles,
        plugins: plugins.to_path_buf(),
        builtin_plugins: crate::edit::resolve_paths().builtin_plugins,
        data: data.to_path_buf(),
        legacy_config,
        downloads: None,
        inspector,
    };
    match orcadeck_web::Server::start(Arc::new(store), port) {
        Ok(server) => {
            // The PI connects to the plugin host from the editor page, whose port is only known now.
            host.allow_origin(format!("http://127.0.0.1:{}", server.addr.port()));
            conn.write_line(&format!("OK {}", server.url()));
            conn.wait_close();
            server.stop();
        }
        Err(e) => conn.write_line(&format!("ERR {e}")),
    }
}

pub fn run(profile: &Path, plugins: &Path, n: Option<usize>) -> Result<(), String> {
    let scene = Scene::load(profile)?;
    let (tx, rx) = mpsc::channel::<Paint>();
    let (msg_tx, msg_rx) = mpsc::channel::<BuiltinMsg>();
    let (reload_tx, reload_rx) = mpsc::channel::<ReloadCmd>();

    // Plugin host: only plugins the scene uses are launched.
    let data_root = PathBuf::from(std::env::var_os("APPDATA").unwrap_or_default()).join("OrcaDeck");
    let mut scenes = SceneSet::open(profile, plugins, &data_root, DEVICE)?;
    let found = host::discover(&[plugins.to_path_buf()], Some(&data_root));
    let view: View = Arc::new(Mutex::new(ViewState::default()));
    let runtime = Arc::new(Runtime {
        tx: tx.clone(),
        reload_tx,
        wake_tx: Mutex::new(None),
        view: Arc::clone(&view),
        gen: Arc::new(AtomicU64::new(0)),
        retired: Mutex::new(HashSet::new()),
        restores: Mutex::new(HashMap::new()),
        retire: Mutex::new(HashMap::new()),
        settings: Mutex::new(SettingsPending::default()),
        manifest: Mutex::new(profile.join("manifest.json")),
        data: data_root.clone(),
    });
    let events = Arc::clone(&runtime);
    let host = host::Host::start(
        host::Config {
            devices: vec![host::Device { id: DEVICE.into(), name: "AKP153E".into(), columns: 5, rows: 3, kind: 0 }],
            ..Default::default()
        },
        move |out: host::Output| events.output(out),
    )
    .map_err(|e| format!("plugin host: {e}"))?;

    // `orcadeck edit` attaches here. The worker thread blocks in the named pipe, so it costs no
    // CPU, and it only starts the web server while an edit client is connected.
    let ctl_profile = profile.to_path_buf();
    let ctl_plugins = plugins.to_path_buf();
    let ctl_data = data_root.clone();
    let ctl_host = host.clone();
    let ctl_inspector: Arc<dyn orcadeck_web::InspectorBridge> = Arc::new(RunInspector::new(Arc::clone(&runtime), host.clone()));
    ctl::spawn(ctl::NAME, move |conn| {
        serve_edit(conn, &ctl_profile, &ctl_plugins, &ctl_data, &ctl_host, Some(Arc::clone(&ctl_inspector)))
    });

    // Built-in actions on their own thread: Orca RPC blocks for ~450 ms now and then.
    let mut registry = bi::Registry::new(bi::Deck::new(Box::new(bi::PipeOrca::from_env()), bi::Deck::default_quota_dir()));
    let builtins: HashSet<&'static str> = registry.uuids().into_iter().collect();
    let builtin = Arc::clone(&runtime);
    thread::Builder::new()
        .name("builtin".into())
        .spawn(move || {
            let mut ctx_key: HashMap<String, u8> = HashMap::new();
            loop {
                let now = bi::now_ms();
                for (ctx, svg) in registry.tick(now) {
                    let Some(&k) = ctx_key.get(&ctx) else { continue };
                    if let Some(img) = svg_image(&svg, image_px(k)) {
                        builtin.paint(&ctx, k, img);
                    }
                }
                let timeout = registry.next_timeout(bi::now_ms());
                match msg_rx.recv_timeout(timeout) {
                    Ok(BuiltinMsg::Press(ctx)) => {
                        let fb = registry.key_down(&ctx, bi::now_ms());
                        if fb != bi::Feedback::None {
                            if let Some(&k) = ctx_key.get(&ctx) {
                                let base = builtin.view.lock().unwrap_or_else(|e| e.into_inner()).effective(k);
                                builtin.feedback(&ctx, k, fb == bi::Feedback::Ok, base);
                            }
                        }
                    }
                    Ok(BuiltinMsg::Scene(SceneCmd::Replace { old, new, ctx_keys })) => {
                        for ctx in old {
                            registry.disappear(&ctx);
                        }
                        for key in new {
                            registry.appear(key);
                        }
                        ctx_key = ctx_keys;
                    }
                    Ok(BuiltinMsg::Cell { key, old, new }) => {
                        if let Some(old) = old {
                            registry.disappear(&old);
                            ctx_key.remove(&old);
                        }
                        if let Some(k) = new {
                            let ctx = k.context.clone();
                            registry.appear(k);
                            ctx_key.insert(ctx, key);
                        }
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                    Err(mpsc::RecvTimeoutError::Disconnected) => return,
                }
            }
        })
        .map_err(|e| e.to_string())?;

    let placed = place_scene(&scene, plugins, &found, &host, &runtime, &builtins);
    let _ = msg_tx.send(BuiltinMsg::Scene(SceneCmd::Replace { old: Vec::new(), new: placed.builtin_keys, ctx_keys: placed.ctx_keys }));
    let mut running: HashSet<String> = HashSet::new();
    launch_plugins(&placed.used, &found, &host, &runtime, &data_root, &mut running);
    let mut by_key = placed.by_key;
    let mut latest = placed.latest;
    drop(tx);

    // Main loop: reconnect forever; `latest` repaints the device after a re-plug.
    let black = DynamicImage::new_rgb8(KEY_PX, KEY_PX);
    for k in 1..=15u8 {
        latest.entry(k).or_insert_with(|| black.clone());
    }
    loop {
        let s = session(n, &rx, &reload_rx, &mut latest, &mut by_key, &host, &msg_tx, &runtime, &mut scenes, plugins, &found, &mut running, &builtins);
        match s {
            Ok(()) => return Ok(()),
            Err(e) => {
                eprintln!("device: {e}; retrying in 2 s");
                runtime.tick();
                retire_due(&runtime, &host, &mut running, Instant::now());
                thread::sleep(Duration::from_secs(2));
            }
        }
    }
}

fn hid_reader_mode() -> bool {
    std::env::var("ORCADECK_HID_READER")
        .map(|v| v.eq_ignore_ascii_case("thread"))
        .unwrap_or(false)
}

pub trait DockWriter {
    fn report_n(&self) -> usize;
    fn send(&mut self, payload: &[u8]) -> Result<(), String>;
    fn set_key(&mut self, key: u8, jpeg: &[u8]) -> Result<(), String>;
    fn flush(&mut self) -> Result<(), String>;
}

impl DockWriter for Dock {
    fn report_n(&self) -> usize {
        self.n
    }
    fn send(&mut self, payload: &[u8]) -> Result<(), String> {
        Dock::send(self, payload)
    }
    fn set_key(&mut self, key: u8, jpeg: &[u8]) -> Result<(), String> {
        Dock::set_key(self, key, jpeg)
    }
    fn flush(&mut self) -> Result<(), String> {
        Dock::flush(self)
    }
}

fn open_read_device(api: &HidApi) -> Result<HidDevice, String> {
    let mut cands: Vec<_> = api.device_list().filter(|d| d.vendor_id() == VID && d.product_id() == PID).collect();
    cands.sort_by_key(|d| if d.usage_page() >= 0xFF00 { 0 } else { 1 });
    let info = cands.first().ok_or("device not found (reader handle)")?;
    info.open_device(api).map_err(|e| e.to_string())
}

fn compute_next_wake(
    pending: &HashMap<u8, (DynamicImage, u64)>,
    last_batch: Instant,
    gap: Duration,
    hb: Instant,
    rt: &Runtime,
    now: Instant,
) -> Instant {
    let mut wake = hb + Duration::from_secs(HEARTBEAT_SECS);
    if !pending.is_empty() {
        wake = wake.min(last_batch + gap);
    }
    if let Some(d) = rt.next_deadline(now) {
        wake = wake.min(d);
    }
    wake
}

#[allow(clippy::too_many_arguments)]
fn handle_input_packet<W: DockWriter>(
    buf: &[u8],
    dock: &mut W,
    by_key: &mut HashMap<u8, Target>,
    msg_tx: &mpsc::Sender<BuiltinMsg>,
    host: &host::Host,
    scenes: &mut SceneSet,
    plugins: &Path,
    latest: &mut HashMap<u8, DynamicImage>,
    found: &[host::Plugin],
    running: &mut HashSet<String>,
    rt: &Runtime,
    builtins: &HashSet<&'static str>,
    pending: &mut HashMap<u8, (DynamicImage, u64)>,
    t0: Instant,
) -> Result<(), String> {
    if std::env::var_os("ORCADECK_TRACE").is_some() {
        eprintln!("{:>6}ms in {:02x?}", t0.elapsed().as_millis(), &buf[..buf.len().min(16)]);
    }
    match parse_input(buf) {
        Input::Handshake => dock.send(&handshake_reply(dock.report_n()))?,
        Input::Key { key, .. } if (1..=15).contains(&key) => match by_key.get(&key).cloned() {
            Some(Target::Builtin(ctx)) => {
                let _ = msg_tx.send(BuiltinMsg::Press(ctx));
            }
            Some(Target::Plugin(ctx)) => {
                host.key(&ctx, true);
                host.key(&ctx, false);
            }
            Some(Target::Scene(target)) => {
                match switch_scene(scenes, &target, plugins, by_key, latest, host, found, running, msg_tx, rt, builtins) {
                    Ok(paints) => {
                        let gen = rt.gen.load(Ordering::Relaxed);
                        pending.extend(paints.into_iter().map(|(key, img)| (key, (img, gen))));
                    }
                    Err(e) => eprintln!("scene: {e}"),
                }
            }
            None => {}
        },
        _ => {}
    }
    Ok(())
}

fn process_paints<W: DockWriter>(
    rx: &mpsc::Receiver<Paint>,
    rt: &Runtime,
    latest: &mut HashMap<u8, DynamicImage>,
    pending: &mut HashMap<u8, (DynamicImage, u64)>,
    last_batch: &mut Instant,
    gap: Duration,
    dock: &mut W,
    t0: Instant,
) -> Result<(), String> {
    while let Ok(p) = rx.try_recv() {
        if p.gen < rt.gen.load(Ordering::Relaxed) || rt.retired(&p.ctx) {
            continue;
        }
        latest.insert(p.key, p.img.clone());
        pending.insert(p.key, (p.img, p.gen));
    }
    if !pending.is_empty() && last_batch.elapsed() >= gap {
        let gen = rt.gen.load(Ordering::Relaxed);
        pending.retain(|_, (_, g)| *g >= gen);
        if !pending.is_empty() {
            let (started, count) = (Instant::now(), pending.len());
            let mut bytes = 0;
            for (key, (img, _)) in pending.drain() {
                let j = render::key_jpeg(&img, key);
                bytes += j.len();
                dock.set_key(key, &j)?;
            }
            dock.flush()?;
            *last_batch = Instant::now();
            if std::env::var_os("ORCADECK_TRACE").is_some() {
                eprintln!("{:>6}ms painted {count} keys {bytes} B in {} ms", t0.elapsed().as_millis(), started.elapsed().as_millis());
            }
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn session_loop_threaded<W: DockWriter>(
    event_rx: &mpsc::Receiver<ReaderMsg>,
    dock: &mut W,
    rx: &mpsc::Receiver<Paint>,
    reload: &mpsc::Receiver<ReloadCmd>,
    latest: &mut HashMap<u8, DynamicImage>,
    by_key: &mut HashMap<u8, Target>,
    host: &host::Host,
    msg_tx: &mpsc::Sender<BuiltinMsg>,
    rt: &Runtime,
    scenes: &mut SceneSet,
    plugins: &Path,
    found: &[host::Plugin],
    running: &mut HashSet<String>,
    builtins: &HashSet<&'static str>,
    pending: &mut HashMap<u8, (DynamicImage, u64)>,
    gap: Duration,
    mut last_batch: Instant,
    mut hb: Instant,
    t0: Instant,
) -> Result<(), String> {
    loop {
        let now = Instant::now();
        let next_wake = compute_next_wake(pending, last_batch, gap, hb, rt, now);
        let timeout = next_wake.saturating_duration_since(now);

        match event_rx.recv_timeout(timeout) {
            Ok(ReaderMsg::Data(buf)) => {
                handle_input_packet(&buf, dock, by_key, msg_tx, host, scenes, plugins, latest, found, running, rt, builtins, pending, t0)?;
            }
            Ok(ReaderMsg::Wake) => {}
            Ok(ReaderMsg::Error(err)) => {
                return Err(err);
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err("reader thread disconnected".into());
            }
        }

        while let Ok(msg) = event_rx.try_recv() {
            match msg {
                ReaderMsg::Data(buf) => {
                    handle_input_packet(&buf, dock, by_key, msg_tx, host, scenes, plugins, latest, found, running, rt, builtins, pending, t0)?;
                }
                ReaderMsg::Wake => {}
                ReaderMsg::Error(err) => return Err(err),
            }
        }

        rt.tick();
        retire_due(rt, host, running, Instant::now());

        while let Ok(ReloadCmd::Cell { scene, cell }) = reload.try_recv() {
            if let Some((key, img)) = reload_cell(&scene, cell, rt, by_key, host, found, running, msg_tx, builtins, plugins) {
                latest.insert(key, img.clone());
                pending.insert(key, (img, rt.gen.load(Ordering::Relaxed)));
            }
        }

        process_paints(rx, rt, latest, pending, &mut last_batch, gap, dock, t0)?;

        if hb.elapsed() >= Duration::from_secs(HEARTBEAT_SECS) {
            dock.send(&heartbeat(dock.report_n()))?;
            hb = Instant::now();
        }
    }
}

/// One connection to the device. Returns Err when it is lost or cannot be opened.
#[allow(clippy::too_many_arguments)]
fn session(
    n: Option<usize>,
    rx: &mpsc::Receiver<Paint>,
    reload: &mpsc::Receiver<ReloadCmd>,
    latest: &mut HashMap<u8, DynamicImage>,
    by_key: &mut HashMap<u8, Target>,
    host: &host::Host,
    msg_tx: &mpsc::Sender<BuiltinMsg>,
    rt: &Runtime,
    scenes: &mut SceneSet,
    plugins: &Path,
    found: &[host::Plugin],
    running: &mut HashSet<String>,
    builtins: &HashSet<&'static str>,
) -> Result<(), String> {
    let api = HidApi::new().map_err(|e| e.to_string())?;
    let mut dock = Dock::open(&api, n)?;
    dock.init(80)?;
    eprintln!("device ready, N = {}", dock.n);
    let gen = rt.gen.load(Ordering::Relaxed);
    let mut pending: HashMap<u8, (DynamicImage, u64)> = latest.iter().map(|(&k, img)| (k, (img.clone(), gen))).collect();
    let gap = batch_gap();
    let mut last_batch = Instant::now() - gap;
    let mut hb = Instant::now();
    let t0 = Instant::now();

    if !hid_reader_mode() {
        let mut buf = vec![0u8; dock.n + 1];
        loop {
            let k = dock.dev.read_timeout(&mut buf, 30).map_err(|e| e.to_string())?;
            if k > 0 {
                handle_input_packet(&buf[..k], &mut dock, by_key, msg_tx, host, scenes, plugins, latest, found, running, rt, builtins, &mut pending, t0)?;
            }
            rt.tick();
            retire_due(rt, host, running, Instant::now());
            // Editor saves: repaint just that key, through the same pending batch as everything else.
            while let Ok(ReloadCmd::Cell { scene, cell }) = reload.try_recv() {
                if let Some((key, img)) = reload_cell(&scene, cell, rt, by_key, host, found, running, msg_tx, builtins, plugins) {
                    latest.insert(key, img.clone());
                    pending.insert(key, (img, rt.gen.load(Ordering::Relaxed)));
                }
            }
            process_paints(rx, rt, latest, &mut pending, &mut last_batch, gap, &mut dock, t0)?;
            if hb.elapsed() >= Duration::from_secs(HEARTBEAT_SECS) {
                dock.send(&heartbeat(dock.n))?;
                hb = Instant::now();
            }
        }
    } else {
        let (event_tx, event_rx) = mpsc::channel();
        let read_dev = open_read_device(&api)?;
        let report_n = dock.n;
        let reader_tx = event_tx.clone();
        let _reader = thread::Builder::new()
            .name("hid-reader".into())
            .spawn(move || {
                let mut buf = vec![0u8; report_n + 1];
                loop {
                    match read_dev.read(&mut buf) {
                        Ok(k) if k > 0 => {
                            if reader_tx.send(ReaderMsg::Data(buf[..k].to_vec())).is_err() {
                                break;
                            }
                        }
                        Ok(_) => {
                            let _ = reader_tx.send(ReaderMsg::Error("read returned 0 bytes (device disconnected)".into()));
                            break;
                        }
                        Err(e) => {
                            let _ = reader_tx.send(ReaderMsg::Error(e.to_string()));
                            break;
                        }
                    }
                }
            })
            .map_err(|e| e.to_string())?;

        rt.set_wake_tx(Some(event_tx));
        let res = session_loop_threaded(
            &event_rx,
            &mut dock,
            rx,
            reload,
            latest,
            by_key,
            host,
            msg_tx,
            rt,
            scenes,
            plugins,
            found,
            running,
            builtins,
            &mut pending,
            gap,
            last_batch,
            hb,
            t0,
        );
        rt.set_wake_tx(None);
        res
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn white(px: u32) -> DynamicImage {
        DynamicImage::ImageRgb8(image::RgbImage::from_pixel(px, px, image::Rgb([255, 255, 255])))
    }

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("orcadeck-app-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn runtime(dir: &Path) -> (Runtime, mpsc::Receiver<Paint>) {
        let (reload_tx, _) = mpsc::channel();
        runtime_with_reload(dir, reload_tx)
    }

    fn runtime_with_reload(dir: &Path, reload_tx: mpsc::Sender<ReloadCmd>) -> (Runtime, mpsc::Receiver<Paint>) {
        let (tx, rx) = mpsc::channel();
        let rt = Runtime {
            tx,
            reload_tx,
            wake_tx: Mutex::new(None),
            view: Arc::new(Mutex::new(ViewState::default())),
            gen: Arc::new(AtomicU64::new(0)),
            retired: Mutex::new(HashSet::new()),
            restores: Mutex::new(HashMap::new()),
            retire: Mutex::new(HashMap::new()),
            settings: Mutex::new(SettingsPending::default()),
            manifest: Mutex::new(dir.join("manifest.json")),
            data: dir.to_path_buf(),
        };
        (rt, rx)
    }

    fn settings_at(path: &Path) -> Value {
        let doc: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        doc["Actions"]["2,1"]["Settings"].clone()
    }

    #[test]
    fn settings_are_debounced_and_unchanged_is_not_written() {
        let dir = tmp("settings");
        let path = dir.join("manifest.json");
        std::fs::write(&path, r#"{"Actions":{"2,1":{"UUID":"a","Settings":{"v":0}}}}"#).unwrap();
        let (rt, _rx) = runtime(&dir);
        rt.set_cell("CTX", (2, 1));
        let t0 = Instant::now();

        rt.queue_settings("CTX", json!({"v": 1}));
        rt.flush_settings(t0);
        assert_eq!(settings_at(&path), json!({"v": 1}));

        rt.queue_settings("CTX", json!({"v": 2}));
        rt.queue_settings("UNKNOWN", json!({"v": 9}));
        rt.flush_settings(t0 + Duration::from_millis(400));
        assert_eq!(settings_at(&path), json!({"v": 1}), "debounced");

        rt.queue_settings("CTX", json!({"v": 3}));
        rt.flush_settings(t0 + Duration::from_millis(1100));
        assert_eq!(settings_at(&path), json!({"v": 3}), "latest value wins");

        let stamp = std::fs::metadata(&path).unwrap().modified().unwrap();
        rt.queue_settings("CTX", json!({"v": 3}));
        rt.flush_settings(t0 + Duration::from_millis(2500));
        assert_eq!(std::fs::metadata(&path).unwrap().modified().unwrap(), stamp, "unchanged value is not rewritten");
    }

    #[test]
    fn feedback_restores_after_its_deadline() {
        let dir = tmp("feedback");
        let (rt, rx) = runtime(&dir);
        let base = white(KEY_PX);
        rt.view.lock().unwrap().base.insert(1, base.clone());
        let t0 = Instant::now();
        rt.feedback("", 1, true, base.clone());
        let Paint { key: k, img: overlay, .. } = rx.try_recv().expect("overlay");
        assert_eq!(k, 1);
        assert_ne!(overlay.to_rgba8(), base.to_rgba8());

        rt.tick_at(t0 + Duration::from_millis(FEEDBACK_MS - 50));
        assert!(rx.try_recv().is_err(), "restored before the deadline");
        // The deadline is recorded after the overlay is rasterised, so read it instead of guessing.
        let deadline = *rt.restores.lock().unwrap().get(&1).unwrap();
        rt.tick_at(deadline - Duration::from_millis(1));
        assert!(rx.try_recv().is_err(), "restored just before the deadline");
        rt.tick_at(deadline);
        let (k, restored) = match rx.try_recv().expect("restore") {
            Paint { key, img, .. } => (key, img),
        };
        assert_eq!(k, 1);
        assert_eq!(restored.to_rgba8(), base.to_rgba8());
    }

    #[test]
    fn a_switch_retires_the_old_plugin_contexts() {
        let dir = tmp("retire");
        let (rt, rx) = runtime(&dir);
        rt.set_cell("CTX", (2, 1));
        rt.retire("CTX");
        let key = host::Key { context: "CTX".into(), device: DEVICE.into(), column: 2, row: 1 };
        rt.output(host::Output::Title { key, title: "x".into(), state: None });
        assert!(rx.try_recv().is_err(), "late paint from the old scene is dropped");
        rt.clear_cells();
        assert!(rt.settings.lock().unwrap().cells.get("CTX").is_none(), "old cell mapping is dropped");
    }

    #[test]
    fn unused_plugins_are_retired_after_the_grace_period_and_cancelled_on_return() {
        let dir = tmp("retire-deadline");
        let (rt, _rx) = runtime(&dir);
        let running: HashSet<String> = ["com.a", "com.b"].into_iter().map(String::from).collect();
        let used_b: HashMap<String, usize> = HashMap::from([("com.b".to_string(), 1)]);
        let t0 = Instant::now();

        rt.retire_unused(&used_b, &running, t0);
        assert!(rt.due_retires(t0 + Duration::from_millis(RETIRE_MS - 1)).is_empty(), "not before 2 s");
        // A switch back to the scene that uses both cancels com.a's pending stop.
        let used_ab: HashMap<String, usize> =
            HashMap::from([("com.a".to_string(), 1), ("com.b".to_string(), 1)]);
        rt.retire_unused(&used_ab, &running, t0 + Duration::from_millis(500));
        assert!(rt.due_retires(t0 + Duration::from_secs(10)).is_empty(), "a used plugin is not retired");

        // Chained switches do not extend an existing deadline.
        rt.retire_unused(&used_b, &running, t0);
        rt.retire_unused(&used_b, &running, t0 + Duration::from_millis(1000));
        let mut due = rt.due_retires(t0 + Duration::from_millis(RETIRE_MS));
        due.sort();
        assert_eq!(due, ["com.a"], "only the unused plugin, at its first deadline");
        assert!(rt.due_retires(t0 + Duration::from_secs(10)).is_empty(), "the deadline is one-shot");
    }

    #[test]
    fn retiring_flushes_settings_first_and_forgets_the_plugin() {
        let dir = tmp("retire-flush");
        let path = dir.join("manifest.json");
        std::fs::write(&path, r#"{"Actions":{"2,1":{"UUID":"a","Settings":{"v":0}}}}"#).unwrap();
        let (rt, _rx) = runtime(&dir);
        rt.set_cell("CTX", (2, 1));
        rt.queue_settings("CTX", json!({"v": 1}));
        let host = host::Host::start(host::Config { port: 0, ..Default::default() }, |_| {}).unwrap();
        let mut running: HashSet<String> = ["com.a".to_string()].into_iter().collect();
        let t0 = Instant::now();
        rt.retire_unused(&HashMap::new(), &running, t0);

        retire_due(&rt, &host, &mut running, t0 + Duration::from_millis(RETIRE_MS - 1));
        assert!(running.contains("com.a"), "still inside the grace period");
        assert_eq!(settings_at(&path), json!({"v": 0}), "nothing flushed early");

        retire_due(&rt, &host, &mut running, t0 + Duration::from_millis(RETIRE_MS));
        assert_eq!(settings_at(&path), json!({"v": 1}), "pending settings are written before the stop");
        assert!(!running.contains("com.a"), "running forgets the retired plugin so a scene can launch it again");
    }

    #[test]
    fn global_settings_round_trip() {
        let dir = tmp("globals");
        save_global(&dir, "com.foo.bar", &json!({"b": 2, "a": 1}));
        assert_eq!(load_global(&dir, "com.foo.bar"), Some(json!({"a": 1, "b": 2})));
        assert_eq!(load_global(&dir, "missing"), None);
        let raw = std::fs::read_to_string(dir.join("globals").join("com.foo.bar.json")).unwrap();
        assert!(raw.starts_with("{\n    \"a\": 1"), "4-space sorted file: {raw}");
        assert!(globals_path(&dir, "../evil").is_none(), "path traversal rejected");
    }

    #[test]
    fn only_web_urls_pass() {
        for ok in ["http://x", "HTTPS://example.com/a?b=1&c=2", "https://127.0.0.1:8080/"] {
            assert!(is_web_url(ok), "{ok}");
        }
        for bad in [
            "file:///C:/x",
            "javascript:alert(1)",
            "ftp://x",
            "mailto:a@b",
            "https://x/\"",
            "https://x/%PATH%",
            "https://x/a b",
            "",
        ] {
            assert!(!is_web_url(bad), "{bad}");
        }
    }

    #[test]
    fn title_composes_at_key_size() {
        let base = white(KEY_PX);
        let top = raster(&title_svg("Agent 1", KEY_PX), KEY_PX).expect("title svg");
        let out = blend(&base, &top, KEY_PX);
        assert_eq!((out.width(), out.height()), (KEY_PX, KEY_PX));
        let (b, o) = (base.to_rgba8(), out.to_rgba8());
        assert_ne!(b.get_pixel(KEY_PX / 2, KEY_PX - 3), o.get_pixel(KEY_PX / 2, KEY_PX - 3), "bottom bar not drawn");
    }

    #[test]
    fn title_svg_escapes_text_and_truncates() {
        let svg = title_svg("a&b <c>", KEY_PX);
        assert!(svg.contains("a&amp;b &lt;c&gt;"));
        assert!(title_svg(&"x".repeat(40), KEY_PX).contains('…'));
    }

    fn big(px: u32) -> DynamicImage {
        DynamicImage::ImageRgb8(image::RgbImage::from_pixel(px, px, image::Rgb([1, 2, 3])))
    }

    #[test]
    fn ingest_scales_images_to_key_size_once() {
        let dir = tmp("ingest-px");
        let (rt, rx) = runtime(&dir);
        rt.paint("c", 1, big(200));
        rt.paint("c", 16, big(200));
        {
            let v = rt.view.lock().unwrap();
            assert_eq!((v.base[&1].width(), v.base[&1].height()), (KEY_PX, KEY_PX), "main key stored at 85");
            assert_eq!((v.base[&16].width(), v.base[&16].height()), (SIDE_PX, SIDE_PX), "side key stored at 80");
        }
        for p in [rx.try_recv().unwrap(), rx.try_recv().unwrap()] {
            assert_eq!((p.img.width(), p.img.height()), (image_px(p.key), image_px(p.key)), "paint already at key size");
        }

        let png = dir.join("big.png");
        big(200).save(&png).unwrap();
        let uri = format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(std::fs::read(&png).unwrap()));
        let img = decode_uri(&uri, image_px(1)).expect("data uri");
        assert_eq!((img.width(), img.height()), (KEY_PX, KEY_PX), "raster data URI scaled at decode");
    }

    #[test]
    fn feedback_overlay_changes_pixels() {
        let base = white(KEY_PX);
        for ok in [true, false] {
            let out = compose_feedback(&base, ok, KEY_PX);
            assert_eq!((out.width(), out.height()), (KEY_PX, KEY_PX));
            let (b, o) = (base.to_rgba8(), out.to_rgba8());
            assert_ne!(b.get_pixel(KEY_PX / 4, KEY_PX / 2), o.get_pixel(KEY_PX / 4, KEY_PX / 2), "badge missing (ok={ok})");
        }
    }

    /// A real image file, so `Scene::image_of` and `load_img` run exactly as on disk.
    fn png(path: &Path, rgb: [u8; 3]) {
        image::RgbImage::from_pixel(KEY_PX, KEY_PX, image::Rgb(rgb)).save(path).expect("png");
    }

    #[test]
    fn an_editor_reload_repaints_only_the_changed_cell() {
        let root = tmp("reload");
        let dir = root.join("A.sdProfile");
        std::fs::create_dir_all(dir.join("Images")).unwrap();
        png(&dir.join("Images/old.png"), [255, 0, 0]);
        png(&dir.join("Images/new.png"), [0, 255, 0]);
        let manifest = dir.join("manifest.json");
        std::fs::write(&manifest, r#"{"Actions":{"0,0":{"UUID":"com.x.action","States":[{"Image":"old.png"}]}}}"#).unwrap();

        let (rt, _paints) = runtime(&root);
        *rt.manifest.lock().unwrap() = manifest.clone();
        let host = host::Host::start(host::Config { port: 0, ..Default::default() }, |_| {}).unwrap();
        let old = host.appear(host::Instance::new("com.x", "com.x.action", DEVICE, 0, 0));
        rt.set_cell(&old, (0, 0));
        rt.view.lock().unwrap().base.insert(13, white(KEY_PX));

        let (tx, rx) = mpsc::channel();
        let mut by_key = HashMap::from([(13, Target::Plugin(old.clone()))]);
        let mut running = HashSet::new();
        let builtins: HashSet<&'static str> = ["com.test.builtin"].into_iter().collect();
        let found: Vec<host::Plugin> = Vec::new();

        assert!(
            reload_cell("B", (0, 0), &rt, &mut by_key, &host, &found, &mut running, &tx, &builtins, &root).is_none(),
            "a scene that is not on the deck is only written to disk"
        );

        // The editor replaces the action and its image; only key 13 is repainted.
        std::fs::write(&manifest, r#"{"Actions":{"0,0":{"UUID":"com.test.builtin","States":[{"Image":"new.png"}]}}}"#).unwrap();
        let (key, img) = reload_cell("A", (0, 0), &rt, &mut by_key, &host, &found, &mut running, &tx, &builtins, &root)
            .expect("reload");
        assert_eq!(key, 13);
        assert_eq!(*img.to_rgb8().get_pixel(0, 0), image::Rgb([0, 255, 0]), "placeholder from the fresh manifest");
        assert!(rt.retired(&old), "the old plugin instance gets no more paints");
        assert_eq!(by_key.get(&13), Some(&Target::Builtin("0,0".into())));
        match rx.try_recv().unwrap() {
            BuiltinMsg::Cell { key: k, old: gone, new: Some(new) } => {
                assert_eq!((k, gone), (13, None), "a plugin has no builtin registration to drop");
                assert_eq!((new.uuid.as_str(), new.context.as_str(), new.row, new.col), ("com.test.builtin", "0,0", 0, 0));
            }
            _ => panic!("the builtin thread is not told about the new action"),
        }

        // A builtin replaced by another builtin keeps the same context and re-registers in place.
        let builtins: HashSet<&'static str> = ["com.test.builtin", "com.test.builtin2"].into_iter().collect();
        std::fs::write(&manifest, r#"{"Actions":{"0,0":{"UUID":"com.test.builtin2","States":[{"Image":"old.png"}]}}}"#).unwrap();
        assert!(reload_cell("A", (0, 0), &rt, &mut by_key, &host, &found, &mut running, &tx, &builtins, &root).is_some());
        match rx.try_recv().unwrap() {
            BuiltinMsg::Cell { key: 13, old: Some(gone), new: Some(new) } => {
                assert_eq!((gone.as_str(), new.uuid.as_str()), ("0,0", "com.test.builtin2"));
            }
            _ => panic!("the old builtin is not dropped"),
        }
    }

    #[test]
    fn inspector_tokens_need_the_scene_on_the_deck() {
        let root = tmp("inspector");
        let dir = root.join("A.sdProfile");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("manifest.json"), "{}").unwrap();
        let (rt, _paints) = runtime(&root);
        *rt.manifest.lock().unwrap() = dir.join("manifest.json");

        let host = host::Host::start(host::Config { port: 0, ..Default::default() }, |_| {}).unwrap();
        let ctx = host.appear(host::Instance::new("com.x", "com.x.action", DEVICE, 2, 1));
        rt.set_cell(&ctx, (2, 1));
        let inspector = RunInspector::new(Arc::new(rt), host.clone());

        let (token, port, got) = inspector.open("A", "2,1").expect("token");
        assert!(!token.is_empty());
        assert_eq!((got.as_str(), port), (ctx.as_str(), host.port()));
        assert!(inspector.open("B", "2,1").is_err(), "scene not on the deck");
        assert!(inspector.open("A", "0,0").is_err(), "no plugin action in that cell");
        assert!(inspector.open("A", "2,1").is_ok(), "each request gets its own single-use token");
    }

    /// Task 4 end to end, without a device: a client asks the daemon for the editor, the server
    /// appears inside this process and goes away when the client disconnects (Ctrl+C).
    #[cfg(windows)]
    #[test]
    fn edit_inside_run_lives_only_while_its_client_does() {
        let root = tmp("serve-edit");
        let scene = root.join("profiles").join("A.sdProfile");
        std::fs::create_dir_all(&scene).unwrap();
        std::fs::write(scene.join("manifest.json"), r#"{"DeviceUUID":"AKP153E","Actions":{}}"#).unwrap();
        let host = host::Host::start(host::Config { port: 0, ..Default::default() }, |_| {}).unwrap();
        let name = format!(r"\\.\pipe\orcadeck-ctl-test-edit-{}", std::process::id());
        let listener = ctl::Listener::new(&name).expect("pipe instance");
        let (profile, plugins, data) = (scene.clone(), root.join("plugins"), root.clone());
        let server = std::thread::spawn(move || {
            assert!(listener.accept(|conn| serve_edit(conn, &profile, &plugins, &data, &host, None)));
        });

        let session = ctl::request_at(&name, 0).expect("daemon present").expect("editor url");
        let rest = session.url().trim_start_matches("http://127.0.0.1:");
        let port: u16 = rest.split('/').next().unwrap().parse().unwrap();
        std::net::TcpStream::connect(("127.0.0.1", port)).expect("the embedded editor listens");

        drop(session); // Ctrl+C in `orcadeck edit`
        server.join().expect("serve_edit returns and stops the server");
    }

    #[derive(Default)]
    struct FakeWriter {
        flushes: Vec<(Instant, usize)>,
        keys_sent: Vec<(Instant, u8, Vec<u8>)>,
        sent_payloads: Vec<Vec<u8>>,
        n: usize,
        fail_set_key_on: Option<u8>,
        fail_send: bool,
        fail_flush: bool,
    }

    impl DockWriter for FakeWriter {
        fn report_n(&self) -> usize {
            self.n
        }
        fn send(&mut self, payload: &[u8]) -> Result<(), String> {
            if self.fail_send {
                return Err("unplugged during send".into());
            }
            self.sent_payloads.push(payload.to_vec());
            Ok(())
        }
        fn set_key(&mut self, key: u8, jpeg: &[u8]) -> Result<(), String> {
            if self.fail_set_key_on == Some(key) {
                return Err(format!("unplugged during set_key for key {key}"));
            }
            self.keys_sent.push((Instant::now(), key, jpeg.to_vec()));
            Ok(())
        }
        fn flush(&mut self) -> Result<(), String> {
            if self.fail_flush {
                return Err("unplugged during flush".into());
            }
            self.flushes.push((Instant::now(), self.keys_sent.len()));
            Ok(())
        }
    }

    #[test]
    fn coalescing_delivers_latest_image_and_batches_stay_separated_by_gap() {
        let dir = tmp("coalescing");
        let (rt, rx) = runtime(&dir);
        let mut latest = HashMap::new();
        let mut pending = HashMap::new();
        let gap = Duration::from_millis(50);
        let mut last_batch = Instant::now() - gap;
        let mut writer = FakeWriter { n: 512, ..Default::default() };

        // 1. Queue multiple frames for key 3 (e.g. Red, Green, Blue) during one gap
        let red = DynamicImage::ImageRgb8(image::RgbImage::from_pixel(KEY_PX, KEY_PX, image::Rgb([255, 0, 0])));
        let green = DynamicImage::ImageRgb8(image::RgbImage::from_pixel(KEY_PX, KEY_PX, image::Rgb([0, 255, 0])));
        let blue = DynamicImage::ImageRgb8(image::RgbImage::from_pixel(KEY_PX, KEY_PX, image::Rgb([0, 0, 255])));

        rt.send(3, red, "c");
        rt.send(3, green, "c");
        rt.send(3, blue.clone(), "c");

        // Also queue another key 4
        let white_img = white(KEY_PX);
        rt.send(4, white_img, "c");

        // Process batch
        process_paints(&rx, &rt, &mut latest, &mut pending, &mut last_batch, gap, &mut writer, Instant::now()).unwrap();

        // Verification:
        // Key 3 must be sent only ONCE (coalesced to the latest image: blue)
        // Key 4 must be sent once
        assert_eq!(writer.flushes.len(), 1, "exactly one STP flush for the batch");
        assert_eq!(writer.keys_sent.len(), 2, "2 keys sent");

        let key3_sent = writer.keys_sent.iter().find(|(_, k, _)| *k == 3).expect("key 3 sent");
        let expected_blue_jpeg = render::key_jpeg(&blue, 3);
        assert_eq!(key3_sent.2, expected_blue_jpeg, "key 3 received the latest image (blue)");

        // 2. Queue another frame for key 3 immediately (during the gap)
        let yellow = DynamicImage::ImageRgb8(image::RgbImage::from_pixel(KEY_PX, KEY_PX, image::Rgb([255, 255, 0])));
        rt.send(3, yellow.clone(), "c");

        // Try to process batch immediately: last_batch.elapsed() < gap
        process_paints(&rx, &rt, &mut latest, &mut pending, &mut last_batch, gap, &mut writer, Instant::now()).unwrap();
        assert_eq!(writer.flushes.len(), 1, "no new batch sent before gap elapsed");

        // Sleep until gap elapses
        std::thread::sleep(gap + Duration::from_millis(10));
        process_paints(&rx, &rt, &mut latest, &mut pending, &mut last_batch, gap, &mut writer, Instant::now()).unwrap();

        assert_eq!(writer.flushes.len(), 2, "second batch sent after gap elapsed");
        assert!(writer.flushes[1].0.duration_since(writer.flushes[0].0) >= gap, "batches stay separated by the gap");

        let key3_second_sent = writer.keys_sent.last().expect("second key sent");
        assert_eq!(key3_second_sent.1, 3);
        assert_eq!(key3_second_sent.2, render::key_jpeg(&yellow, 3));
    }

    #[test]
    fn reader_thread_delivers_events_without_polling_delay() {
        let dir = tmp("reader-latency");
        let (rt, rx) = runtime(&dir);
        let (_reload_tx, reload_rx) = mpsc::channel();
        let (msg_tx, msg_rx) = mpsc::channel();
        let (event_tx, event_rx) = mpsc::channel();
        let host = host::Host::start(host::Config { port: 0, ..Default::default() }, |_| {}).unwrap();
        let mut by_key = HashMap::from([(2u8, Target::Builtin("CTX2".into()))]);
        let mut latest = HashMap::new();
        let mut pending = HashMap::new();
        let mut running = HashSet::new();
        let builtins = HashSet::new();
        let scene_path = dir.join("A.sdProfile");
        std::fs::create_dir_all(&scene_path).unwrap();
        std::fs::write(scene_path.join("manifest.json"), r#"{"DeviceUUID":"AKP153E","Actions":{}}"#).unwrap();
        let mut scenes = SceneSet::open(&scene_path, &dir, &dir, DEVICE).unwrap();
        let mut writer = FakeWriter { n: 64, ..Default::default() };
        let gap = Duration::from_secs(2);

        let worker = std::thread::spawn(move || {
            session_loop_threaded(
                &event_rx,
                &mut writer,
                &rx,
                &reload_rx,
                &mut latest,
                &mut by_key,
                &host,
                &msg_tx,
                &rt,
                &mut scenes,
                Path::new("."),
                &[],
                &mut running,
                &builtins,
                &mut pending,
                gap,
                Instant::now(),
                Instant::now(),
                Instant::now(),
            )
        });

        // Send a key press event packet (key 2)
        let mut press_buf = vec![0u8; 64];
        press_buf[9] = 2;
        press_buf[10] = 1;

        let t0 = Instant::now();
        event_tx.send(ReaderMsg::Data(press_buf)).unwrap();
        let msg = msg_rx.recv_timeout(Duration::from_millis(250)).expect("received within 250 ms");
        let elapsed = t0.elapsed();
        assert!(elapsed < Duration::from_millis(250), "delivered promptly via event channel ({elapsed:?})");
        match msg {
            BuiltinMsg::Press(ctx) => assert_eq!(ctx, "CTX2"),
            _ => panic!("unexpected msg"),
        }

        // Stop the worker thread with an error (simulating unplug)
        event_tx.send(ReaderMsg::Error("device unplugged".into())).unwrap();
        let res = worker.join().expect("thread join");
        assert_eq!(res, Err("device unplugged".to_string()), "reader exit on error triggers reconnect path");
    }

    #[test]
    fn reader_thread_env_switch() {
        std::env::remove_var("ORCADECK_HID_READER");
        assert!(!hid_reader_mode(), "default OFF");

        std::env::set_var("ORCADECK_HID_READER", "0");
        assert!(!hid_reader_mode(), "non-thread is OFF");

        std::env::set_var("ORCADECK_HID_READER", "thread");
        assert!(hid_reader_mode(), "thread is ON");

        std::env::set_var("ORCADECK_HID_READER", "THREAD");
        assert!(hid_reader_mode(), "case-insensitive THREAD is ON");

        std::env::remove_var("ORCADECK_HID_READER");
    }

    #[test]
    fn batch_gap_defaults_to_250_and_honours_env() {
        std::env::remove_var("ORCADECK_GAP_MS");
        assert_eq!(batch_gap(), Duration::from_millis(BATCH_GAP_MS));
        assert_eq!(BATCH_GAP_MS, 250);

        std::env::set_var("ORCADECK_GAP_MS", "400");
        assert_eq!(batch_gap(), Duration::from_millis(400));

        std::env::remove_var("ORCADECK_GAP_MS");
    }

    #[test]
    fn threaded_loop_wakes_on_runtime_send_and_reload() {
        let dir = tmp("threaded-wake");
        let (reload_tx, reload_rx) = mpsc::channel();
        let (rt, rx) = runtime_with_reload(&dir, reload_tx);
        let (msg_tx, _msg_rx) = mpsc::channel();
        let (event_tx, event_rx) = mpsc::channel();
        rt.set_wake_tx(Some(event_tx.clone()));

        let scene_path = dir.join("A.sdProfile");
        std::fs::create_dir_all(&scene_path).unwrap();
        std::fs::write(scene_path.join("manifest.json"), r#"{"DeviceUUID":"AKP153E","Actions":{"0,0":{"UUID":"builtin","States":[{"Image":""}]}}}"#).unwrap();
        let mut scenes = SceneSet::open(&scene_path, &dir, &dir, DEVICE).unwrap();
        rt.set_manifest(scene_path.join("manifest.json"));
        let rt = Arc::new(rt);

        let host = host::Host::start(host::Config { port: 0, ..Default::default() }, |_| {}).unwrap();
        let mut by_key = HashMap::new();
        let mut latest = HashMap::new();
        let mut pending = HashMap::new();
        let mut running = HashSet::new();
        let builtins = HashSet::new();
        let fake = Arc::new(Mutex::new(FakeWriter { n: 64, ..Default::default() }));
        let fake_clone = Arc::clone(&fake);
        let gap = Duration::from_millis(50);

        struct SharedWriter(Arc<Mutex<FakeWriter>>);
        impl DockWriter for SharedWriter {
            fn report_n(&self) -> usize { self.0.lock().unwrap().report_n() }
            fn send(&mut self, payload: &[u8]) -> Result<(), String> { self.0.lock().unwrap().send(payload) }
            fn set_key(&mut self, key: u8, jpeg: &[u8]) -> Result<(), String> { self.0.lock().unwrap().set_key(key, jpeg) }
            fn flush(&mut self) -> Result<(), String> { self.0.lock().unwrap().flush() }
        }

        let mut writer = SharedWriter(fake_clone);
        let rt_worker = Arc::clone(&rt);
        let worker = std::thread::spawn(move || {
            session_loop_threaded(
                &event_rx,
                &mut writer,
                &rx,
                &reload_rx,
                &mut latest,
                &mut by_key,
                &host,
                &msg_tx,
                &rt_worker,
                &mut scenes,
                Path::new("."),
                &[],
                &mut running,
                &builtins,
                &mut pending,
                gap,
                Instant::now() - gap,
                Instant::now(),
                Instant::now(),
            )
        });

        // Loop is idle (waiting on event_rx with up to 10s heartbeat).
        // 1. Test paint wake: a Runtime::send paint must be flushed within gap + 50 ms.
        let t0 = Instant::now();
        rt.send(1, white(KEY_PX), "c");

        let deadline = t0 + gap + Duration::from_millis(50);
        let mut flushed = false;
        while Instant::now() < deadline {
            if fake.lock().unwrap().flushes.len() >= 1 {
                flushed = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        let elapsed = t0.elapsed();
        assert!(flushed, "paint flushed within gap + 50 ms (took {elapsed:?}, heartbeat is 10s)");
        assert!(elapsed < Duration::from_millis(150), "woke immediately without waiting for 10s heartbeat");

        // Wait gap before testing reload so batch gap doesn't hold back the reload paint
        std::thread::sleep(gap);

        // 2. Test reload wake: a reload is handled promptly
        let prev_flushes = fake.lock().unwrap().flushes.len();
        let t_reload = Instant::now();
        rt.reload("A", (0, 0));

        let reload_deadline = t_reload + gap + Duration::from_millis(50);
        let mut reloaded = false;
        while Instant::now() < reload_deadline {
            if fake.lock().unwrap().flushes.len() > prev_flushes {
                reloaded = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        let reload_elapsed = t_reload.elapsed();
        assert!(reloaded, "reload caused repaint and flushed within gap + 50 ms (took {reload_elapsed:?})");
        assert!(reload_elapsed < Duration::from_millis(150), "reload woke immediately without waiting for 10s heartbeat");

        // Stop loop
        event_tx.send(ReaderMsg::Error("done".into())).unwrap();
        worker.join().unwrap().unwrap_err();
    }

    #[test]
    fn stale_paint_after_switch_dropped_and_only_new_image_written() {
        let dir = tmp("gap1-stale-drop");
        let (rt, rx) = runtime(&dir);
        let (msg_tx, _msg_rx) = mpsc::channel();
        let host = host::Host::start(host::Config { port: 0, ..Default::default() }, |_| {}).unwrap();
        let builtins = HashSet::new();
        let mut running = HashSet::new();

        let scene_a = dir.join("A.sdProfile");
        let scene_b = dir.join("B.sdProfile");
        std::fs::create_dir_all(&scene_a).unwrap();
        std::fs::create_dir_all(&scene_b).unwrap();
        std::fs::write(scene_a.join("manifest.json"), r#"{"DeviceUUID":"AKP153E","Actions":{}}"#).unwrap();
        std::fs::write(scene_b.join("manifest.json"), r#"{"DeviceUUID":"AKP153E","Actions":{}}"#).unwrap();

        let mut scenes = SceneSet::open(&scene_a, &dir, &dir, DEVICE).unwrap();
        let mut by_key = HashMap::from([(1u8, Target::Plugin("CTX_OLD".into()))]);
        let mut latest = HashMap::new();
        let mut pending = HashMap::new();
        let gap = Duration::from_millis(10);
        let mut last_batch = Instant::now() - gap;
        let mut writer = FakeWriter { n: 64, ..Default::default() };

        // 1. Old scene: queue paint for key 1
        let red = DynamicImage::ImageRgb8(image::RgbImage::from_pixel(KEY_PX, KEY_PX, image::Rgb([255, 0, 0])));
        rt.send(1, red, "CTX_OLD");

        // 2. Run real switch_scene to scene B
        let _paints = switch_scene(
            &mut scenes,
            &SceneTarget::Id("B".into()),
            Path::new("."),
            &mut by_key,
            &mut latest,
            &host,
            &[],
            &mut running,
            &msg_tx,
            &rt,
            &builtins,
        ).expect("switch_scene to B succeeds");

        assert_eq!(rt.gen.load(Ordering::Relaxed), 1);
        assert!(rt.retired("CTX_OLD"));

        // 3. Queue a new-scene paint for the same key (key 1) with blue image
        let blue = DynamicImage::ImageRgb8(image::RgbImage::from_pixel(KEY_PX, KEY_PX, image::Rgb([0, 0, 255])));
        rt.send(1, blue.clone(), "CTX_NEW");

        // Late paint from old plugin context (simulating delayed thread)
        let yellow = DynamicImage::ImageRgb8(image::RgbImage::from_pixel(KEY_PX, KEY_PX, image::Rgb([255, 255, 0])));
        rt.send(1, yellow, "CTX_OLD");

        // 4. Run process_paints
        process_paints(&rx, &rt, &mut latest, &mut pending, &mut last_batch, gap, &mut writer, Instant::now()).unwrap();

        // 5. Verification:
        // Exactly one image written for key 1, matching the blue image from new scene
        let key1_sends: Vec<_> = writer.keys_sent.iter().filter(|(_, k, _)| *k == 1).collect();
        assert_eq!(key1_sends.len(), 1, "only one image written for key 1");
        assert_eq!(key1_sends[0].2, render::key_jpeg(&blue, 1), "only the new scene image (blue) was written");
        assert!(pending.is_empty(), "pending holds no stale entry");
        assert_eq!(latest.get(&1), Some(&blue), "latest reflects the new scene image");
    }

    #[test]
    fn stale_pending_image_purged_by_retain_filter_during_race() {
        let dir = tmp("gap1-pending-purge");
        let (rt, rx) = runtime(&dir);
        let mut latest = HashMap::new();
        let mut pending = HashMap::new();
        let gap = Duration::from_millis(50);
        let mut last_batch = Instant::now();
        let mut writer = FakeWriter { n: 64, ..Default::default() };

        // 1. Paint arrives at gen 0 for key 3 (red image)
        let red = DynamicImage::ImageRgb8(image::RgbImage::from_pixel(KEY_PX, KEY_PX, image::Rgb([255, 0, 0])));
        rt.send(3, red, "CTX_A");

        // Call process_paints while last_batch.elapsed() < gap:
        // Key 3 is received and inserted into `pending` with gen 0, but not flushed!
        process_paints(&rx, &rt, &mut latest, &mut pending, &mut last_batch, gap, &mut writer, Instant::now()).unwrap();
        assert_eq!(pending.len(), 1, "pending holds key 3 at gen 0");
        assert_eq!(pending.get(&3).map(|(_, g)| *g), Some(0));
        assert_eq!(writer.keys_sent.len(), 0, "nothing flushed yet due to gap");

        // 2. Scene switch occurs while pending already holds gen 0 image:
        // Runtime::gen is bumped to 1
        rt.gen.fetch_add(1, Ordering::SeqCst);

        // 3. Gap elapses without any new paint for key 3 arriving, but key 4 arrives at gen 1
        last_batch = Instant::now() - gap - Duration::from_millis(5);

        let white_img = white(KEY_PX);
        rt.send(4, white_img.clone(), "CTX_B");

        // 4. Run process_paints:
        // pending.retain(|_, (_, g)| *g >= gen) must purge key 3 (g=0 < gen=1)
        // and flush key 4 (g=1 >= gen=1)
        process_paints(&rx, &rt, &mut latest, &mut pending, &mut last_batch, gap, &mut writer, Instant::now()).unwrap();

        // 5. Verify:
        // Key 3 was purged by pending.retain and NEVER written to writer
        assert!(writer.keys_sent.iter().all(|(_, k, _)| *k != 3), "stale gen 0 key 3 was never written");
        let key4_sent = writer.keys_sent.iter().find(|(_, k, _)| *k == 4);
        assert!(key4_sent.is_some(), "key 4 at gen 1 was written");
        assert_eq!(key4_sent.unwrap().2, render::key_jpeg(&white_img, 4));
        assert!(pending.is_empty(), "pending is completely drained and has no stale entries");
    }

    #[test]
    fn reader_error_ends_threaded_session_loop_promptly() {
        let dir = tmp("gap2-reader-err");
        let (rt, rx) = runtime(&dir);
        let (_reload_tx, reload_rx) = mpsc::channel();
        let (msg_tx, _msg_rx) = mpsc::channel();
        let (event_tx, event_rx) = mpsc::channel();
        let host = host::Host::start(host::Config { port: 0, ..Default::default() }, |_| {}).unwrap();
        let mut by_key = HashMap::new();
        let mut latest = HashMap::new();
        let mut pending = HashMap::new();
        let mut running = HashSet::new();
        let builtins = HashSet::new();
        let scene_path = dir.join("A.sdProfile");
        std::fs::create_dir_all(&scene_path).unwrap();
        std::fs::write(scene_path.join("manifest.json"), r#"{"DeviceUUID":"AKP153E","Actions":{}}"#).unwrap();
        let mut scenes = SceneSet::open(&scene_path, &dir, &dir, DEVICE).unwrap();
        let mut writer = FakeWriter { n: 64, ..Default::default() };
        let gap = Duration::from_secs(5);

        let worker = std::thread::spawn(move || {
            session_loop_threaded(
                &event_rx,
                &mut writer,
                &rx,
                &reload_rx,
                &mut latest,
                &mut by_key,
                &host,
                &msg_tx,
                &rt,
                &mut scenes,
                Path::new("."),
                &[],
                &mut running,
                &builtins,
                &mut pending,
                gap,
                Instant::now(),
                Instant::now(),
                Instant::now(),
            )
        });

        let t0 = Instant::now();
        event_tx.send(ReaderMsg::Error("usb read failed: device detached".into())).unwrap();

        let res = worker.join().expect("session_loop_threaded terminates promptly");
        let elapsed = t0.elapsed();
        assert!(elapsed < Duration::from_millis(250), "terminated promptly on ReaderMsg::Error ({elapsed:?})");
        assert_eq!(res, Err("usb read failed: device detached".to_string()));
    }

    #[test]
    fn writer_error_propagates_from_process_paints_without_panic_and_preserves_images_in_latest() {
        let dir = tmp("gap2-writer-err");
        let (rt, rx) = runtime(&dir);
        let mut latest = HashMap::new();
        let mut pending = HashMap::new();
        let gap = Duration::from_millis(10);
        let mut last_batch = Instant::now() - gap;

        let mut failing_writer = FakeWriter {
            n: 64,
            fail_set_key_on: Some(2),
            ..Default::default()
        };

        let img1 = white(KEY_PX);
        let img2 = DynamicImage::ImageRgb8(image::RgbImage::from_pixel(KEY_PX, KEY_PX, image::Rgb([255, 0, 0])));
        let img3 = DynamicImage::ImageRgb8(image::RgbImage::from_pixel(KEY_PX, KEY_PX, image::Rgb([0, 255, 0])));

        rt.send(1, img1.clone(), "ctx");
        rt.send(2, img2.clone(), "ctx");
        rt.send(3, img3.clone(), "ctx");

        let res = process_paints(
            &rx,
            &rt,
            &mut latest,
            &mut pending,
            &mut last_batch,
            gap,
            &mut failing_writer,
            Instant::now(),
        );

        assert!(res.is_err(), "writer failure must surface as Err");
        let err_msg = res.unwrap_err();
        assert!(err_msg.contains("unplugged during set_key for key 2"), "error preserved: {err_msg}");

        // Images are NOT lost: `latest` holds all queued images
        assert_eq!(latest.get(&1), Some(&img1));
        assert_eq!(latest.get(&2), Some(&img2));
        assert_eq!(latest.get(&3), Some(&img3));

        // Reconnect path parity: pending is re-seeded from latest
        let gen = rt.gen.load(Ordering::Relaxed);
        let mut reconnected_pending: HashMap<u8, (DynamicImage, u64)> =
            latest.iter().map(|(&k, img)| (k, (img.clone(), gen))).collect();

        assert_eq!(reconnected_pending.len(), 3, "all 3 images re-queued for reconnect");

        let mut healthy_writer = FakeWriter { n: 64, ..Default::default() };
        let mut reconnect_batch = Instant::now() - gap;
        process_paints(
            &rx,
            &rt,
            &mut latest,
            &mut reconnected_pending,
            &mut reconnect_batch,
            gap,
            &mut healthy_writer,
            Instant::now(),
        ).expect("reconnected paint succeeds");

        assert_eq!(healthy_writer.flushes.len(), 1);
        assert_eq!(healthy_writer.keys_sent.len(), 3);
        assert!(reconnected_pending.is_empty());
    }

    #[test]
    fn writer_error_propagates_from_session_loop_threaded() {
        let dir = tmp("gap2-loop-writer-err");
        let (rt, rx) = runtime(&dir);
        let (_reload_tx, reload_rx) = mpsc::channel();
        let (msg_tx, _msg_rx) = mpsc::channel();
        let (_event_tx, event_rx) = mpsc::channel();
        let host = host::Host::start(host::Config { port: 0, ..Default::default() }, |_| {}).unwrap();
        let mut by_key = HashMap::new();
        let mut latest = HashMap::new();
        let mut pending = HashMap::new();
        let mut running = HashSet::new();
        let builtins = HashSet::new();
        let scene_path = dir.join("A.sdProfile");
        std::fs::create_dir_all(&scene_path).unwrap();
        std::fs::write(scene_path.join("manifest.json"), r#"{"DeviceUUID":"AKP153E","Actions":{}}"#).unwrap();
        let mut scenes = SceneSet::open(&scene_path, &dir, &dir, DEVICE).unwrap();

        let mut failing_writer = FakeWriter {
            n: 64,
            fail_set_key_on: Some(1),
            ..Default::default()
        };
        let gap = Duration::from_millis(10);

        rt.send(1, white(KEY_PX), "c");

        let res = session_loop_threaded(
            &event_rx,
            &mut failing_writer,
            &rx,
            &reload_rx,
            &mut latest,
            &mut by_key,
            &host,
            &msg_tx,
            &rt,
            &mut scenes,
            Path::new("."),
            &[],
            &mut running,
            &builtins,
            &mut pending,
            gap,
            Instant::now() - gap,
            Instant::now(),
            Instant::now(),
        );

        assert!(res.is_err(), "writer failure terminates session loop with Err");
        let err = res.unwrap_err();
        assert!(err.contains("unplugged during set_key for key 1"));
    }
}
