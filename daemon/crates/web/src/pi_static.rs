//! Static files for third-party property inspectors: `GET /api/plugins/<uuid>/pi/<file>`.
//!
//! The iframe is sandboxed without `allow-same-origin`, so this content runs with an opaque origin:
//! it cannot read the editor's cookie, token, DOM or API. Assets are served from this origin because
//! the plugin host only receives the single-use PI token, never the editor token. Containment here is
//! path-based: `safe_id` + decoded-segment checks + canonicalize, so nothing outside the plugin folder
//! is ever read. CSP blocks external network by default (§9.2 of docs/research/inspector.md).
use crate::http::{pct_decode, Request, Response};
use crate::store::{safe_id, Store};
use std::{fs, path::Path};

/// Injected into PI HTML: waits for the editor's `postMessage` (port, single-use token, info) and
/// performs the Stream Deck bootstrap. Keeping credentials out of the URL avoids them leaking through
/// redirects or logs.
const BOOTSTRAP: &str = r#"<script>(function(){function h(e){if(e.source!==parent||!e.data||e.data.type!=='orcadeck-pi-credentials')return;var d=e.data;if(typeof window.connectElgatoStreamDeckSocket!=='function')return;removeEventListener('message',h);window.connectElgatoStreamDeckSocket(d.port,d.token,'registerPropertyInspector',JSON.stringify(d.info||{}),JSON.stringify(d.actionInfo||{}));}addEventListener('message',h);parent.postMessage({type:'orcadeck-pi-ready'},'*');})();</script>"#;

/// Matches `/api/plugins/<uuid>/pi/<file...>`; `None` means "not a PI request".
pub fn route(store: &dyn Store, req: &Request, editor_port: u16) -> Option<Response> {
    let rest = req.path.strip_prefix("/api/plugins/")?;
    let mut segments = rest.split('/');
    let uuid = pct_decode(segments.next()?);
    if segments.next()? != "pi" {
        return None;
    }
    let parts: Vec<String> = segments.map(pct_decode).collect();
    Some(serve(store, &uuid, &parts, req, editor_port))
}

fn serve(store: &dyn Store, uuid: &str, parts: &[String], req: &Request, editor_port: u16) -> Response {
    let host = req.header("host").unwrap_or_default();
    let origin = |h: &str| h == format!("127.0.0.1:{editor_port}") || h == format!("localhost:{editor_port}");
    if !origin(host) {
        return Response::error(403, "bad Host");
    }
    if let Some(o) = req.header("origin") {
        // `null` is the sandboxed PI fetching its own assets (opaque origin).
        if o != "null" && !o.strip_prefix("http://").is_some_and(origin) {
            return Response::error(403, "bad Origin");
        }
    }
    if !safe_id(uuid) {
        return Response::error(400, "bad plugin id");
    }
    let Some(dir) = store.plugin_dir(uuid) else { return Response::error(404, "no such plugin") };
    let Some((path, ctype)) = resolve(&dir, parts) else { return Response::error(404, "not found") };
    let Ok(bytes) = fs::read(&path) else { return Response::error(404, "not found") };
    let body = if ctype.starts_with("text/html") {
        let mut html = String::from_utf8_lossy(&bytes).into_owned();
        html.push_str(BOOTSTRAP);
        html.into_bytes()
    } else {
        bytes
    };
    let origin = format!("http://127.0.0.1:{editor_port}");
    let ws = req.query_param("ws").and_then(|p| p.parse::<u16>().ok()).filter(|p| *p != 0);
    let connect = ws.map_or_else(|| "'none'".to_string(), |p| format!("ws://127.0.0.1:{p}"));
    let csp = format!(
        "default-src 'none'; script-src {origin} 'unsafe-inline'; style-src {origin} 'unsafe-inline'; \
         img-src {origin} data: blob:; font-src {origin} data:; media-src {origin} data:; \
         connect-src {connect}; frame-ancestors {origin}; base-uri 'none'; form-action 'none'"
    );
    Response::new(200, ctype, body)
        .with("Content-Security-Policy", csp)
        .with("X-Frame-Options", "SAMEORIGIN")
}

/// Resolves decoded path segments inside `base`: no `..`, absolute paths, drive letters, backslashes
/// or NUL, and the canonical file must still be under the canonical plugin folder.
pub(crate) fn resolve(base: &Path, parts: &[String]) -> Option<(std::path::PathBuf, &'static str)> {
    let rel: Vec<&str> = parts.iter().map(String::as_str).collect();
    if rel.is_empty()
        || rel.iter().any(|s| s.is_empty() || *s == "." || *s == ".." || s.contains(['\\', '/', ':', '\0']))
    {
        return None;
    }
    let root = base.canonicalize().ok()?;
    let file = rel.iter().fold(base.to_path_buf(), |p, s| p.join(s)).canonicalize().ok()?;
    (file.starts_with(&root) && file.is_file()).then(|| (file.clone(), mime(&file)))
}

fn mime(p: &Path) -> &'static str {
    match p.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).as_deref() {
        Some("html" | "htm") => "text/html; charset=utf-8",
        Some("js" | "mjs") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("json" | "map") => "application/json",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("ico") => "image/x-icon",
        Some("woff") => "font/woff",
        Some("woff2") => "font/woff2",
        Some("ttf") => "font/ttf",
        Some("otf") => "font/otf",
        Some("wasm") => "application/wasm",
        Some("txt" | "md") => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}
