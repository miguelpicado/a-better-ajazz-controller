//! End-to-end property-inspector routing from a sandboxed iframe: a PI with `Origin: null` (no
//! `allow-same-origin`) still registers with its single-use token and exchanges settings/messages.
use orcadeck_host::{Config, Device, Host, Instance, Output, Plugin};
use serde_json::{json, Value};
use std::{
    net::TcpStream,
    path::Path,
    sync::mpsc::{channel, Receiver},
    time::Duration,
};
use tungstenite::client::IntoClientRequest;
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{Message, WebSocket};

type Ws = WebSocket<MaybeTlsStream<TcpStream>>;
const T: Duration = Duration::from_secs(5);

fn start() -> (Host, Receiver<Output>) {
    let (tx, rx) = channel();
    let tx = std::sync::Mutex::new(tx);
    let cfg = Config {
        port: 0,
        devices: vec![Device { id: "DEV".into(), name: "AKP153E".into(), columns: 6, rows: 3, kind: 0 }],
        // The editor origin frames the PI; the PI itself is sandboxed, hence `null`.
        origins: vec!["http://127.0.0.1:18619".into(), "null".into()],
        ..Config::default()
    };
    let host = Host::start(cfg, move |o: Output| tx.lock().unwrap().send(o).unwrap()).unwrap();
    (host, rx)
}

fn plugin() -> Plugin {
    Plugin::from_manifest(Path::new("x/com.fake.sdPlugin"), json!({"UUID": "com.fake", "CodePath": "p.js"})).unwrap()
}

fn open(host: &Host, origin: &str) -> tungstenite::Result<Ws> {
    let mut req = format!("ws://127.0.0.1:{}", host.port()).into_client_request().unwrap();
    req.headers_mut().insert("Origin", origin.parse().unwrap());
    let (ws, _) = tungstenite::connect(req)?;
    if let MaybeTlsStream::Plain(s) = ws.get_ref() {
        s.set_read_timeout(Some(T)).unwrap();
    }
    Ok(ws)
}

fn connect(host: &Host, origin: &str, register: Value) -> Ws {
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

#[test]
fn sandboxed_property_inspector_round_trip() {
    let (host, out) = start();
    let uuid = host.add(&plugin());
    let ctx = host.appear(Instance::new("com.fake", "com.fake.act", "DEV", 0, 0));

    // Native plugin processes send no Origin header.
    let mut plugin_ws = {
        let req = format!("ws://127.0.0.1:{}", host.port()).into_client_request().unwrap();
        let (ws, _) = tungstenite::connect(req).unwrap();
        if let MaybeTlsStream::Plain(s) = ws.get_ref() {
            s.set_read_timeout(Some(T)).unwrap();
        }
        ws
    };
    send(&mut plugin_ws, json!({"event": "registerPlugin", "uuid": uuid}));
    assert_eq!(recv(&mut plugin_ws)["event"], "deviceDidConnect");
    assert_eq!(recv(&mut plugin_ws)["event"], "willAppear");

    let token = host.open_inspector(&ctx).expect("context exists");
    let mut pi = connect(&host, "null", json!({"event": "registerPropertyInspector", "uuid": token}));
    assert_eq!(recv(&mut pi)["event"], "didReceiveSettings");
    assert_eq!(recv(&mut plugin_ws)["event"], "propertyInspectorDidAppear");

    send(&mut pi, json!({"event": "sendToPlugin", "context": token, "payload": {"x": 1}}));
    let m = recv(&mut plugin_ws);
    assert_eq!((m["event"].as_str(), m["context"].as_str()), (Some("sendToPlugin"), Some(ctx.as_str())));

    send(&mut plugin_ws, json!({"event": "sendToPropertyInspector", "context": ctx, "payload": {"y": 2}}));
    assert_eq!(recv(&mut pi)["payload"], json!({"y": 2}));

    send(&mut pi, json!({"event": "setSettings", "context": ctx, "payload": {"k": "v"}}));
    assert_eq!(out.recv_timeout(T).unwrap(), Output::Settings { context: ctx.clone(), settings: json!({"k": "v"}) });
    assert_eq!(recv(&mut plugin_ws)["payload"]["settings"], json!({"k": "v"}));

    drop(pi);
    assert_eq!(recv(&mut plugin_ws)["event"], "propertyInspectorDidDisappear");

    // A token is single-use, and `null` is only accepted because this host lists it.
    let mut again = connect(&host, "null", json!({"event": "registerPropertyInspector", "uuid": token}));
    assert!(closed(&mut again));
    assert!(open(&host, "https://evil.example").is_err(), "foreign origin rejected");
}
