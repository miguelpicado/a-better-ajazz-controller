//! Property inspector endpoint + static file serving over a real loopback socket.
use orcadeck_web::{FsStore, InspectorBridge, InspectorSession, Server};
use serde_json::{json, Value};
use std::{
    fs,
    io::{Read, Write},
    net::TcpStream,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

const SCENE: &str = "PI000000-0000-0000-0000-000000000001";
const UUID: &str = "com.test.pi";
const PI: &str = "PropertyInspector/index.html";

fn w(p: impl AsRef<Path>, b: impl AsRef<[u8]>) {
    fs::create_dir_all(p.as_ref().parent().unwrap()).unwrap();
    fs::write(p, b).unwrap();
}

fn fixture(name: &str) -> PathBuf {
    let t = std::env::temp_dir().join(format!("orcadeck-web-pi-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&t);
    w(t.join(format!("profiles/{SCENE}.sdProfile/manifest.json")), json!({
        "Name": "PI", "DeviceUUID": "AKP153E",
        "Actions": {
            "0,0": { "UUID": "com.test.pi.act", "ActionID": "a", "Name": "Act", "Settings": { "a": 1 },
                     "PropertyInspectorPath": PI },
            "1,0": { "UUID": "com.test.pi.nopi", "ActionID": "b", "Name": "No PI" },
        }
    }).to_string());
    let plugin = t.join(format!("plugins/{UUID}.sdPlugin"));
    w(
        plugin.join("manifest.json"),
        json!({
            "Name": "PI", "Version": "2.0",
            "Actions": [{ "UUID": "com.test.pi.act", "Name": "Act", "PropertyInspectorPath": PI },
                         { "UUID": "com.test.pi.nopi", "Name": "No PI" }]
        })
        .to_string(),
    );
    w(
        plugin.join(PI),
        "<!doctype html><html><body><script src=\"./app.js\"></script></body></html>",
    );
    w(
        plugin.join("PropertyInspector/app.js"),
        "window.connectElgatoStreamDeckSocket = function () {};",
    );
    w(t.join("secret.txt"), "outside");
    t
}

fn store(t: &Path, bridge: Option<Arc<dyn InspectorBridge>>) -> FsStore {
    FsStore {
        profiles: t.join("profiles"),
        plugins: t.join("plugins"),
        builtin_plugins: vec![],
        data: t.join("data"),
        legacy_config: None,
        downloads: None,
        inspector: bridge,
    }
}

#[derive(Default)]
struct Bridge {
    calls: Mutex<Vec<(String, String)>>,
    issued: Mutex<u32>,
}

impl InspectorBridge for Bridge {
    fn open(&self, scene: &str, key: &str) -> Result<InspectorSession, String> {
        self.calls
            .lock()
            .unwrap()
            .push((scene.to_string(), key.to_string()));
        let mut n = self.issued.lock().unwrap();
        *n += 1;
        Ok(InspectorSession {
            token: format!("pi-token-{n}"),
            port: 18618,
            context: "CTX".into(),
        })
    }
}

struct Resp {
    status: u16,
    head: String,
    body: Vec<u8>,
}

impl Resp {
    fn json(&self) -> Value {
        serde_json::from_slice(&self.body)
            .unwrap_or_else(|_| panic!("not JSON: {}", String::from_utf8_lossy(&self.body)))
    }
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }
    fn has(&self, h: &str) -> bool {
        self.head
            .to_ascii_lowercase()
            .contains(&h.to_ascii_lowercase())
    }
}

fn raw(s: &Server, req: &str) -> Resp {
    let mut c = TcpStream::connect(s.addr).unwrap();
    c.write_all(req.as_bytes()).unwrap();
    let mut out = vec![];
    c.read_to_end(&mut out).unwrap();
    let split = out
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .expect("headers");
    let head = String::from_utf8_lossy(&out[..split]).into_owned();
    Resp {
        status: head[9..12].parse().unwrap(),
        head,
        body: out[split + 4..].to_vec(),
    }
}

fn call(s: &Server, method: &str, path: &str, body: Option<Value>) -> Resp {
    let b = body.map(|b| b.to_string()).unwrap_or_default();
    raw(s, &format!(
        "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nX-OrcaDeck-Token: {}\r\nContent-Type: application/json\r\n\
         Content-Length: {}\r\n\r\n{b}", s.addr.port(), s.token(), b.len()))
}

/// PI assets are requested by the sandboxed iframe: same host, no editor token.
fn get_pi(s: &Server, path: &str, extra: &str) -> Resp {
    raw(
        s,
        &format!(
            "GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\n{extra}\r\n",
            s.addr.port()
        ),
    )
}

#[test]
fn inspector_endpoint_501_without_a_bridge() {
    let t = fixture("501");
    let s = Server::start(Arc::new(store(&t, None)), 0).unwrap();
    let r = call(
        &s,
        "POST",
        &format!("/api/scenes/{SCENE}/cells/0,0/inspector"),
        Some(json!({})),
    );
    assert_eq!(r.status, 501);
    assert!(r.json()["error"].as_str().unwrap().contains("daemon"));
    // The editor token gate is unchanged on the new route.
    assert_eq!(
        raw(
            &s,
            &format!(
                "POST /api/scenes/{SCENE}/cells/0,0/inspector HTTP/1.1\r\nHost: 127.0.0.1:{}\r\n\
        Content-Type: application/json\r\nContent-Length: 2\r\n\r\n{{}}",
                s.addr.port()
            )
        )
        .status,
        401
    );
}

#[test]
fn inspector_endpoint_issues_a_separate_single_use_token() {
    let t = fixture("token");
    let bridge = Arc::new(Bridge::default());
    let s = Server::start(Arc::new(store(&t, Some(bridge.clone()))), 0).unwrap();
    let path = format!("/api/scenes/{SCENE}/cells/0,0/inspector");
    let r = call(&s, "POST", &path, Some(json!({})));
    assert_eq!(r.status, 200);
    let j = r.json();
    assert_eq!(j["token"], "pi-token-1");
    assert_ne!(j["token"].as_str().unwrap(), s.token());
    assert_eq!(
        (j["port"].as_u64(), j["context"].as_str()),
        (Some(18618), Some("CTX"))
    );
    assert_eq!(
        j["url"].as_str(),
        Some("/api/plugins/com.test.pi/pi/PropertyInspector/index.html?ws=18618")
    );
    assert_eq!(j["info"]["plugin"]["uuid"], UUID);
    assert_eq!(j["actionInfo"]["action"], "com.test.pi.act");
    assert_eq!(j["actionInfo"]["payload"]["settings"], json!({ "a": 1 }));
    assert_eq!(
        j["actionInfo"]["payload"]["coordinates"],
        json!({ "column": 0, "row": 0 })
    );
    assert_eq!(
        bridge.calls.lock().unwrap().as_slice(),
        [(SCENE.to_string(), "0,0".to_string())]
    );

    // Each selection gets a fresh token.
    assert_eq!(
        call(&s, "POST", &path, Some(json!({}))).json()["token"],
        "pi-token-2"
    );

    // Action without a PI, empty cell, bad cell and unknown scene do not issue tokens.
    for (p, status) in [
        (format!("/api/scenes/{SCENE}/cells/1,0/inspector"), 404),
        (format!("/api/scenes/{SCENE}/cells/2,0/inspector"), 404),
        (format!("/api/scenes/{SCENE}/cells/9,9/inspector"), 400),
        ("/api/scenes/NOPE/cells/0,0/inspector".to_string(), 404),
    ] {
        assert_eq!(call(&s, "POST", &p, Some(json!({}))).status, status, "{p}");
    }
    assert_eq!(bridge.calls.lock().unwrap().len(), 2);
}

#[test]
fn pi_static_serves_files_and_injects_the_bootstrap() {
    let t = fixture("static");
    let s = Server::start(Arc::new(store(&t, None)), 0).unwrap();
    let port = s.addr.port();
    let html = get_pi(
        &s,
        "/api/plugins/com.test.pi/pi/PropertyInspector/index.html?ws=18618",
        "",
    );
    assert_eq!(html.status, 200);
    assert!(html.has("content-type: text/html; charset=utf-8"));
    assert!(html.has("x-frame-options: SAMEORIGIN"));
    assert!(!html.has("x-frame-options: deny"));
    assert!(html.has(&format!("frame-ancestors http://127.0.0.1:{port}")));
    assert!(html.has("connect-src ws://127.0.0.1:18618"));
    assert!(html.has("default-src 'none'"));
    assert!(!html.has("https:") && !html.text().contains("https:"));
    assert!(
        html.text().contains("orcadeck-pi-ready"),
        "bootstrap injected"
    );

    let js = get_pi(
        &s,
        "/api/plugins/com.test.pi/pi/PropertyInspector/app.js",
        "",
    );
    assert_eq!(js.status, 200);
    assert!(js.has("content-type: text/javascript"));
    assert!(
        !js.text().contains("orcadeck-pi-ready"),
        "no bootstrap in assets"
    );

    // The sandboxed PI reports Origin: null and cross-site fetch metadata; both are expected.
    assert_eq!(
        get_pi(
            &s,
            "/api/plugins/com.test.pi/pi/PropertyInspector/index.html",
            "Origin: null\r\n"
        )
        .status,
        200
    );
    assert_eq!(
        get_pi(
            &s,
            "/api/plugins/com.test.pi/pi/PropertyInspector/index.html",
            "Sec-Fetch-Site: cross-site\r\n"
        )
        .status,
        200
    );
    assert_eq!(
        get_pi(
            &s,
            "/api/plugins/com.test.pi/pi/PropertyInspector/index.html",
            &format!("Origin: http://127.0.0.1:{port}\r\n")
        )
        .status,
        200
    );
    assert_eq!(
        get_pi(
            &s,
            "/api/plugins/com.test.pi/pi/PropertyInspector/index.html",
            "Origin: https://evil.example\r\n"
        )
        .status,
        403
    );
    assert_eq!(raw(&s, "GET /api/plugins/com.test.pi/pi/PropertyInspector/index.html HTTP/1.1\r\nHost: evil.example\r\n\r\n").status, 403);
    assert!(
        !html.has("access-control-allow-origin"),
        "no CORS on plugin files"
    );
}

#[test]
fn pi_static_blocks_traversal_and_bad_ids() {
    let t = fixture("traversal");
    let s = Server::start(Arc::new(store(&t, None)), 0).unwrap();
    let paths = [
        "/api/plugins/com.test.pi/pi/../../secret.txt",
        "/api/plugins/com.test.pi/pi/PropertyInspector/../../../secret.txt",
        "/api/plugins/com.test.pi/pi/%2e%2e/secret.txt",
        "/api/plugins/com.test.pi/pi/..%2fsecret.txt",
        "/api/plugins/com.test.pi/pi/..%5csecret.txt",
        "/api/plugins/com.test.pi/pi/C:%5cWindows%5cwin.ini",
        "/api/plugins/com.test.pi/pi/%2fetc%2fpasswd",
        "/api/plugins/com.test.pi/pi/PropertyInspector//app.js",
        "/api/plugins/com.test.pi/pi/PropertyInspector",
        "/api/plugins/com.test.pi/pi/PropertyInspector/nope.js",
        "/api/plugins/../pi/index.html",
        "/api/plugins/com.test.pi/pi/../../plugins/com.test.pi.sdPlugin/PropertyInspector/index.html",
    ];
    for p in paths {
        let r = get_pi(&s, p, "");
        assert!(
            [400, 404].contains(&r.status),
            "{p} -> {} {}",
            r.status,
            r.text()
        );
    }
    assert_eq!(
        get_pi(&s, "/api/plugins/com.test.pi/pi/%2e%2e/secret.txt", "")
            .text()
            .contains("outside"),
        false
    );
}
