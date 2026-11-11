mod app;
mod ctl;
mod edit;
mod profile;
mod proto;
mod register;
mod render;
mod scenes;

use hidapi::{HidApi, HidDevice};
use proto::*;
use std::{
    path::Path,
    time::{Duration, Instant},
};

struct Dock {
    dev: HidDevice,
    n: usize,
}

impl Dock {
    fn open(api: &HidApi, n_override: Option<usize>) -> Result<Dock, String> {
        let mut cands: Vec<_> = api
            .device_list()
            .filter(|d| d.vendor_id() == VID && d.product_id() == PID)
            .collect();
        // prefer vendor usage page (INFERRED, see hid-protocol.md §0)
        cands.sort_by_key(|d| if d.usage_page() >= 0xFF00 { 0 } else { 1 });
        let info = cands
            .first()
            .ok_or("device not found (monitors on? Stream Dock closed?)")?;
        let dev = info.open_device(api).map_err(|e| e.to_string())?;
        let n = n_override.or_else(|| {
            let mut buf = [0u8; 4096];
            let len = dev.get_report_descriptor(&mut buf).ok()?;
            output_len_from_descriptor(&buf[..len])
        });
        let n = n.unwrap_or(512);
        Ok(Dock { dev, n })
    }

    fn send(&self, payload: &[u8]) -> Result<(), String> {
        self.dev
            .write(&wire(payload))
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    fn init(&self, bright: u8) -> Result<(), String> {
        self.send(&wake(self.n))?;
        self.send(&brightness(self.n, bright))?;
        // sequence captured from the original app with Frida (2026-11-10): DIS, LIG, CLE ff, images, one STP.
        // The device sends no ACKs for these, so nothing is awaited.
        self.send(&clear_all(self.n))?;
        Ok(())
    }

    fn set_key(&self, key: u8, jpeg: &[u8]) -> Result<(), String> {
        for p in image_packets(self.n, key, jpeg) {
            self.send(&p)?;
        }
        Ok(())
    }

    fn flush(&self) -> Result<(), String> {
        self.send(&flush(self.n))
    }
}

fn tag(vid: u16, pid: u16) -> &'static str {
    match (vid, pid) {
        (0xEEEF, 0x2929) => "   <== AKP153E BOOTLOADER",
        (0x0300, 0x1010) => "   <== AKP153E app",
        _ => "",
    }
}

fn plugins_dir() -> std::path::PathBuf {
    std::env::var_os("ORCADECK_PLUGINS")
        .map(Into::into)
        .unwrap_or_else(|| {
            Path::new(&std::env::var_os("APPDATA").unwrap_or_default())
                .join("HotSpot/StreamDock/plugins")
        })
}

fn load_img(p: &Path) -> Result<image::DynamicImage, String> {
    image::open(p).map_err(|e| format!("{}: {e}", p.display()))
}

/// Scene -> (key, jpeg) list.
fn scene_jpegs(dir: &Path, plugins: &Path) -> Result<Vec<(u8, Vec<u8>)>, String> {
    let s = profile::Scene::load(dir)?;
    let mut out = vec![];
    for ((c, r), a) in &s.actions {
        let (Some(key), Some(path)) = (key_index(*c, *r), s.image_of(a, plugins)) else {
            continue;
        };
        match load_img(&path) {
            Ok(img) => out.push((key, render::key_jpeg(&img, key))),
            Err(e) => eprintln!("skip {c},{r} ({}): {e}", a.name),
        }
    }
    // keys without an action get black, otherwise the device's boot splash stays visible
    let black = image::DynamicImage::new_rgb8(KEY_PX, KEY_PX);
    for key in 1..=15u8 {
        if !out.iter().any(|(k, _)| *k == key) {
            out.push((key, render::key_jpeg(&black, key)));
        }
    }
    Ok(out)
}

fn run(args: &[String]) -> Result<(), String> {
    let a = |i: usize| args.get(i).map(String::as_str);
    let n_env = std::env::var("ORCADECK_REPORT_LEN")
        .ok()
        .and_then(|v| v.parse().ok());
    match a(0) {
        // offline: render a scene to JPEG files (no device needed)
        Some("render") => {
            let (dir, out) = (
                a(1).ok_or("render <sdProfile> <outDir>")?,
                a(2).ok_or("render <sdProfile> <outDir>")?,
            );
            std::fs::create_dir_all(out).map_err(|e| e.to_string())?;
            for (key, j) in scene_jpegs(Path::new(dir), &plugins_dir())? {
                std::fs::write(Path::new(out).join(format!("key{key:02}.jpg")), &j)
                    .map_err(|e| e.to_string())?;
                println!("key {key:2}: {} bytes", j.len());
            }
            Ok(())
        }
        // read-only: enumerate and show report descriptor / derived N. Sends nothing.
        Some("probe") => {
            let api = HidApi::new().map_err(|e| e.to_string())?;
            let mut found = false;
            for d in api
                .device_list()
                .filter(|d| d.vendor_id() == VID && d.product_id() == PID)
            {
                found = true;
                println!(
                    "{:?} usage_page={:#06x} usage={:#x} iface={} product={:?}",
                    d.path(),
                    d.usage_page(),
                    d.usage(),
                    d.interface_number(),
                    d.product_string()
                );
                if let Ok(dev) = d.open_device(&api) {
                    let mut buf = [0u8; 4096];
                    if let Ok(len) = dev.get_report_descriptor(&mut buf) {
                        println!("  descriptor ({len} B): {:02x?}", &buf[..len]);
                        println!(
                            "  output payload N = {:?}",
                            output_len_from_descriptor(&buf[..len])
                        );
                    }
                }
            }
            if !found {
                return Err("device not found".into());
            }
            Ok(())
        }
        // the product: driver + plugin host + built-in actions in one process
        Some("run") => app::run(
            Path::new(a(1).ok_or("run <sdProfile>")?),
            &plugins_dir(),
            n_env,
        ),
        Some("edit") => edit::run(&args[1..]),
        // Double click / shell: open the editor with this package staged, never install it directly.
        Some("install") => edit::run_install(a(1).ok_or("install <paquete>")?),
        // Per-user file association for .streamDeckPlugin/.sdPlugin (HKCU: no admin).
        Some("register") => register::register(),
        Some("unregister") => register::unregister(),
        Some("scene") => {
            let dir = a(1).ok_or("scene <sdProfile>")?;
            let jp = scene_jpegs(Path::new(dir), &plugins_dir())?;
            let api = HidApi::new().map_err(|e| e.to_string())?;
            let d = Dock::open(&api, n_env)?;
            println!("N = {}", d.n);
            d.init(80)?;
            for (k, j) in &jp {
                d.set_key(*k, j)?;
            }
            d.flush()
        }
        Some("bright") => {
            let api = HidApi::new().map_err(|e| e.to_string())?;
            let d = Dock::open(&api, n_env)?;
            d.send(&brightness(
                d.n,
                a(1).and_then(|v| v.parse().ok()).ok_or("bright <0-100>")?,
            ))
        }
        // read-only: only ENUMERATES HID devices (never opens or writes); prints arrivals/removals, flags the
        // AKP153E bootloader (EEEF:2929) so we can confirm when a future update enters it. Ctrl+C to stop.
        Some("hidwatch") => {
            let mut api = HidApi::new().map_err(|e| e.to_string())?;
            let t0 = Instant::now();
            let mut prev: std::collections::BTreeSet<(u16, u16, String)> = Default::default();
            println!("hidwatch: enumerating only (read-only). EEEF:2929 = AKP153E bootloader, 0300:1010 = AKP153E app");
            loop {
                api.refresh_devices().map_err(|e| e.to_string())?;
                let cur: std::collections::BTreeSet<_> = api
                    .device_list()
                    .map(|d| {
                        (
                            d.vendor_id(),
                            d.product_id(),
                            d.product_string().unwrap_or("").to_string(),
                        )
                    })
                    .collect();
                for d in cur.difference(&prev) {
                    println!(
                        "{:>7}ms + {:04x}:{:04x} {:?}{}",
                        t0.elapsed().as_millis(),
                        d.0,
                        d.1,
                        d.2,
                        tag(d.0, d.1)
                    );
                }
                for d in prev.difference(&cur) {
                    println!(
                        "{:>7}ms - {:04x}:{:04x} {:?}{}",
                        t0.elapsed().as_millis(),
                        d.0,
                        d.1,
                        d.2,
                        tag(d.0, d.1)
                    );
                }
                prev = cur;
                std::thread::sleep(Duration::from_millis(100));
            }
        }
        // poll GET_REPORT (input id 0) / feature reports and print when content changes (key-held state?)
        Some("poll") => {
            let api = HidApi::new().map_err(|e| e.to_string())?;
            let d = Dock::open(&api, n_env)?;
            d.init(80)?;
            let kind = a(1).unwrap_or("input");
            let mut prev: Vec<u8> = vec![];
            let t0 = Instant::now();
            println!("polling {kind} reports");
            loop {
                let mut b = vec![0u8; d.n + 1];
                let r = if kind == "feature" {
                    d.dev.get_feature_report(&mut b)
                } else {
                    d.dev.get_input_report(&mut b)
                };
                match r {
                    Ok(k) => {
                        let cur = b[..k.min(24)].to_vec();
                        if cur != prev {
                            println!("{:>6}ms n={k} {:02x?}", t0.elapsed().as_millis(), cur);
                            prev = cur;
                        }
                    }
                    Err(e) => {
                        println!("err {e}");
                        std::thread::sleep(Duration::from_millis(500));
                    }
                }
                std::thread::sleep(Duration::from_millis(30));
            }
        }
        Some("watch") => {
            let api = HidApi::new().map_err(|e| e.to_string())?;
            let d = Dock::open(&api, n_env)?;
            println!(
                "N = {}; press keys (Ctrl+C to stop); raw reports printed",
                d.n
            );
            d.init(80)?;
            if let Some(m) = a(1).and_then(|v| v.parse::<u8>().ok()) {
                println!("sending MOD {m}");
                d.send(&crt(d.n, "MOD", &[(0x0a, m)]))?;
            }
            // experiment hook: ORCADECK_CMD="VERB:0a=01,0b=00" (hex offsets/values)
            if let Ok(spec) = std::env::var("ORCADECK_CMD") {
                let (verb, rest) = spec.split_once(':').unwrap_or((&spec, ""));
                let extra: Vec<(usize, u8)> = rest
                    .split(',')
                    .filter_map(|kv| {
                        let (o, v) = kv.split_once('=')?;
                        Some((
                            usize::from_str_radix(o, 16).ok()?,
                            u8::from_str_radix(v, 16).ok()?,
                        ))
                    })
                    .collect();
                println!("sending {verb} {extra:02x?}");
                d.send(&crt(d.n, verb, &extra))?;
                // ajazz-pi (rev2) order: QUCMD, LIG, CLE, images + STP, CONNECT
                if let Ok(sc) = std::env::var("ORCADECK_SCENE") {
                    d.send(&brightness(d.n, 80))?;
                    d.send(&clear_all(d.n))?;
                    for (k, j) in &scene_jpegs(Path::new(&sc), &plugins_dir())? {
                        d.set_key(*k, j)?;
                        d.flush()?;
                    }
                    d.send(&heartbeat(d.n))?;
                }
            }
            let mut b = vec![0u8; d.n + 1];
            let mut last_hb = Instant::now();
            loop {
                if last_hb.elapsed() >= Duration::from_secs(HEARTBEAT_SECS) {
                    d.send(&heartbeat(d.n))?;
                    last_hb = Instant::now();
                }
                let k = d.dev.read_timeout(&mut b, 500).map_err(|e| e.to_string())?;
                if k == 0 {
                    continue;
                }
                let inp = parse_input(&b[..k]);
                if inp == Input::Handshake {
                    d.send(&handshake_reply(d.n))?;
                }
                println!("{inp:?}  raw[..16]={:02x?}", &b[..k.min(16)]);
            }
        }
        Some("help") | Some("-h") | Some("--help") => {
            println!("{USAGE}");
            Ok(())
        }
        _ => Err(USAGE.into()),
    }
}

const USAGE: &str = "\
uso: orcadeck <comando>

  run <perfil>        driver HID + host de plugins + acciones integradas
  edit [--port N] [--no-open]   editor web (dentro de run si está vivo)
  install <paquete>   abre el editor para instalar un .streamDeckPlugin/.sdPlugin/.zip
  register            asocia esos ficheros a OrcaDeck para este usuario (sin admin)
  unregister          deshace la asociación y restaura la anterior
  render <perfil> <outDir>      escena a JPEG (sin dispositivo)
  scene <perfil> | bright <0-100> | watch | probe | poll | hidwatch

Detalle: docs/guide/USAGE.md";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Err(e) = run(&args) {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}
