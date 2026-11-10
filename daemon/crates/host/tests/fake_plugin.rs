//! A fake plugin (and property inspector) talking to the host over a real WebSocket.

use orcadeck_host::{Config, Device, Host, Instance, Key, Output, Plugin};
use serde_json::{json, Value};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::{channel, Receiver};
use std::time::{Duration, Instant};
use tungstenite::client::IntoClientRequest;
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{Message, WebSocket};

type Ws = WebSocket<MaybeTlsStream<TcpStream>>;
const T: Duration = Duration::from_secs(5);
const EDITOR: &str = "http://127.0.0.1:18619";

fn start() -> (Host, Receiver<Output>) {
    let (tx, rx) = channel();
    let tx = std::sync::Mutex::new(tx);
    let cfg = Config {
        port: 0,
        devices: vec![Device { id: "DEV".into(), name: "AKP153E".into(), columns: 6, rows: 3, kind: 0 }],
        origins: vec![EDITOR.into()],
        ..Config::default()
    };
    let host = Host::start(cfg, move |o: Output| tx.lock().unwrap().send(o).unwrap()).unwrap();
    (host, rx)
}

fn plugin() -> Plugin {
    Plugin::from_manifest(Path::new("x/com.fake.sdPlugin"), json!({"UUID": "com.fake", "CodePath": "p.js"})).unwrap()
}

fn open(host: &Host, origin: Option<&str>) -> tungstenite::Result<Ws> {
    let mut req = format!("ws://127.0.0.1:{}", host.port()).into_client_request().unwrap();
    if let Some(o) = origin {
        req.headers_mut().insert("Origin", o.parse().unwrap());
    }
    let (ws, _) = tungstenite::connect(req)?;
    if let MaybeTlsStream::Plain(s) = ws.get_ref() {
        s.set_read_timeout(Some(T)).unwrap();
    }
    Ok(ws)
}

fn connect(host: &Host, origin: Option<&str>, register: Value) -> Ws {
    let mut ws = open(host, origin).unwrap();
    send(&mut ws, register);
    ws
}

fn recv(ws: &mut Ws) -> Value {
    loop {
        if let Message::Text(t) = ws.read().expect("expected a frame") {
            return serde_json::from_str(&t).unwrap();
        }
    }
}

/// True once the host has dropped the socket (no frame arrives, the read fails or closes).
fn closed(ws: &mut Ws) -> bool {
    loop {
        match ws.read() {
            Ok(Message::Close(_)) | Err(_) => return true,
            Ok(Message::Text(_)) => return false,
            Ok(_) => {}
        }
    }
}

fn send(ws: &mut Ws, v: Value) {
    ws.send(Message::text(v.to_string())).unwrap();
}

fn key(context: &str, column: u32, row: u32) -> Key {
    Key { context: context.into(), device: "DEV".into(), column, row }
}

#[test]
fn register_appear_set_image_key_down() {
    let (host, out) = start();
    let uuid = host.add(&plugin());

    // Placed before the plugin connects: delivered on registration.
    let c1 = host.appear(Instance::new("com.fake", "com.fake.act", "DEV", 1, 2));
    assert_eq!(c1.len(), 32);
    let mut ws = connect(&host, None, json!({"event": "registerPlugin", "uuid": uuid}));

    let dev = recv(&mut ws);
    assert_eq!(dev["event"], "deviceDidConnect");
    assert_eq!(dev["deviceInfo"]["size"], json!({"columns": 6, "rows": 3}));
    let appear = recv(&mut ws);
    assert_eq!(appear["event"], "willAppear");
    assert_eq!(appear["context"], c1.as_str());
    assert_eq!(appear["action"], "com.fake.act");
    assert_eq!(appear["payload"]["coordinates"], json!({"column": 1, "row": 2}));
    assert_eq!(appear["payload"]["controller"], "Keypad");

    // Placed while connected: delivered at once.
    let mut side = Instance::new("com.fake", "com.fake.act", "DEV", 5, 0);
    side.controller = "Information".into();
    let c2 = host.appear(side);
    assert_ne!(c1, c2);
    let appear = recv(&mut ws);
    assert_eq!((appear["context"].as_str(), appear["payload"]["controller"].as_str()), (Some(c2.as_str()), Some("Information")));

    send(&mut ws, json!({"event": "setImage", "context": c2, "payload": {"image": "data:image/svg+xml,<svg/>", "target": 0}}));
    assert_eq!(
        out.recv_timeout(T).unwrap(),
        Output::Image { key: key(&c2, 5, 0), image: "data:image/svg+xml,<svg/>".into(), state: None }
    );

    assert!(host.key(&c1, true));
    let down = recv(&mut ws);
    assert_eq!((down["event"].as_str(), down["context"].as_str()), (Some("keyDown"), Some(c1.as_str())));
    assert!(host.key(&c1, false));
    assert_eq!(recv(&mut ws)["event"], "keyUp");
    assert!(!host.key("nope", true));

    // Unknown commands and foreign contexts are dropped; the socket keeps working.
    send(&mut ws, json!({"event": "setFeedbackLayout", "context": c1, "payload": {}}));
    send(&mut ws, json!({"event": "showOk", "context": "not-mine"}));
    send(&mut ws, json!({"event": "setTitle", "context": c1, "payload": {"title": "hi", "target": 0}}));
    assert_eq!(out.recv_timeout(T).unwrap(), Output::Title { key: key(&c1, 1, 2), title: "hi".into(), state: None });
    send(&mut ws, json!({"event": "setState", "context": c1, "payload": {"state": 1}}));
    assert_eq!(out.recv_timeout(T).unwrap(), Output::State { key: key(&c1, 1, 2), state: 1 });
    send(&mut ws, json!({"event": "showAlert", "context": c1}));
    assert_eq!(out.recv_timeout(T).unwrap(), Output::ShowAlert(key(&c1, 1, 2)));
    send(&mut ws, json!({"event": "openUrl", "payload": {"url": "https://example.com"}}));
    assert_eq!(out.recv_timeout(T).unwrap(), Output::OpenUrl("https://example.com".into()));
    send(&mut ws, json!({"event": "logMessage", "payload": {"message": "hello"}}));

    // Settings round trip.
    send(&mut ws, json!({"event": "setSettings", "context": c1, "payload": {"a": 1}}));
    assert_eq!(out.recv_timeout(T).unwrap(), Output::Settings { context: c1.clone(), settings: json!({"a": 1}) });
    send(&mut ws, json!({"event": "getSettings", "context": c1}));
    let s = recv(&mut ws);
    assert_eq!((s["event"].as_str(), &s["payload"]["settings"], &s["payload"]["state"]), (Some("didReceiveSettings"), &json!({"a": 1}), &json!(1)));

    host.load_global_settings("com.fake", json!({"g": 0}));
    send(&mut ws, json!({"event": "getGlobalSettings", "context": uuid}));
    assert_eq!(recv(&mut ws)["payload"]["settings"], json!({"g": 0}));
    send(&mut ws, json!({"event": "setGlobalSettings", "context": uuid, "payload": {"g": 2}}));
    assert_eq!(out.recv_timeout(T).unwrap(), Output::GlobalSettings { plugin: "com.fake".into(), settings: json!({"g": 2}) });
    send(&mut ws, json!({"event": "getGlobalSettings", "context": uuid}));
    assert_eq!(recv(&mut ws)["payload"]["settings"], json!({"g": 2}));

    host.disappear(&c2);
    let gone = recv(&mut ws);
    assert_eq!((gone["event"].as_str(), gone["context"].as_str()), (Some("willDisappear"), Some(c2.as_str())));
    assert!(out.try_recv().is_err(), "no stray outputs");
}

#[test]
fn property_inspector_routing() {
    let (host, out) = start();
    let uuid = host.add(&plugin());
    let c1 = host.appear(Instance::new("com.fake", "com.fake.act", "DEV", 0, 0));
    let mut plugin = connect(&host, None, json!({"event": "registerPlugin", "uuid": uuid}));
    recv(&mut plugin); // deviceDidConnect
    recv(&mut plugin); // willAppear

    let token = host.open_inspector(&c1).unwrap();
    assert!(host.open_inspector("nope").is_none());
    let mut pi = connect(&host, Some(EDITOR), json!({"event": "registerPropertyInspector", "uuid": token}));
    assert_eq!(recv(&mut pi)["event"], "didReceiveSettings");
    assert_eq!(recv(&mut plugin)["event"], "propertyInspectorDidAppear");

    // PI -> plugin, routed by socket whatever the PI puts in `context`.
    send(&mut pi, json!({"event": "sendToPlugin", "context": token, "payload": {"x": 1}}));
    let m = recv(&mut plugin);
    assert_eq!((m["event"].as_str(), m["context"].as_str(), &m["payload"]), (Some("sendToPlugin"), Some(c1.as_str()), &json!({"x": 1})));

    // Plugin -> PI.
    send(&mut plugin, json!({"event": "sendToPropertyInspector", "context": c1, "payload": {"y": 2}}));
    let m = recv(&mut pi);
    assert_eq!((m["event"].as_str(), &m["payload"]), (Some("sendToPropertyInspector"), &json!({"y": 2})));

    // PI setSettings is persisted and echoed to the plugin.
    send(&mut pi, json!({"event": "setSettings", "context": c1, "payload": {"k": "v"}}));
    assert_eq!(out.recv_timeout(T).unwrap(), Output::Settings { context: c1.clone(), settings: json!({"k": "v"}) });
    let m = recv(&mut plugin);
    assert_eq!((m["event"].as_str(), &m["payload"]["settings"]), (Some("didReceiveSettings"), &json!({"k": "v"})));

    drop(pi);
    assert_eq!(recv(&mut plugin)["event"], "propertyInspectorDidDisappear");

    // Tokens are single use.
    let mut again = connect(&host, Some(EDITOR), json!({"event": "registerPropertyInspector", "uuid": token}));
    assert!(closed(&mut again));
}

#[test]
fn pi_without_token_is_rejected() {
    let (host, _out) = start();
    let c1 = host.appear(Instance::new("com.fake", "com.fake.act", "DEV", 0, 0));
    // The context is not a credential: only a token from open_inspector is.
    let mut pi = connect(&host, Some(EDITOR), json!({"event": "registerPropertyInspector", "uuid": c1}));
    assert!(closed(&mut pi));
    let mut pi = connect(&host, Some(EDITOR), json!({"event": "registerPropertyInspector"}));
    assert!(closed(&mut pi));
}

#[test]
fn unknown_plugin_uuid_is_rejected() {
    let (host, _out) = start();
    host.add(&plugin());
    let mut ws = connect(&host, None, json!({"event": "registerPlugin", "uuid": "WRONG"}));
    assert!(closed(&mut ws));
}

#[test]
fn origins() {
    let (host, _out) = start();
    assert!(open(&host, Some("null")).is_err(), "null origin rejected");
    assert!(open(&host, Some("file://")).is_err(), "file origin rejected");
    assert!(open(&host, Some("https://evil.example")).is_err(), "foreign origin rejected");
    assert!(open(&host, Some("http://127.0.0.1:1")).is_err(), "unlisted loopback origin rejected");
    assert!(open(&host, Some(EDITOR)).is_ok(), "configured loopback origin accepted");
    assert!(open(&host, None).is_ok(), "no origin (native plugin) accepted");
}

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("orcadeck-host-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// A real node plugin: `body` runs as `p.js`; node ignores the SDK argv that follows the script.
fn node_plugin(root: &Path, id: &str, body: &str) -> Plugin {
    let dir = root.join(format!("{id}.sdPlugin"));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("p.js"), body).unwrap();
    Plugin::from_manifest(&dir, json!({"UUID": id, "CodePath": "p.js"})).unwrap()
}

fn skip_without_node() -> bool {
    if Command::new("node").arg("--version").output().is_ok() {
        return false;
    }
    eprintln!("skipped: node not found");
    true
}

/// Waits until the marker has `want` bytes (test-side polling is fine); returns the last size seen.
fn wait_bytes(marker: &Path, want: u64, timeout: Duration) -> u64 {
    let deadline = Instant::now() + timeout;
    loop {
        let n = std::fs::metadata(marker).map(|m| m.len()).unwrap_or(0);
        if n >= want || Instant::now() >= deadline {
            return n;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}

fn marker_len(marker: &Path) -> u64 {
    std::fs::metadata(marker).map(|m| m.len()).unwrap_or(0)
}

#[test]
fn global_settings_seed_before_launch() {
    let (host, _out) = start();
    // Seeded with no slot yet: launch/add must preserve it.
    host.load_global_settings("com.fake", json!({"seed": 1}));
    let uuid = host.add(&plugin());
    let mut ws = connect(&host, None, json!({"event": "registerPlugin", "uuid": uuid}));
    assert_eq!(recv(&mut ws)["event"], "deviceDidConnect");
    send(&mut ws, json!({"event": "getGlobalSettings", "context": uuid}));
    assert_eq!(recv(&mut ws)["payload"]["settings"], json!({"seed": 1}));
}

#[test]
fn crashed_plugin_is_relaunched() {
    if skip_without_node() {
        return;
    }
    let root = tmp("restart");
    let marker = root.join("marker");
    let body = format!(
        "require('fs').appendFileSync({}, 'x'); process.exit(1);",
        json!(marker.to_str().unwrap())
    );
    let plugin = node_plugin(&root, "com.crash", &body);
    let (host, _out) = start();
    host.launch(plugin.clone()).unwrap();
    // First run writes once, crashes, and the 1 s backoff must produce a second run.
    let n = wait_bytes(&marker, 2, Duration::from_secs(10));
    host.stop(&plugin.id);
    let _ = std::fs::remove_dir_all(&root);
    assert!(n >= 2, "crashed plugin was not relaunched (marker {n} B)");
}

#[test]
fn stop_prevents_restart() {
    if skip_without_node() {
        return;
    }
    let root = tmp("stop");
    let marker = root.join("marker");
    let body = format!(
        "const fs = require('fs'); fs.appendFileSync({m}, 'x'); setInterval(() => fs.appendFileSync({m}, 'x'), 100);",
        m = json!(marker.to_str().unwrap())
    );
    let plugin = node_plugin(&root, "com.alive", &body);
    let (host, _out) = start();
    host.launch(plugin.clone()).unwrap();
    assert!(wait_bytes(&marker, 2, Duration::from_secs(10)) >= 2, "plugin did not run");
    host.stop(&plugin.id);
    std::thread::sleep(Duration::from_millis(300)); // let an in-flight write land
    let after_stop = marker_len(&marker);
    std::thread::sleep(Duration::from_millis(1500)); // longer than the 1 s first backoff
    let later = marker_len(&marker);
    let _ = std::fs::remove_dir_all(&root);
    assert_eq!(after_stop, later, "plugin kept running or was restarted after stop");
}
