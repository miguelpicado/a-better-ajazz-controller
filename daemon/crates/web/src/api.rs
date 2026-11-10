//! Request routing, the security gate (Host/Origin/token) and the JSON endpoints. See README.md.
use crate::http::{pct_decode, Request, Response};
use crate::store::{safe_id, Error, Plugin, Result, Store};
use serde_json::{json, Map, Value};
use std::sync::{Arc, Mutex};

const INDEX: &str = include_str!("../web/index.html");
const APP_JS: &str = include_str!("../web/app.js");
const APP_CSS: &str = include_str!("../web/app.css");
const ICON: &str = include_str!("../web/icon.svg");

const PAGE_CSP: &str = "default-src 'none'; script-src 'self'; style-src 'self'; img-src 'self' data:; \
                        connect-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'none'";

pub struct Ctx {
    pub store: Arc<dyn Store>,
    pub token: String,
    pub port: u16,
    /// ponytail: one lock for every read-modify-write; per-scene locks if it ever matters.
    pub lock: Mutex<()>,
}

impl Ctx {
    fn cookie_name(&self) -> String {
        format!("orcadeck_{}", self.port)
    }

    fn host_ok(&self, h: &str) -> bool {
        h == format!("127.0.0.1:{}", self.port) || h == format!("localhost:{}", self.port)
    }

    fn token_ok(&self, given: Option<&str>) -> bool {
        // Constant-time compare.
        given.is_some_and(|g| {
            g.len() == self.token.len() && g.bytes().zip(self.token.bytes()).fold(0u8, |a, (x, y)| a | (x ^ y)) == 0
        })
    }
}

/// DNS-rebinding and CSRF gate, applied to every request before routing.
fn gate(ctx: &Ctx, req: &Request) -> Option<Response> {
    if !req.header("host").is_some_and(|h| ctx.host_ok(h)) {
        return Some(Response::error(403, "bad Host"));
    }
    if let Some(o) = req.header("origin") {
        if !o.strip_prefix("http://").is_some_and(|h| ctx.host_ok(h)) {
            return Some(Response::error(403, "bad Origin"));
        }
    }
    if req.header("sec-fetch-site") == Some("cross-site") {
        return Some(Response::error(403, "cross-site request"));
    }
    None
}

pub fn handle(ctx: &Ctx, req: Request) -> Response {
    if let Some(r) = gate(ctx, &req) {
        return r;
    }
    let page = |ctype: &'static str, body: &str| Response::new(200, ctype, body).with("Content-Security-Policy", PAGE_CSP);
    match (req.method.as_str(), req.path.as_str()) {
        ("GET", "/") => {
            if let Some(t) = req.query_param("token") {
                if ctx.token_ok(Some(&t)) {
                    // Swap the URL token for an HttpOnly cookie and drop it from the address bar.
                    return Response::new(303, "text/plain", "").with("Location", "/").with(
                        "Set-Cookie",
                        format!("{}={}; Path=/; HttpOnly; SameSite=Strict", ctx.cookie_name(), ctx.token),
                    );
                }
            }
            page("text/html; charset=utf-8", INDEX)
        }
        ("GET", "/app.js") => page("text/javascript; charset=utf-8", APP_JS),
        ("GET", "/app.css") => page("text/css; charset=utf-8", APP_CSS),
        ("GET", "/icon.svg") => page("image/svg+xml", ICON),
        (_, p) if p.starts_with("/api/") => {
            let given = req.header("x-orcadeck-token").or_else(|| req.cookie(&ctx.cookie_name()));
            if !ctx.token_ok(given) {
                return Response::error(401, "missing or bad session token");
            }
            if matches!(req.method.as_str(), "POST" | "PUT")
                && !req.header("content-type").is_some_and(|c| c.starts_with("application/json"))
            {
                return Response::error(415, "Content-Type must be application/json");
            }
            match api(ctx, &req) {
                Ok(r) => r,
                Err(e) => Response::error(e.status, &e.msg),
            }
        }
        _ => Response::error(404, "not found"),
    }
}

fn body(req: &Request) -> Result<Value> {
    serde_json::from_slice(&req.body).map_err(|e| Error::new(400, format!("bad JSON: {e}")))
}

fn str_field<'a>(b: &'a Value, k: &str) -> Result<&'a str> {
    b.get(k).and_then(Value::as_str).filter(|s| !s.is_empty())
        .ok_or_else(|| Error::new(400, format!("expected {{{k}}}")))
}

fn ok(v: Value) -> Result<Response> {
    Ok(Response::json(200, &v))
}

fn api(ctx: &Ctx, req: &Request) -> Result<Response> {
    let segs: Vec<String> = req.path["/api/".len()..].split('/').map(pct_decode).collect();
    let s: Vec<&str> = segs.iter().map(String::as_str).collect();
    let store = &*ctx.store;
    let m = req.method.as_str();
    for id in s.iter().skip(1).take(1) {
        if !safe_id(id) {
            return Err(Error::new(400, "bad id"));
        }
    }
    let _guard = (m != "GET").then(|| ctx.lock.lock().unwrap_or_else(|e| e.into_inner()));
    match (m, s.as_slice()) {
        ("GET", ["scenes"]) => ok(Value::Array(store.scenes()?.into_iter().map(|s| json!({
            "id": s.id, "name": s.name, "device": s.device, "keys": s.keys, "listed": s.listed, "current": s.current,
        })).collect())),
        ("POST", ["scenes"]) => create_scene(store, &body(req)?),
        ("PUT", ["scenes", "order"]) => {
            let ids: Vec<String> = serde_json::from_value(body(req)?).map_err(|_| Error::new(400, "expected [ids]"))?;
            let known = store.scenes()?;
            if let Some(bad) = ids.iter().find(|i| !known.iter().any(|k| &k.id == *i)) {
                return Err(Error::new(400, format!("unknown scene {bad}")));
            }
            store.set_order(&ids)?;
            ok(json!(ids))
        }
        ("PUT", ["scenes", "current"]) => {
            let b = body(req)?;
            let id = b.get("id").and_then(Value::as_str).filter(|i| safe_id(i)).ok_or_else(|| Error::new(400, "expected {id}"))?;
            store.scene(id)?;
            store.set_current(id)?;
            ok(json!({ "id": id }))
        }
        ("GET", ["scenes", id]) => ok(store.scene(id)?),
        ("PUT", ["scenes", id]) => {
            let doc = body(req)?;
            validate_scene(&doc).map_err(|e| Error::new(400, e))?;
            store.put_scene(id, &doc)?;
            ok(doc)
        }
        ("PUT", ["scenes", id, "cells", key]) => {
            let mut doc = store.scene(id)?;
            let key = cell_key(&doc, key)?;
            let a = build_action(store, body(req)?)?;
            actions(&mut doc).insert(key, a.clone());
            save(store, id, &doc)?;
            ok(a)
        }
        ("DELETE", ["scenes", id, "cells", key]) => {
            let mut doc = store.scene(id)?;
            let key = cell_key(&doc, key)?;
            let gone = actions(&mut doc).remove(&key).ok_or_else(|| Error::new(404, "empty cell"))?;
            save(store, id, &doc)?;
            ok(gone)
        }
        ("POST", ["scenes", id, "cells", key, "move"]) => move_cell(store, id, key, &body(req)?),
        ("GET", ["actions"]) => ok(action_list(&store.plugins()?)),
        ("GET", ["plugins"]) => ok(Value::Array(store.plugins()?.iter().map(plugin_json).collect())),
        ("GET", ["plugins", "inbox"]) => ok(store.plugin_inbox()?),
        ("POST", ["plugins", "inspect"]) => {
            let b = body(req)?;
            ok(store.plugin_inspect(str_field(&b, "path")?)?)
        }
        ("POST", ["plugins", "install"]) => {
            let b = body(req)?;
            let force = b.get("force").and_then(Value::as_bool).unwrap_or(false);
            ok(store.plugin_install(str_field(&b, "path")?, force)?)
        }
        ("GET", ["plugins", uuid, "manifest"]) => {
            let p = store.plugins()?.into_iter().find(|p| p.uuid == *uuid).ok_or_else(|| Error::new(404, "no such plugin"))?;
            ok(p.manifest)
        }
        ("DELETE", ["plugins", uuid]) => ok(store.plugin_remove(uuid)?),
        ("GET", ["image"]) => image(store, req),
        ("GET", ["device"]) => ok(store.device()?),
        ("PUT", ["device"]) => ok(store.set_device(&body(req)?)?),
        _ => Err(Error::new(404, "no such endpoint")),
    }
}

fn save(store: &dyn Store, id: &str, doc: &Value) -> Result<()> {
    validate_scene(doc).map_err(|e| Error::new(400, e))?;
    store.put_scene(id, doc)
}

fn actions(doc: &mut Value) -> &mut Map<String, Value> {
    if !doc.get("Actions").is_some_and(Value::is_object) {
        doc["Actions"] = json!({});
    }
    doc["Actions"].as_object_mut().unwrap()
}

/// Grid bounds per device model. AKP153E: 5×3 keys + column 5 = the 3 side cells.
fn bounds(doc: &Value) -> (u32, u32) {
    match doc.get("DeviceUUID").and_then(Value::as_str) {
        Some("AKP153E") => (5, 2),
        _ => (15, 15),
    }
}

fn parse_key(k: &str) -> Option<(u32, u32)> {
    let (c, r) = k.split_once(',')?;
    let digits = |s: &str| !s.is_empty() && s.len() <= 2 && s.bytes().all(|b| b.is_ascii_digit());
    (digits(c) && digits(r)).then(|| (c.parse().unwrap(), r.parse().unwrap()))
}

fn cell_key(doc: &Value, k: &str) -> Result<String> {
    let (maxc, maxr) = bounds(doc);
    match parse_key(k) {
        Some((c, r)) if c <= maxc && r <= maxr => Ok(format!("{c},{r}")),
        _ => Err(Error::new(400, format!("bad cell {k:?}"))),
    }
}

fn validate_action(a: &Value, at: &str) -> std::result::Result<(), String> {
    let o = a.as_object().ok_or(format!("{at}: action must be an object"))?;
    if !o.get("UUID").and_then(Value::as_str).is_some_and(|u| !u.is_empty()) {
        return Err(format!("{at}: UUID must be a non-empty string"));
    }
    for k in ["Name", "ActionID", "Controller"] {
        if o.get(k).is_some_and(|v| !v.is_string()) {
            return Err(format!("{at}: {k} must be a string"));
        }
    }
    if o.get("Settings").is_some_and(|v| !v.is_object()) {
        return Err(format!("{at}: Settings must be an object"));
    }
    let states = match o.get("States") {
        None => 0,
        Some(Value::Array(s)) if s.iter().all(Value::is_object) => s.len(),
        _ => return Err(format!("{at}: States must be an array of objects")),
    };
    match o.get("State") {
        None => {}
        Some(v) => match v.as_u64() {
            Some(i) if states == 0 || (i as usize) < states => {}
            _ => return Err(format!("{at}: State must index States")),
        },
    }
    match o.get("Actions") {
        None => Ok(()),
        Some(Value::Array(nested)) => nested.iter().enumerate().try_for_each(|(i, n)| validate_action(n, &format!("{at}/{i}"))),
        _ => Err(format!("{at}: Actions must be an array")),
    }
}

/// Schema check from docs/research/manifest-profiles-admin.md §2.3. Unknown fields are allowed and kept.
pub fn validate_scene(doc: &Value) -> std::result::Result<(), String> {
    let o = doc.as_object().ok_or("scene must be a JSON object")?;
    if o.get("Name").is_some_and(|v| !v.is_string()) {
        return Err("Name must be a string".into());
    }
    let Some(acts) = o.get("Actions") else { return Ok(()) };
    let acts = acts.as_object().ok_or("Actions must be an object keyed \"col,row\"")?;
    let (maxc, maxr) = bounds(doc);
    for (k, a) in acts {
        match parse_key(k) {
            Some((c, r)) if c <= maxc && r <= maxr => validate_action(a, k)?,
            _ => return Err(format!("bad cell key {k:?}")),
        }
    }
    Ok(())
}

fn find_action<'a>(plugins: &'a [Plugin], uuid: &str) -> Option<(&'a Plugin, &'a Value)> {
    plugins.iter().find_map(|p| {
        p.manifest.get("Actions")?.as_array()?.iter()
            .find(|a| a.get("UUID").and_then(Value::as_str) == Some(uuid)).map(|a| (p, a))
    })
}

/// Plugin that owns an action UUID: declared in its manifest, else the longest `<plugin>.` prefix.
fn owner<'a>(plugins: &'a [Plugin], uuid: &str) -> Option<&'a Plugin> {
    find_action(plugins, uuid).map(|(p, _)| p).or_else(|| {
        plugins.iter().filter(|p| uuid == p.uuid || uuid.starts_with(&format!("{}.", p.uuid))).max_by_key(|p| p.uuid.len())
    })
}

/// A body with an `ActionID` is a complete action and is stored as given. Otherwise it is new: defaults come
/// from the plugin manifest, mirroring tools/streamdock/profiles.mjs (`ActionID, Controller:"", Name,
/// Settings, State:0, States, UUID`); any other fields in the body are kept on top.
fn build_action(store: &dyn Store, b: Value) -> Result<Value> {
    validate_action(&b, "action").map_err(|e| Error::new(400, e))?;
    if b.get("ActionID").is_some() {
        return Ok(b);
    }
    let uuid = b["UUID"].as_str().unwrap_or_default().to_string();
    let plugins = store.plugins()?;
    let def = find_action(&plugins, &uuid).map(|(_, a)| a.clone()).unwrap_or(Value::Null);
    let mut a = json!({
        "ActionID": crate::new_id(false),
        "Controller": "",
        "Name": def.get("Name").cloned().unwrap_or(json!("")),
        "Settings": def.get("Settings").filter(|s| s.is_object()).cloned().unwrap_or(json!({})),
        "State": 0,
        "States": def.get("States").filter(|s| s.is_array()).cloned().unwrap_or(json!([{ "Image": "" }])),
        "UUID": uuid,
    });
    for (k, v) in b.as_object().unwrap() {
        a[k] = v.clone();
    }
    Ok(a)
}

fn move_cell(store: &dyn Store, id: &str, key: &str, b: &Value) -> Result<Response> {
    let to = b.get("to").and_then(Value::as_str).ok_or_else(|| Error::new(400, "expected {to:\"c,r\"}"))?;
    let dst_id = b.get("scene").and_then(Value::as_str).unwrap_or(id);
    if !safe_id(dst_id) {
        return Err(Error::new(400, "bad scene id"));
    }
    let copy = b.get("copy").and_then(Value::as_bool).unwrap_or(false);
    let mut src = store.scene(id)?;
    let from = cell_key(&src, key)?;
    let a = actions(&mut src).get(&from).cloned().ok_or_else(|| Error::new(404, "empty cell"))?;
    let mut dst = if dst_id == id { None } else { Some(store.scene(dst_id)?) };
    let to = cell_key(dst.as_ref().unwrap_or(&src), to)?;
    if dst.is_none() && from == to {
        return ok(json!({ "moved": false }));
    }
    let placed = if copy {
        let mut c = a.clone();
        c["ActionID"] = json!(crate::new_id(false));
        c
    } else {
        a
    };
    // A move onto an occupied cell swaps; a copy overwrites.
    let displaced = actions(dst.as_mut().unwrap_or(&mut src)).insert(to.clone(), placed);
    if !copy {
        match displaced {
            Some(d) => actions(&mut src).insert(from.clone(), d),
            None => actions(&mut src).remove(&from),
        };
    }
    if let Some(d) = &dst {
        save(store, dst_id, d)?;
    }
    if dst.is_none() || !copy {
        save(store, id, &src)?;
    }
    ok(json!({ "moved": !copy, "copied": copy, "to": to, "scene": dst_id }))
}

fn create_scene(store: &dyn Store, b: &Value) -> Result<Response> {
    let name = b.get("name").and_then(Value::as_str).map(str::trim).filter(|n| !n.is_empty())
        .ok_or_else(|| Error::new(400, "expected {name}"))?;
    let scenes = store.scenes()?;
    // Inherit the device binding from the scene the device shows now.
    let tpl = scenes.iter().find(|s| s.current).and_then(|s| store.scene(&s.id).ok()).unwrap_or(json!({}));
    let id = crate::new_id(true);
    let page = format!("{id}.sdProfile");
    let mut doc = json!({
        "Name": name, "Version": "1.0", "AppIdentifier": "",
        "DeviceUUID": tpl.get("DeviceUUID").cloned().unwrap_or(json!("AKP153E")),
        "Pages": { "Current": page, "Pages": [page] }, "Actions": {},
    });
    for k in ["DeviceModel", "DeviceSerialNumber"] {
        if let Some(v) = tpl.get(k) {
            doc[k] = v.clone();
        }
    }
    store.put_scene(&id, &doc)?;
    let mut order: Vec<String> = scenes.into_iter().filter(|s| s.listed).map(|s| s.id).collect();
    order.push(id.clone());
    store.set_order(&order)?;
    Ok(Response::json(201, &json!({ "id": id, "scene": doc })))
}

fn runtime(m: &Value) -> &'static str {
    if m.get("UUID").and_then(Value::as_str) == Some("com.miguelpicado.orcadock") {
        return "builtin";
    }
    let code = m.get("CodePathWin").or_else(|| m.get("CodePath")).and_then(Value::as_str).unwrap_or("").to_ascii_lowercase();
    match () {
        _ if m.is_null() => "unknown",
        _ if code.ends_with(".exe") => "exe",
        _ if code.ends_with(".html") || code.ends_with(".htm") => "html",
        _ if code.ends_with(".js") || code.ends_with(".mjs") || code.ends_with(".cjs") => "node",
        _ if code.is_empty() => "builtin",
        _ => "other",
    }
}

fn plugin_json(p: &Plugin) -> Value {
    let m = &p.manifest;
    let s = |k: &str| m.get(k).and_then(Value::as_str).unwrap_or("").trim().to_string();
    let icon = [s("Icon"), s("CategoryIcon")].into_iter().find(|i| !i.is_empty())
        .or_else(|| m.pointer("/Actions/0/Icon").and_then(Value::as_str).map(String::from)).unwrap_or_default();
    json!({
        "uuid": p.uuid, "name": if s("Name").is_empty() { p.uuid.clone() } else { s("Name") },
        "version": s("Version"), "author": s("Author"), "description": s("Description"),
        "category": s("Category"), "status": p.status, "icon": icon, "encrypted": p.encrypted,
        "overlay": p.overlay, "builtin": p.builtin, "runtime": runtime(m),
        "origin": p.origin, "removable": p.removable,
        "actions": m.get("Actions").and_then(Value::as_array).map_or(0, Vec::len),
        "dir": p.dir.to_string_lossy(),
    })
}

fn action_list(plugins: &[Plugin]) -> Value {
    Value::Array(plugins.iter().filter(|p| p.status == "ok").filter_map(|p| {
        let acts: Vec<Value> = p.manifest.get("Actions")?.as_array()?.iter()
            .filter(|a| a.get("VisibleInActionsList").and_then(Value::as_bool) != Some(false))
            .filter_map(|a| Some(json!({
                "uuid": a.get("UUID")?.as_str()?,
                "name": a.get("Name").and_then(Value::as_str).unwrap_or("").trim(),
                "tooltip": a.get("Tooltip").cloned().unwrap_or(json!("")),
                "icon": a.get("Icon").and_then(Value::as_str).filter(|s| !s.is_empty())
                    .or_else(|| a.pointer("/States/0/Image").and_then(Value::as_str).filter(|s| !s.is_empty()))
                    .unwrap_or(""),
                "controllers": a.get("Controllers").cloned().unwrap_or(json!(["Keypad"])),
            }))).collect();
        (!acts.is_empty()).then(|| {
            let mut j = plugin_json(p);
            j["actions"] = Value::Array(acts);
            j
        })
    }).collect())
}

/// `?scene=<id>&uuid=<action or plugin uuid>&img=<States[].Image or Icon>[&state=N]`. An empty `img` falls
/// back to the plugin manifest's state image, then its icon.
fn image(store: &dyn Store, req: &Request) -> Result<Response> {
    let scene = req.query_param("scene");
    let uuid = req.query_param("uuid").unwrap_or_default();
    let mut img = req.query_param("img").unwrap_or_default();
    let plugins = store.plugins()?;
    let own = owner(&plugins, &uuid);
    if img.is_empty() {
        let state: usize = req.query_param("state").and_then(|s| s.parse().ok()).unwrap_or(0);
        if let Some((_, a)) = find_action(&plugins, &uuid) {
            let st = a.get("States").and_then(|s| s.get(state).or_else(|| s.get(0)));
            img = st.and_then(|s| s.get("Image")).and_then(Value::as_str).filter(|s| !s.is_empty())
                .or_else(|| a.get("Icon").and_then(Value::as_str)).unwrap_or("").to_string();
        } else if let Some(p) = own {
            img = p.manifest.get("Icon").and_then(Value::as_str).unwrap_or("").to_string();
        }
    }
    let (bytes, ctype) = store.image(scene.as_deref(), own.map(|p| p.uuid.as_str()), &img)
        .ok_or_else(|| Error::new(404, "image not found"))?;
    // Plugin SVGs are third-party content: never let them run script in this origin.
    Ok(Response::new(200, ctype, bytes).with("Content-Security-Policy", "sandbox; default-src 'none'; style-src 'unsafe-inline'"))
}
