use orcadeck_web::{FsStore, Server, Store};
use serde_json::Value;
use std::{
    io::{Read, Write},
    net::TcpStream,
    path::PathBuf,
    sync::Arc,
};

const ORCA_ID: &str = "R0KZ54Z0-063U-947X-31BI-GG95R8F4HT88";
const MON_ID: &str = "3I0U64IN-IT4R-RUB6-EZ4Z-VXS311JFXI05";
const REC_ID: &str = "H33G4VTZ-25Z8-PYN7-R34M-ST39SX8TRT2Q";

fn get_real_paths() -> Option<(PathBuf, PathBuf, Vec<PathBuf>, Option<PathBuf>)> {
    let appdata = std::env::var_os("APPDATA").map(PathBuf::from)?;
    let hotspot = appdata.join("HotSpot/StreamDock");
    if !hotspot.join("profiles").is_dir() {
        return None;
    }
    let profiles = hotspot.join("profiles");
    let plugins = hotspot.join("plugins");
    let mut builtin = Vec::new();
    let prog = PathBuf::from(r"C:\Program Files (x86)\Stream Dock AJAZZ Global\plugins");
    if prog.is_dir() {
        builtin.push(prog);
    }
    let config = hotspot.join("config");
    let legacy = if config.is_dir() { Some(config) } else { None };
    Some((profiles, plugins, builtin, legacy))
}

#[test]
fn test_real_folders_read_only() {
    let (profiles, plugins, builtin, legacy) = match get_real_paths() {
        Some(p) => p,
        None => {
            eprintln!("Real StreamDock folders not found; skipping read-only real data test.");
            return;
        }
    };

    let temp_data = std::env::temp_dir().join(format!("orcadeck-web-test-data-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&temp_data);
    std::fs::create_dir_all(&temp_data).unwrap();

    let store = FsStore {
        profiles,
        plugins,
        builtin_plugins: builtin,
        data: temp_data.clone(),
        legacy_config: legacy,
        downloads: None,
        inspector: None,
    };

    // 1. Scenes: verify real scenes loaded and metadata matches
    let scenes = store.scenes().expect("scenes should load");
    assert!(scenes.len() >= 3, "expected at least 3 device scenes, got {}", scenes.len());

    let orca = scenes.iter().find(|s| s.id == ORCA_ID).expect("Orca scene should exist");
    assert_eq!(orca.name, "Orca");
    assert_eq!(orca.device, "AKP153E");
    assert_eq!(orca.keys, 17, "Orca scene should have 17 keys");
    assert!(orca.listed, "Orca scene should be listed");
    assert!(orca.current, "Orca scene should be current");

    let mon = scenes.iter().find(|s| s.id == MON_ID).expect("Monitorización scene should exist");
    assert_eq!(mon.name, "Monitorización");
    assert_eq!(mon.device, "AKP153E");
    assert_eq!(mon.keys, 17);
    assert!(mon.listed);

    let rec = scenes.iter().find(|s| s.id == REC_ID).expect("Grabación scene should exist");
    assert_eq!(rec.name, "Grabación");
    assert_eq!(rec.device, "AKP153E");
    assert_eq!(rec.keys, 15);
    assert!(rec.listed);

    // 2. Specific scene cells
    let doc = store.scene(ORCA_ID).expect("Orca scene manifest should load");
    let actions = doc.get("Actions").and_then(Value::as_object).expect("Actions map");
    assert_eq!(actions.len(), 17);
    assert_eq!(actions["0,0"]["UUID"], "com.miguelpicado.orcadock.agent");
    assert_eq!(actions["0,2"]["UUID"], "com.miguelpicado.orcadock.quota");
    assert_eq!(actions["2,0"]["UUID"], "com.hotspot.streamdock.profile.rotate");
    assert_eq!(actions["5,0"]["UUID"], "com.miguelpicado.orcadock.clock");
    assert_eq!(actions["5,2"]["UUID"], "com.miguelpicado.orcadock.summary");

    // 3. Plugins: Orca Deck actions are builtin
    let all_plugins = store.plugins().expect("plugins should load");
    let orcadock = all_plugins.iter().find(|p| p.uuid == "com.miguelpicado.orcadock").expect("com.miguelpicado.orcadock should exist");
    assert_eq!(orcadock.status, "ok");
    assert!(orcadock.builtin, "com.miguelpicado.orcadock should be marked builtin");

    let acts = orcadock.manifest.get("Actions").and_then(Value::as_array).expect("Actions array");
    let act_uuids: Vec<&str> = acts.iter().filter_map(|a| a.get("UUID").and_then(Value::as_str)).collect();
    assert!(act_uuids.contains(&"com.miguelpicado.orcadock.agent"));
    assert!(act_uuids.contains(&"com.miguelpicado.orcadock.quota"));
    assert!(act_uuids.contains(&"com.miguelpicado.orcadock.clock"));
    assert!(act_uuids.contains(&"com.miguelpicado.orcadock.summary"));

    // 4. Image resolution: profile images and plugin images
    let arrow_img = store.image(Some(ORCA_ID), None, "ARROW_RIGHT.png");
    assert!(arrow_img.is_some(), "ARROW_RIGHT.png should be found in Orca profile Images");
    let (bytes, mime) = arrow_img.unwrap();
    assert_eq!(mime, "image/png");
    assert!(!bytes.is_empty());

    let action_img = store.image(None, Some("com.miguelpicado.orcadock"), "imgs/action");
    assert!(action_img.is_some(), "imgs/action should be found in com.miguelpicado.orcadock plugin");
    let (bytes, mime) = action_img.unwrap();
    assert_eq!(mime, "image/png");
    assert!(!bytes.is_empty());

    // 5. Device info
    let dev = store.device().expect("device info");
    assert_eq!(dev["model"], "AKP153E");

    // 6. Loopback HTTP server test with real data
    let server = Server::start(Arc::new(store), 0).expect("bind server");
    let port = server.addr.port();
    let token = server.token().to_string();

    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
    let req = format!(
        "GET /api/scenes HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nX-OrcaDeck-Token: {token}\r\nConnection: close\r\n\r\n"
    );
    stream.write_all(req.as_bytes()).unwrap();
    let mut resp = Vec::new();
    stream.read_to_end(&mut resp).unwrap();
    let sresp = String::from_utf8_lossy(&resp);
    assert!(sresp.starts_with("HTTP/1.1 200 OK"));
    assert!(sresp.contains("Monitorización"));
    assert!(sresp.contains("Orca"));
    assert!(sresp.contains("Grabación"));

    server.stop();
    let _ = std::fs::remove_dir_all(&temp_data);
}
