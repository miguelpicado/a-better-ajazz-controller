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
    load_img,
    profile::{self, Scene},
    proto::*,
    render, Dock,
};
use base64::Engine;
use hidapi::HidApi;
use image::{imageops, DynamicImage};
use orcadeck_builtin as bi;
use orcadeck_host as host;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{mpsc, Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

/// Minimum time between paint batches (all pending keys go out together). The firmware misses key
/// presses while busy with images, so paint traffic must leave idle gaps. ORCADECK_GAP_MS tunes it.
fn batch_gap() -> Duration {
    Duration::from_millis(std::env::var("ORCADECK_GAP_MS").ok().and_then(|v| v.parse().ok()).unwrap_or(400))
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
        image::load_from_memory(&bytes).ok()
    }
}

const SANS: &str = r#"font-family="Segoe UI, Arial, sans-serif""#;
/// How long an ok/alert overlay stays before the current image is sent again.
const FEEDBACK_MS: u64 = 600;

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
    /// Base image with the title bar drawn on it (black if the key has no image yet).
    fn effective(&self, key: u8) -> DynamicImage {
        let px = image_px(key);
        let base = self.base.get(&key).cloned().unwrap_or_else(|| DynamicImage::new_rgb8(px, px));
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

/// Turns one plugin output into at most one image on the shared paint channel. Called on socket
/// threads; the HID stays with the main loop.
fn handle_output(tx: &mpsc::Sender<(u8, DynamicImage)>, view: &View, out: host::Output) {
    let Some(k) = out_key(&out).and_then(|key| key_index(key.column as u8, key.row as u8)) else { return };
    let (ok, px) = (matches!(&out, host::Output::ShowOk(_)), image_px(k));
    let mut v = view.lock().unwrap_or_else(|e| e.into_inner());
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
                v.base.insert(k, img);
                image = Some(v.effective(k));
            }
        }
        host::Output::ShowOk(_) | host::Output::ShowAlert(_) => {
            let base = v.effective(k);
            drop(v);
            feedback(tx, view, k, ok, base);
            return;
        }
        _ => {}
    }
    drop(v);
    if let Some(img) = image {
        let _ = tx.send((k, img));
    }
}

/// Two paints on the shared channel: the badge over `base`, then the key's current image.
fn feedback(tx: &mpsc::Sender<(u8, DynamicImage)>, view: &View, key: u8, ok: bool, base: DynamicImage) {
    let _ = tx.send((key, compose_feedback(&base, ok, image_px(key))));
    let (tx, view) = (tx.clone(), Arc::clone(view));
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(FEEDBACK_MS));
        let img = view.lock().unwrap_or_else(|e| e.into_inner()).effective(key);
        let _ = tx.send((key, img));
    });
}

/// Where a scene cell's action runs.
enum Target {
    Builtin(String),
    Plugin(String),
}

pub fn run(profile: &Path, plugins: &Path, n: Option<usize>) -> Result<(), String> {
    let scene = Scene::load(profile)?;
    let (tx, rx) = mpsc::channel::<(u8, DynamicImage)>();
    let (press_tx, press_rx) = mpsc::channel::<String>();

    // Last image, title and per-state images of every key. The host threads compose over this; the
    // main loop only ever receives the resulting image through the channel.
    let mut latest: HashMap<u8, DynamicImage> = HashMap::new();
    let mut view = ViewState::default();
    for ((c, r), a) in &scene.actions {
        let Some(key) = key_index(*c, *r) else { continue };
        if let Some(img) = scene.image_of(a, plugins).and_then(|p| load_img(&p).ok()) {
            latest.insert(key, img.clone()); // placeholder until the action paints
            view.base.insert(key, img);
        }
        view.states.insert(key, (0..a.states.len()).map(|i| state_path(&scene, a, plugins, i)).collect());
        view.state.insert(key, a.state as u32);
    }
    let view: View = Arc::new(Mutex::new(view));

    // Plugin host: only plugins the scene uses are launched.
    let data_root = PathBuf::from(std::env::var_os("APPDATA").unwrap_or_default()).join("OrcaDeck");
    let found = host::discover(&[plugins.to_path_buf()], Some(&data_root));
    let events_tx = tx.clone();
    let events_view = Arc::clone(&view);
    let host = host::Host::start(
        host::Config {
            devices: vec![host::Device { id: DEVICE.into(), name: "AKP153E".into(), columns: 5, rows: 3, kind: 0 }],
            ..Default::default()
        },
        move |out: host::Output| handle_output(&events_tx, &events_view, out),
    )
    .map_err(|e| format!("plugin host: {e}"))?;

    // Place every cell: built-in (daemon) or plugin (host).
    let mut registry = bi::Registry::new(bi::Deck::new(Box::new(bi::PipeOrca::from_env()), bi::Deck::default_quota_dir()));
    let mut by_key: HashMap<u8, Target> = HashMap::new();
    let mut ctx_key: HashMap<String, u8> = HashMap::new();
    let mut used: Vec<&host::Plugin> = Vec::new();
    for ((c, r), a) in &scene.actions {
        let Some(key) = key_index(*c, *r) else { continue };
        let ctx = format!("{c},{r}");
        let bk = bi::Key { uuid: a.uuid.clone(), context: ctx.clone(), device: DEVICE.into(), row: *r as u32, col: *c as u32 };
        if registry.appear(bk) {
            ctx_key.insert(ctx.clone(), key);
            by_key.insert(key, Target::Builtin(ctx));
        } else if let Some(p) = found.iter().filter(|p| a.uuid.starts_with(&format!("{}.", p.id))).max_by_key(|p| p.id.len()) {
            let mut inst = host::Instance::new(&p.id, &a.uuid, DEVICE, *c as u32, *r as u32);
            inst.settings = a.settings.clone();
            inst.state = a.state as u32;
            by_key.insert(key, Target::Plugin(host.appear(inst)));
            if !used.iter().any(|u| u.id == p.id) {
                used.push(p);
            }
        } else {
            eprintln!("no handler for {} at {c},{r} ({})", a.uuid, a.name);
        }
    }
    for p in used {
        match host.launch(p.clone()) {
            Ok(()) => eprintln!("plugin {} started", p.id),
            Err(e) => eprintln!("plugin {}: {e}", p.id),
        }
    }

    // Built-in actions on their own thread: Orca RPC blocks for ~450 ms now and then.
    let builtin_tx = tx.clone();
    let builtin_view = Arc::clone(&view);
    thread::Builder::new()
        .name("builtin".into())
        .spawn(move || loop {
            for (ctx, svg) in registry.tick(bi::now_ms()) {
                let Some(&k) = ctx_key.get(&ctx) else { continue };
                if let Some(img) = svg_image(&svg, image_px(k)) {
                    builtin_view.lock().unwrap_or_else(|e| e.into_inner()).base.insert(k, img.clone());
                    let _ = builtin_tx.send((k, img));
                }
            }
            match press_rx.recv_timeout(Duration::from_millis(400)) {
                Ok(ctx) => {
                    let fb = registry.key_down(&ctx, bi::now_ms());
                    if fb != bi::Feedback::None {
                        if let Some(&k) = ctx_key.get(&ctx) {
                            let base = builtin_view.lock().unwrap_or_else(|e| e.into_inner()).effective(k);
                            feedback(&builtin_tx, &builtin_view, k, fb == bi::Feedback::Ok, base);
                        }
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => return,
            }
        })
        .map_err(|e| e.to_string())?;
    drop(tx);

    // Main loop: reconnect forever; `latest` repaints the device after a re-plug.
    let black = DynamicImage::new_rgb8(KEY_PX, KEY_PX);
    for k in 1..=15u8 {
        latest.entry(k).or_insert_with(|| black.clone());
    }
    loop {
        match session(n, &rx, &mut latest, &by_key, &host, &press_tx) {
            Ok(()) => return Ok(()),
            Err(e) => {
                eprintln!("device: {e}; retrying in 2 s");
                thread::sleep(Duration::from_secs(2));
            }
        }
    }
}

/// One connection to the device. Returns Err when it is lost or cannot be opened.
fn session(
    n: Option<usize>,
    rx: &mpsc::Receiver<(u8, DynamicImage)>,
    latest: &mut HashMap<u8, DynamicImage>,
    by_key: &HashMap<u8, Target>,
    host: &host::Host,
    press_tx: &mpsc::Sender<String>,
) -> Result<(), String> {
    let api = HidApi::new().map_err(|e| e.to_string())?;
    let dock = Dock::open(&api, n)?;
    dock.init(80)?;
    eprintln!("device ready, N = {}", dock.n);
    let mut pending: HashMap<u8, DynamicImage> = latest.clone();
    let gap = batch_gap();
    let mut last_batch = Instant::now() - gap;
    let mut hb = Instant::now();
    let t0 = Instant::now();
    let mut buf = vec![0u8; dock.n + 1];
    loop {
        let k = dock.dev.read_timeout(&mut buf, 30).map_err(|e| e.to_string())?;
        if k > 0 {
            if std::env::var_os("ORCADECK_TRACE").is_some() {
                eprintln!("{:>6}ms in {:02x?}", t0.elapsed().as_millis(), &buf[..k.min(16)]);
            }
            match parse_input(&buf[..k]) {
                Input::Handshake => dock.send(&handshake_reply(dock.n))?,
                Input::Key { key, .. } if (1..=15).contains(&key) => match by_key.get(&key) {
                    Some(Target::Builtin(ctx)) => {
                        let _ = press_tx.send(ctx.clone());
                    }
                    Some(Target::Plugin(ctx)) => {
                        host.key(ctx, true);
                        host.key(ctx, false);
                    }
                    None => {}
                },
                _ => {}
            }
        }
        while let Ok((key, img)) = rx.try_recv() {
            latest.insert(key, img.clone());
            pending.insert(key, img);
        }
        if !pending.is_empty() && last_batch.elapsed() >= gap {
            let (started, count) = (Instant::now(), pending.len());
            let mut bytes = 0;
            for (key, img) in pending.drain() {
                let j = render::key_jpeg(&img, key);
                bytes += j.len();
                dock.set_key(key, &j)?;
            }
            dock.flush()?;
            last_batch = Instant::now();
            if std::env::var_os("ORCADECK_TRACE").is_some() {
                eprintln!("{:>6}ms painted {count} keys {bytes} B in {} ms", t0.elapsed().as_millis(), started.elapsed().as_millis());
            }
        }
        if hb.elapsed() >= Duration::from_secs(HEARTBEAT_SECS) {
            dock.send(&heartbeat(dock.n))?;
            hb = Instant::now();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn white(px: u32) -> DynamicImage {
        DynamicImage::ImageRgb8(image::RgbImage::from_pixel(px, px, image::Rgb([255, 255, 255])))
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
}
