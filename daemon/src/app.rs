//! `orcadeck run <profile>`: HID driver + plugin host + built-in Orca Deck actions in one process.
//!
//! Threads: main (owns the HID device: input, paint, heartbeat), `builtin` (owns the `Registry`,
//! ticks every 400 ms), host accept/socket threads (plugins). Everything that paints sends
//! `(key, image)` over one channel; the main loop keeps the latest image per key and writes it at most
//! as one batch at most once per `batch_gap()`, so a chatty plugin cannot flood the device.
//! The AKP153E reports a press only on release, so a press is delivered as a click (down + up).
use crate::{load_img, profile::Scene, proto::*, render, Dock};
use base64::Engine;
use hidapi::HidApi;
use image::DynamicImage;
use orcadeck_builtin as bi;
use orcadeck_host as host;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::mpsc,
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

/// Where a scene cell's action runs.
enum Target {
    Builtin(String),
    Plugin(String),
}

pub fn run(profile: &Path, plugins: &Path, n: Option<usize>) -> Result<(), String> {
    let scene = Scene::load(profile)?;
    let (tx, rx) = mpsc::channel::<(u8, DynamicImage)>();
    let (press_tx, press_rx) = mpsc::channel::<String>();

    // Plugin host: only plugins the scene uses are launched.
    let data_root = PathBuf::from(std::env::var_os("APPDATA").unwrap_or_default()).join("OrcaDeck");
    let found = host::discover(&[plugins.to_path_buf()], Some(&data_root));
    let events_tx = tx.clone();
    let host = host::Host::start(
        host::Config {
            devices: vec![host::Device { id: DEVICE.into(), name: "AKP153E".into(), columns: 5, rows: 3, kind: 0 }],
            ..Default::default()
        },
        move |out: host::Output| {
            if let host::Output::Image { key, image, .. } = out {
                let Some(k) = key_index(key.column as u8, key.row as u8) else { return };
                if let Some(img) = decode_uri(&image, image_px(k)) {
                    let _ = events_tx.send((k, img));
                }
                // ponytail: titles, states and showOk/showAlert are ignored; add when a plugin needs them.
            }
        },
    )
    .map_err(|e| format!("plugin host: {e}"))?;

    // Place every cell: built-in (daemon) or plugin (host).
    let mut registry = bi::Registry::new(bi::Deck::new(Box::new(bi::PipeOrca::from_env()), bi::Deck::default_quota_dir()));
    let mut by_key: HashMap<u8, Target> = HashMap::new();
    let mut ctx_key: HashMap<String, u8> = HashMap::new();
    let mut latest: HashMap<u8, DynamicImage> = HashMap::new();
    let mut used: Vec<&host::Plugin> = Vec::new();
    for ((c, r), a) in &scene.actions {
        let Some(key) = key_index(*c, *r) else { continue };
        if let Some(img) = scene.image_of(a, plugins).and_then(|p| load_img(&p).ok()) {
            latest.insert(key, img); // placeholder until the action paints
        }
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
    thread::Builder::new()
        .name("builtin".into())
        .spawn(move || loop {
            for (ctx, svg) in registry.tick(bi::now_ms()) {
                let Some(&k) = ctx_key.get(&ctx) else { continue };
                if let Some(img) = svg_image(&svg, image_px(k)) {
                    let _ = builtin_tx.send((k, img));
                }
            }
            match press_rx.recv_timeout(Duration::from_millis(400)) {
                Ok(ctx) => {
                    let fb = registry.key_down(&ctx, bi::now_ms());
                    eprintln!("press {ctx}: {fb:?}"); // ponytail: Ok/Alert feedback blink not drawn yet
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
