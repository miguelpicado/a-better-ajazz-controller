//! End-to-end tests over a real loopback socket against `FsStore` on a temp dir.
use orcadeck_web::{FsStore, Server};
use serde_json::{json, Value};
use std::{
    fs,
    io::{Read, Write},
    net::TcpStream,
    path::{Path, PathBuf},
    sync::Arc,
};

const ORCA: &str = "R0KZ54Z0-063U-947X-31BI-GG95R8F4HT88";
const MON: &str = "3I0U64IN-IT4R-RUB6-EZ4Z-VXS311JFXI05";
const REC: &str = "H33G4VTZ-25Z8-PYN7-R34M-ST39SX8TRT2Q";
const OLD: &str = "126RD5OK-HHCS-PQU7-QF1K-251C8Y9WEUHY";
const PNG: &[u8] = b"\x89PNG\r\n\x1a\nfake";

fn w(p: impl AsRef<Path>, b: impl AsRef<[u8]>) {
    fs::create_dir_all(p.as_ref().parent().unwrap()).unwrap();
    fs::write(p, b).unwrap();
}

/// Temp copy of the real layout (synthetic content): 3 device scenes + 1 old AKP815 scene, a plain plugin with
/// `en.json`, an encrypted one with overlay, an encrypted one without, a broken one, legacy ini + DataCache.db.
fn fixture(name: &str) -> PathBuf {
    let t = std::env::temp_dir().join(format!("orcadeck-web-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&t);
    let scene = |id: &str, name: &str, actions: Value| {
        let mut m = json!({ "Name": name, "Version": "1.0", "DeviceUUID": "AKP153E", "DeviceModel": "20GBA9901",
            "DeviceSerialNumber": "355499441494", "AppIdentifier0": "*", "Unknown": { "keep": [1, 2] },
            "Pages": { "Current": format!("{id}.sdProfile"), "Pages": [format!("{id}.sdProfile")] }, "Actions": actions });
        if id == OLD {
            m["DeviceUUID"] = json!("AKP815");
        }
        w(t.join(format!("profiles/{id}.sdProfile/manifest.json")), format!("\u{feff}{m}"));
    };
    let act = |uuid: &str, img: &str| json!({ "ActionID": "a", "Controller": "", "Name": uuid, "Settings": { "x": 1 },
        "State": 0, "States": [{ "Image": img, "Extra": true }], "UUID": uuid, "Mystery": 7 });
    scene(ORCA, "Orca", json!({ "0,0": act("com.test.demo.one", "imgs/one"), "1,0": act("com.test.demo.two", "ARROW.png"),
        "5,0": act("com.miguelpicado.orcadock.clock", "imgs/action") }));
    scene(MON, "Monitorización", json!({}));
    scene(REC, "Grabación", json!({}));
    scene(OLD, "WINDOWS Control", json!({ "0,4": act("com.test.demo.one", "") }));
    w(t.join(format!("profiles/{ORCA}.sdProfile/Images/ARROW.png")), PNG);

    let demo = t.join("plugins/com.test.demo.sdPlugin");
    w(demo.join("manifest.json"), json!({ "Name": "演示", "Version": "1.2", "Author": "T", "CodePath": "bin/plugin.js",
        "Icon": "imgs/plugin", "Actions": [
            { "UUID": "com.test.demo.one", "Name": "一", "Icon": "imgs/one", "States": [{ "Image": "imgs/one" }, { "Image": "imgs/one-b" }],
              "Settings": { "default": true } },
            { "UUID": "com.test.demo.two", "Name": "Two", "Icon": "imgs/two", "States": [{ "Image": "" }] },
            { "UUID": "com.test.demo.hidden", "Name": "Hidden", "VisibleInActionsList": false } ] }).to_string());
    w(demo.join("en.json"), json!({ "Name": "Demo", "com.test.demo.one": { "Name": "One", "Tooltip": "First" } }).to_string());
    w(demo.join("imgs/one.png"), PNG);
    w(demo.join("imgs/two.svg"), "<svg xmlns='http://www.w3.org/2000/svg'/>");
    w(t.join("secret.txt"), "nope");
    w(t.join("plugins/com.test.locked.sdPlugin/manifest.json"), b"ELGATO\x00\x01garbage");
    w(t.join("plugins/com.test.overlaid.sdPlugin/manifest.json"), b"ELGATO\x00\x01garbage");
    w(t.join("data/manifests/com.test.overlaid.json"), json!({ "Name": "Overlaid", "CodePath": "x.exe",
        "Actions": [{ "UUID": "com.test.overlaid.go", "Name": "Go" }] }).to_string());
    w(t.join("plugins/com.test.broken.sdPlugin/manifest.json"), "{ nope");
    w(t.join("builtin/com.hotspot.streamdock.profile.rotate.sdPlugin/manifest.json"), json!({ "Name": "Scene Shift",
        "Actions": [{ "UUID": "com.hotspot.streamdock.profile.rotate", "Name": "Scene Shift", "Icon": "Images/switchProfile",
        "States": [{ "Image": "" }] }] }).to_string());
    w(t.join("builtin/com.hotspot.streamdock.profile.rotate.sdPlugin/Images/switchProfile.png"), PNG);

    // Downloads candidates for the on-demand inbox scan: a folder package, an archive (never opened by the scan),
    // a text file that must be ignored, and a package outside Downloads for the traversal tests.
    let inbox = t.join("downloads");
    w(inbox.join("com.test.inbox.sdPlugin/manifest.json"), json!({ "Name": "Inbox", "Version": "0.1", "Author": "T",
        "Actions": [{ "UUID": "com.test.inbox.go", "Name": "Go" }] }).to_string());
    w(inbox.join("bundle.zip"), b"PK\x03\x04not a real zip");
    w(inbox.join("notes.txt"), "ignore me");
    w(t.join("outside/com.test.evil.sdPlugin/manifest.json"), json!({ "Name": "Evil" }).to_string());

    w(t.join("legacy/StreamDockConfig.ini"), format!(
        "[StreamDockCurrentDevice]\nDevice=AKP153E\nAKP153E\\SerialNumber=355499441494\n\n\
         [AKP153E355499441494DeviceConfig]\nsdProfileName={ORCA}.sdProfile\n\n[DeviceBrightness]\nAKP153E355499441494=70\n"));
    fs::copy(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/DataCache.db"), t.join("legacy/DataCache.db")).unwrap();
    t
}

fn start(t: &Path) -> Server {
    let store = FsStore {
        profiles: t.join("profiles"),
        plugins: t.join("plugins"),
        builtin_plugins: vec![t.join("builtin")],
        data: t.join("data"),
        legacy_config: Some(t.join("legacy")),
        downloads: Some(t.join("downloads")),
    };
    Server::start(Arc::new(store), 0).unwrap()
}

struct Resp {
    status: u16,
    head: String,
    body: Vec<u8>,
}

impl Resp {
    fn json(&self) -> Value {
        serde_json::from_slice(&self.body).unwrap_or_else(|_| panic!("not JSON: {}", String::from_utf8_lossy(&self.body)))
    }
    fn has(&self, h: &str) -> bool {
        self.head.to_ascii_lowercase().contains(&h.to_ascii_lowercase())
    }
}

fn raw(s: &Server, req: &str) -> Resp {
    let mut c = TcpStream::connect(s.addr).unwrap();
    c.write_all(req.as_bytes()).unwrap();
    let mut out = vec![];
    c.read_to_end(&mut out).unwrap();
    let split = out.windows(4).position(|w| w == b"\r\n\r\n").unwrap();
    let head = String::from_utf8_lossy(&out[..split]).into_owned();
    Resp { status: head[9..12].parse().unwrap(), head, body: out[split + 4..].to_vec() }
}

/// Request with a valid Host and the token header, as the SPA (or the daemon) would send it.
fn call(s: &Server, method: &str, path: &str, body: Option<Value>) -> Resp {
    let b = body.map(|b| b.to_string()).unwrap_or_default();
    raw(s, &format!(
        "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nX-OrcaDeck-Token: {}\r\nContent-Type: application/json\r\n\
         Content-Length: {}\r\n\r\n{b}", s.addr.port(), s.token(), b.len()))
}

fn scene_file(t: &Path, id: &str) -> Value {
    let b = fs::read_to_string(t.join(format!("profiles/{id}.sdProfile/manifest.json"))).unwrap();
    serde_json::from_str(b.trim_start_matches('\u{feff}')).unwrap()
}

#[test]
fn security_gate() {
    let t = fixture("sec");
    let s = start(&t);
    let port = s.addr.port();
    let tok = s.token().to_string();
    assert!(s.addr.ip().is_loopback());
    assert_eq!(tok.len(), 64);
    let get = |extra: &str| raw(&s, &format!("GET /api/scenes HTTP/1.1\r\n{extra}\r\n"));

    // Token: required, exact, by header or by cookie.
    assert_eq!(get(&format!("Host: 127.0.0.1:{port}\r\n")).status, 401);
    assert_eq!(get(&format!("Host: 127.0.0.1:{port}\r\nX-OrcaDeck-Token: {}\r\n", &tok[..63])).status, 401);
    assert_eq!(get(&format!("Host: 127.0.0.1:{port}\r\nX-OrcaDeck-Token: {tok}\r\n")).status, 200);
    assert_eq!(get(&format!("Host: localhost:{port}\r\nCookie: a=b; orcadeck_{port}={tok}\r\n")).status, 200);
    assert_eq!(get(&format!("Host: localhost:{port}\r\nCookie: orcadeck_1={tok}\r\n")).status, 401);

    // DNS rebinding: a foreign Host is refused even with the right token; so is a missing Host.
    assert_eq!(get(&format!("Host: evil.example:{port}\r\nX-OrcaDeck-Token: {tok}\r\n")).status, 403);
    assert_eq!(get(&format!("Host: 127.0.0.1:1\r\nX-OrcaDeck-Token: {tok}\r\n")).status, 403);
    assert_eq!(get(&format!("X-OrcaDeck-Token: {tok}\r\n")).status, 403);
    assert_eq!(raw(&s, "GET / HTTP/1.1\r\nHost: evil.example\r\n\r\n").status, 403);

    // CSRF: foreign Origin / cross-site fetch refused; same origin accepted.
    let h = format!("Host: 127.0.0.1:{port}\r\nX-OrcaDeck-Token: {tok}\r\n");
    assert_eq!(get(&format!("{h}Origin: http://evil.example\r\n")).status, 403);
    assert_eq!(get(&format!("{h}Origin: http://127.0.0.1:{port}.evil.example\r\n")).status, 403);
    assert_eq!(get(&format!("{h}Origin: null\r\n")).status, 403);
    assert_eq!(get(&format!("{h}Sec-Fetch-Site: cross-site\r\n")).status, 403);
    assert_eq!(get(&format!("{h}Origin: http://localhost:{port}\r\nSec-Fetch-Site: same-origin\r\n")).status, 200);
    let form = raw(&s, &format!("PUT /api/device HTTP/1.1\r\n{h}Content-Type: text/plain\r\nContent-Length: 2\r\n\r\n{{}}"));
    assert_eq!(form.status, 415);

    // No CORS, ever.
    let pre = raw(&s, &format!("OPTIONS /api/scenes HTTP/1.1\r\n{h}Origin: http://evil.example\r\n\
        Access-Control-Request-Method: PUT\r\n\r\n"));
    assert_eq!(pre.status, 403);
    for r in [&pre, &get(&h)] {
        assert!(!r.has("access-control-allow"));
    }

    // URL token becomes an HttpOnly, SameSite=Strict cookie and leaves the address bar.
    let r = raw(&s, &format!("GET /?token={tok} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n\r\n"));
    assert_eq!(r.status, 303);
    assert!(r.has(&format!("set-cookie: orcadeck_{port}={tok}; Path=/; HttpOnly; SameSite=Strict")));
    assert!(r.has("location: /\r") || r.head.to_ascii_lowercase().ends_with("location: /"));
    let r = raw(&s, &format!("GET /?token=wrong HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n\r\n"));
    assert_eq!(r.status, 200);
    assert!(!r.has("set-cookie"));
    assert!(r.has("content-security-policy: default-src 'none'"));
    assert!(r.has("x-frame-options: deny"));

    // Each server has its own token.
    let s2 = start(&t);
    assert_ne!(s2.token(), s.token());
    s2.stop();
}

#[test]
fn scenes_order_current_and_put() {
    let t = fixture("scenes");
    let s = start(&t);
    let list = call(&s, "GET", "/api/scenes", None).json();
    let ids: Vec<&str> = list.as_array().unwrap().iter().map(|s| s["id"].as_str().unwrap()).collect();
    assert_eq!(ids, [MON, ORCA, REC, OLD], "device order from DataCache.db, then the rest");
    assert_eq!(list[1]["current"], true, "current scene from the ini");
    assert_eq!((list[3]["listed"].as_bool(), list[3]["device"].as_str()), (Some(false), Some("AKP815")));
    assert_eq!(list[1]["keys"], 3);

    // Reorder + switch persist in our own state, never in the legacy folder.
    let legacy_before = fs::read(t.join("legacy/StreamDockConfig.ini")).unwrap();
    assert_eq!(call(&s, "PUT", "/api/scenes/order", Some(json!([REC, ORCA]))).status, 200);
    assert_eq!(call(&s, "PUT", "/api/scenes/order", Some(json!(["NOPE"]))).status, 400);
    assert_eq!(call(&s, "PUT", "/api/scenes/current", Some(json!({ "id": REC }))).status, 200);
    let list = call(&s, "GET", "/api/scenes", None).json();
    assert_eq!((list[0]["id"].as_str(), list[0]["current"].as_bool()), (Some(REC), Some(true)));
    assert_eq!(list[2]["listed"], false);
    assert_eq!(fs::read(t.join("legacy/StreamDockConfig.ini")).unwrap(), legacy_before);

    // GET + PUT round trip keeps unknown fields, writes 4-space JSON and a backup.
    let mut doc = call(&s, "GET", &format!("/api/scenes/{ORCA}"), None).json();
    assert_eq!(doc["Unknown"]["keep"], json!([1, 2]));
    doc["Name"] = json!("Orca 2");
    assert_eq!(call(&s, "PUT", &format!("/api/scenes/{ORCA}"), Some(doc)).status, 200);
    let dir = t.join(format!("profiles/{ORCA}.sdProfile"));
    let txt = fs::read_to_string(dir.join("manifest.json")).unwrap();
    assert!(txt.starts_with("{\n    \"Actions\""), "Qt-like layout: sorted keys, 4 spaces");
    let disk = scene_file(&t, ORCA);
    assert_eq!((disk["Name"].as_str(), &disk["Unknown"], &disk["Actions"]["0,0"]["Mystery"]), (Some("Orca 2"), &json!({ "keep": [1, 2] }), &json!(7)));
    assert!(fs::read_to_string(dir.join("manifest.json.orcadeck.bak")).unwrap().contains("\"Orca\""));
    assert!(!dir.join("manifest.json.tmp").exists());

    // Schema validation rejects and leaves the file alone.
    for bad in [json!([]), json!({ "Name": 3 }), json!({ "Actions": [] }), json!({ "DeviceUUID": "AKP153E", "Actions": { "6,0": { "UUID": "x" } } }),
        json!({ "DeviceUUID": "AKP153E", "Actions": { "0,3": { "UUID": "x" } } }), json!({ "Actions": { "a,b": { "UUID": "x" } } }),
        json!({ "Actions": { "0,0": { "Name": "no uuid" } } }), json!({ "Actions": { "0,0": { "UUID": "x", "State": 2, "States": [{}] } } }),
        json!({ "Actions": { "0,0": { "UUID": "x", "Settings": [] } } }), json!({ "Actions": { "0,0": { "UUID": "x", "Actions": [{}] } } })] {
        let r = call(&s, "PUT", &format!("/api/scenes/{ORCA}"), Some(bad.clone()));
        assert_eq!(r.status, 400, "{bad}");
    }
    assert_eq!(fs::read_to_string(dir.join("manifest.json")).unwrap(), txt);
    // AKP815 scenes keep their own (taller) grid.
    let mut old = call(&s, "GET", &format!("/api/scenes/{OLD}"), None).json();
    old["Name"] = json!("x");
    assert_eq!(call(&s, "PUT", &format!("/api/scenes/{OLD}"), Some(old)).status, 200);

    // Bad ids never reach the filesystem.
    assert_eq!(call(&s, "GET", "/api/scenes/..%2F..%2Fsecret", None).status, 400);
    assert_eq!(call(&s, "GET", "/api/scenes/NOPE", None).status, 404);

    // New scene: inherits the device binding and joins the device order.
    let r = call(&s, "POST", "/api/scenes", Some(json!({ "name": "Nueva" })));
    assert_eq!(r.status, 201);
    let id = r.json()["id"].as_str().unwrap().to_string();
    let doc = scene_file(&t, &id);
    assert_eq!((doc["DeviceSerialNumber"].as_str(), doc["Pages"]["Pages"][0].as_str()), (Some("355499441494"), Some(&*format!("{id}.sdProfile"))));
    let list = call(&s, "GET", "/api/scenes", None).json();
    assert_eq!(list[2]["id"].as_str(), Some(&*id));
}

#[test]
fn cells_set_move_copy_delete() {
    let t = fixture("cells");
    let s = start(&t);
    let cell = |k: &str| format!("/api/scenes/{ORCA}/cells/{k}");

    // Dropping an action from the list: defaults from the manifest (with en.json name), fresh ActionID.
    let r = call(&s, "PUT", &cell("2,1"), Some(json!({ "UUID": "com.test.demo.one" })));
    assert_eq!(r.status, 200);
    let a = scene_file(&t, ORCA)["Actions"]["2,1"].clone();
    assert_eq!((a["Name"].as_str(), a["Controller"].as_str(), a["State"].as_u64()), (Some("One"), Some(""), Some(0)));
    assert_eq!(a["States"], json!([{ "Image": "imgs/one" }, { "Image": "imgs/one-b" }]));
    assert_eq!(a["Settings"], json!({ "default": true }));
    assert_eq!(a["ActionID"].as_str().unwrap().len(), 36);
    assert_eq!(scene_file(&t, ORCA)["Unknown"]["keep"], json!([1, 2]));

    // Side cell (column 5) and out-of-grid cells.
    assert_eq!(call(&s, "PUT", &cell("5,2"), Some(json!({ "UUID": "com.test.demo.two", "Settings": { "a": 1 } }))).status, 200);
    assert_eq!(scene_file(&t, ORCA)["Actions"]["5,2"]["Settings"], json!({ "a": 1 }));
    assert_eq!(call(&s, "PUT", &cell("6,0"), Some(json!({ "UUID": "x" }))).status, 400);
    assert_eq!(call(&s, "PUT", &cell("0,0"), Some(json!({ "Name": "no uuid" }))).status, 400);

    // A full action (has ActionID) is stored as given, unknown fields included.
    let mut full = scene_file(&t, ORCA)["Actions"]["0,0"].clone();
    full["Settings"] = json!({ "edited": "yes" });
    call(&s, "PUT", &cell("0,0"), Some(full));
    let a = scene_file(&t, ORCA)["Actions"]["0,0"].clone();
    assert_eq!((&a["Settings"], &a["Mystery"], &a["States"][0]["Extra"]), (&json!({ "edited": "yes" }), &json!(7), &json!(true)));

    // Move onto an empty cell, then onto an occupied one (swap).
    assert_eq!(call(&s, "POST", &format!("{}/move", cell("0,0")), Some(json!({ "to": "4,2" }))).status, 200);
    let acts = scene_file(&t, ORCA)["Actions"].clone();
    assert!(acts.get("0,0").is_none());
    assert_eq!(acts["4,2"]["UUID"], "com.test.demo.one");
    call(&s, "POST", &format!("{}/move", cell("4,2")), Some(json!({ "to": "1,0" })));
    let acts = scene_file(&t, ORCA)["Actions"].clone();
    assert_eq!((acts["1,0"]["UUID"].as_str(), acts["4,2"]["UUID"].as_str()), (Some("com.test.demo.one"), Some("com.test.demo.two")));

    // Copy gets a new ActionID and keeps the source.
    call(&s, "POST", &format!("{}/move", cell("1,0")), Some(json!({ "to": "3,0", "copy": true })));
    let acts = scene_file(&t, ORCA)["Actions"].clone();
    assert_eq!(acts["3,0"]["Settings"], acts["1,0"]["Settings"]);
    assert_ne!(acts["3,0"]["ActionID"], acts["1,0"]["ActionID"]);

    // Move to another scene.
    call(&s, "POST", &format!("{}/move", cell("3,0")), Some(json!({ "to": "0,0", "scene": MON })));
    assert!(scene_file(&t, ORCA)["Actions"].get("3,0").is_none());
    assert_eq!(scene_file(&t, MON)["Actions"]["0,0"]["UUID"], "com.test.demo.one");

    // Delete.
    assert_eq!(call(&s, "DELETE", &cell("1,0"), None).status, 200);
    assert!(scene_file(&t, ORCA)["Actions"].get("1,0").is_none());
    assert_eq!(call(&s, "DELETE", &cell("1,0"), None).status, 404);
    assert_eq!(call(&s, "POST", &format!("{}/move", cell("1,0")), Some(json!({ "to": "2,0" }))).status, 404);
}

#[test]
fn plugins_actions_images_device() {
    let t = fixture("plugins");
    let s = start(&t);
    let plugins = call(&s, "GET", "/api/plugins", None).json();
    let st = |u: &str| plugins.as_array().unwrap().iter().find(|p| p["uuid"] == u).map(|p| p.clone()).unwrap();
    assert_eq!((st("com.test.locked")["status"].as_str(), st("com.test.locked")["encrypted"].as_bool()), (Some("needs-manifest"), Some(true)));
    assert_eq!((st("com.test.overlaid")["status"].as_str(), st("com.test.overlaid")["overlay"].as_bool()), (Some("ok"), Some(true)));
    assert_eq!(st("com.test.overlaid")["runtime"], "exe");
    assert_eq!(st("com.test.broken")["status"], "invalid");
    assert_eq!((st("com.test.demo")["name"].as_str(), st("com.test.demo")["runtime"].as_str()), (Some("Demo"), Some("node")));
    assert_eq!(st("com.hotspot.streamdock.profile.rotate")["builtin"], true);
    assert_eq!(call(&s, "GET", "/api/plugins/com.test.demo/manifest", None).json()["Version"], "1.2");

    let acts = call(&s, "GET", "/api/actions", None).json();
    let groups: Vec<&str> = acts.as_array().unwrap().iter().map(|g| g["uuid"].as_str().unwrap()).collect();
    assert_eq!(groups, ["com.hotspot.streamdock.profile.rotate", "com.test.demo", "com.test.overlaid"]);
    let demo: Vec<&str> = acts[1]["actions"].as_array().unwrap().iter().map(|a| a["name"].as_str().unwrap()).collect();
    assert_eq!(demo, ["One", "Two"], "hidden action left out, en.json applied");

    let img = |q: &str| call(&s, "GET", &format!("/api/image?{q}"), None);
    let r = img(&format!("scene={ORCA}&uuid=com.test.demo.two&img=ARROW.png"));
    assert_eq!((r.status, r.body.as_slice()), (200, PNG));
    assert!(r.has("content-type: image/png") && r.has("content-security-policy: sandbox"));
    assert_eq!(img("uuid=com.test.demo.one&img=imgs/one").status, 200, "plugin-relative, extension probed");
    assert!(img("uuid=com.test.demo.two&img=").has("image/svg+xml"), "empty image falls back to the manifest");
    assert_eq!(img("uuid=com.hotspot.streamdock.profile.rotate&img=").status, 200, "built-in, action uuid = plugin uuid");
    assert_eq!(img("uuid=com.test.demo&img=").status, 404, "plugin icon missing on disk");
    assert_eq!(img("uuid=com.test.demo.one&img=../../secret.txt").status, 404);
    assert_eq!(img(&format!("scene={ORCA}&uuid=x&img=..%5C..%5C..%5Csecret.txt")).status, 404);

    let d = call(&s, "GET", "/api/device", None).json();
    assert_eq!((d["brightness"].as_u64(), d["connected"].as_bool()), (Some(70), Some(false)));
    assert_eq!(call(&s, "PUT", "/api/device", Some(json!({ "brightness": 40 }))).json()["brightness"], 40);
    assert_eq!(call(&s, "PUT", "/api/device", Some(json!({ "brightness": 140 }))).status, 400);
    assert_eq!(call(&s, "GET", "/api/nope", None).status, 404);
    s.stop();
    let _ = fs::remove_dir_all(&t);
}

#[test]
fn plugins_inbox_install_remove() {
    let t = fixture("inbox");
    let s = start(&t);

    // On-demand scan: recent installable candidates only, newest first.
    let inbox = call(&s, "GET", "/api/plugins/inbox", None).json();
    assert_eq!(inbox["dir"].as_str(), Some(t.join("downloads").to_string_lossy().as_ref()));
    let items = inbox["items"].as_array().unwrap();
    let names: Vec<&str> = items.iter().map(|i| i["name"].as_str().unwrap()).collect();
    assert!(names.contains(&"com.test.inbox.sdPlugin") && names.contains(&"bundle.zip"), "{names:?}");
    assert!(!names.iter().any(|n| n.ends_with(".txt")), "{names:?}");
    let ms: Vec<u64> = items.iter().map(|i| i["modified"].as_u64().unwrap()).collect();
    assert!(ms.windows(2).all(|w| w[0] >= w[1]), "newest first");

    let pkg = t.join("downloads/com.test.inbox.sdPlugin");
    let inspect = |p: &Path| call(&s, "POST", "/api/plugins/inspect", Some(json!({ "path": p.to_string_lossy() })));
    let r = inspect(&pkg);
    assert_eq!(r.status, 200, "{}", String::from_utf8_lossy(&r.body));
    let j = r.json();
    assert_eq!((j["uuid"].as_str(), j["name"].as_str(), j["actions"].as_u64()), (Some("com.test.inbox"), Some("Inbox"), Some(1)));
    assert_eq!((j["kind"].as_str(), j["installed"].as_bool()), (Some("folder"), Some(false)));
    // The SPA installs with the path returned by inspect (canonicalized, verbatim prefix on Windows).
    let inspected_path = j["path"].as_str().unwrap().to_string();

    // Only a package that canonicalizes inside Downloads reaches plugman.
    assert_eq!(inspect(&t.join("outside/com.test.evil.sdPlugin")).status, 400);
    assert_eq!(inspect(&t.join("downloads/../outside/com.test.evil.sdPlugin")).status, 400);
    assert_eq!(inspect(&t.join("downloads/notes.txt")).status, 400);
    assert_eq!(inspect(&t.join("downloads/nope.zip")).status, 404);

    // Install copies the folder and records it as ours.
    let install = |force: bool| call(&s, "POST", "/api/plugins/install", Some(json!({ "path": inspected_path, "force": force })));
    let r = install(false);
    assert_eq!(r.status, 200, "{}", String::from_utf8_lossy(&r.body));
    let j = r.json();
    assert_eq!((j["replaced"].as_bool(), j["needsManifest"].as_bool()), (Some(false), Some(false)));
    let dest = t.join("plugins/com.test.inbox.sdPlugin");
    assert!(dest.join("manifest.json").is_file());
    let plugins = call(&s, "GET", "/api/plugins", None).json();
    let find = |u: &str| plugins.as_array().unwrap().iter().find(|p| p["uuid"] == u).unwrap().clone();
    let origin = |u: &str| (find(u)["origin"].as_str().unwrap().to_string(), find(u)["removable"].as_bool().unwrap());
    assert_eq!(origin("com.test.inbox"), ("orcadeck".into(), true));
    assert_eq!(origin("com.test.demo"), ("streamdock".into(), false));
    assert_eq!(origin("com.hotspot.streamdock.profile.rotate"), ("builtin".into(), false));

    // Replacing needs an explicit confirmation in the request.
    assert_eq!(install(false).status, 409);
    let r = install(true);
    assert_eq!(r.status, 200);
    assert_eq!(r.json()["replaced"], true);

    // Deletion: only what this editor installed. Built-in and Stream Dock's plugin stay untouched.
    let del = |u: &str| call(&s, "DELETE", &format!("/api/plugins/{u}"), None).status;
    assert_eq!(del("com.test.demo"), 403);
    assert!(t.join("plugins/com.test.demo.sdPlugin").is_dir());
    assert_eq!(del("com.hotspot.streamdock.profile.rotate"), 403);
    assert_eq!(call(&s, "DELETE", "/api/plugins/..%2F..%2Fsecret", None).status, 400);
    assert_eq!(del("com.test.inbox"), 200);
    assert!(!dest.exists());
    assert_eq!(del("com.test.inbox"), 403);
    assert!(!call(&s, "GET", "/api/plugins", None).json().as_array().unwrap().iter().any(|p| p["uuid"] == "com.test.inbox"));
    s.stop();
    let _ = fs::remove_dir_all(&t);
}
