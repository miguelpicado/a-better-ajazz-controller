# orcadeck-web

Local web editor for the OrcaDeck daemon. It is a library crate. The daemon calls `Server::start` when the user
asks to edit, opens `server.url()` in the browser, and drops the `Server` when done. Nothing runs while it is
stopped.

- HTTP on `std::net`, hand-written (`src/http.rs`, about 150 lines). One request per connection, then
  `Connection: close`. Only `Content-Length` bodies are accepted (16 KB of headers, 4 MB of body). The accept
  loop blocks, so the idle server uses 0 % CPU: measured 0 ms of CPU in 10 s, 2 threads, about 11 MB working set
  (debug build). `tiny_http` would bring 4 crates and its own thread pool for the same job. `axum` would bring
  tokio, which the daemon does not otherwise need yet.
- Dependencies: `serde_json` (plus `serde`, already a transitive dependency, used for the 4-space printer) and
  `getrandom` for the token and ids, plus the workspace crate `orcadeck-plugman` for installing packages. No new
  external crates: `Cargo.lock` gains one path-dependency line, no packages.
- Frontend: `web/` (`index.html`, `app.js`, `app.css`, `icon.svg`) is embedded with `include_str!`. Plain DOM
  and CSS, no build step, no web fonts, 44 KB in total.

```rust
let store = Arc::new(orcadeck_web::FsStore {
    profiles: data.join("profiles"),        // <id>.sdProfile/manifest.json, written on save
    plugins: data.join("plugins"),          // installed plugins: reads, and writes by install/remove
    builtin_plugins: vec![streamdock_install.join("plugins")], // read only
    data: data.clone(),                     // state.json + manifests/<uuid>.json overlays
    legacy_config: Some(hotspot.join("config")), // Stream Dock config, read only (order, current scene, brightness)
    downloads: None,                        // None = %USERPROFILE%\Downloads, scanned on demand
    inspector: None,                        // the daemon fills this while `run` is active
});
let server = orcadeck_web::Server::start(store, 0)?; // 0 = any free port
open_browser(&server.url());                          // http://127.0.0.1:<port>/?token=<64 hex>
```

To try it by hand over a copy of your profiles (never Stream Dock's own folder):

```
cargo run --example serve -- <copy-root> --plugins %APPDATA%\HotSpot\StreamDock\plugins \
  --builtin "C:\Program Files (x86)\Stream Dock AJAZZ Global\plugins" --legacy %APPDATA%\HotSpot\StreamDock\config
```

## Security

| Threat | Defence |
|---|---|
| Other machines | Binds `127.0.0.1` only. |
| Any web page in the browser calling the API | Random 256-bit token per server start, required on every `/api/*` call as the `X-OrcaDeck-Token` header or the `orcadeck_<port>` cookie. Constant-time compare. |
| Token left in the address bar or history | `GET /?token=…` answers `303 → /` with `Set-Cookie: HttpOnly; SameSite=Strict`. |
| DNS rebinding | `Host` must be exactly `127.0.0.1:<port>` or `localhost:<port>`, on every request including static files. |
| CSRF | `Origin`, when present, must be `http://127.0.0.1:<port>` or `http://localhost:<port>`. `Sec-Fetch-Site: cross-site` is refused. `POST`/`PUT` need `Content-Type: application/json` (an HTML form cannot send that). The cookie is `SameSite=Strict`. |
| Cross-origin reads | No CORS headers at all. `OPTIONS` is not routed. |
| Script from plugin images (SVG) | Images are served with `Content-Security-Policy: sandbox`. Pages get a strict CSP (`default-src 'none'`, scripts and styles only from `'self'`), `X-Frame-Options: DENY` and `nosniff`. |
| Path traversal | Scene and plugin ids are limited to `[0-9A-Za-z._-]` without `..`. Image paths reject `..`, absolute paths and drive letters, and the resolved file must stay inside its base folder after `canonicalize`. Install/inspect paths must `canonicalize` inside the Downloads folder, and removal requires the canonical plugin folder to be a direct child of the plugins folder. |

Tests for all of this are in `tests/api.rs::security_gate`.

Known limit: browser cookies are scoped by host, not by port. Another server on `127.0.0.1` would receive the
cookie if the browser visited it. Local processes already run as the same user, so this is accepted.

## API

All bodies are JSON. Errors are `{"error": "..."}` with a 4xx/5xx status.

| Method and path | Body | Result |
|---|---|---|
| `GET /api/scenes` | | `[{id, name, device, keys, listed, current}]`. Device scenes first, in device order (`listed`), then the rest by name. |
| `POST /api/scenes` | `{name}` | `201 {id, scene}`. New scene that copies the device binding (`DeviceUUID`, `DeviceModel`, `DeviceSerialNumber`) of the current scene and is added to the end of the device order. |
| `PUT /api/scenes/order` | `[id, …]` | The device's ordered scene list. Scenes left out stay on disk as "other scenes". |
| `PUT /api/scenes/current` | `{id}` | Scene the device shows. |
| `GET /api/scenes/{id}` | | The scene `manifest.json` as is. |
| `PUT /api/scenes/{id}` | full manifest | Validates, then writes atomically (`.tmp` + rename) and keeps the previous file as `manifest.json.orcadeck.bak`. Full replace: send back what you read and unknown fields survive. |
| `PUT /api/scenes/{id}/cells/{c,r}` | action | Sets a cell. A body with `ActionID` is stored as given. A body without one (for example `{"UUID": "…"}` dropped from the list) is a new action, built like `tools/streamdock/profiles.mjs` does: new `ActionID`, `Controller: ""`, and `Name`, `Settings` and `States` from the plugin manifest. Other fields in the body override those. |
| `DELETE /api/scenes/{id}/cells/{c,r}` | | Removes the action and returns it. |
| `POST /api/scenes/{id}/cells/{c,r}/move` | `{to: "c,r", scene?, copy?}` | Moves to `to`, in this scene or in `scene`. A move onto an occupied cell swaps the two. A copy overwrites the target and gets a new `ActionID`. |
| `GET /api/actions` | | Plugins whose status is `ok`, each with `actions: [{uuid, name, tooltip, icon, controllers}]`. Actions with `VisibleInActionsList: false` are left out. |
| `GET /api/plugins` | | `[{uuid, name, version, author, description, category, icon, status, encrypted, overlay, builtin, runtime, origin, removable, actions, dir}]`. `status` is `ok`, `needs-manifest` (Elgato-encrypted with no overlay) or `invalid`. `runtime` is `exe`, `node`, `html`, `builtin`, `other` or `unknown`. `origin` is `builtin`, `streamdock` (already present before OrcaDeck) or `orcadeck`; only the latter has `removable: true`. |
| `GET /api/plugins/{uuid}/manifest` | | The manifest in use (the overlay if there is one), with `en.json` names applied. |
| `GET /api/plugins/inbox` | | `{dir, items: [{name, path, kind, size, modified}]}`. One scan of `downloads` when called: `.streamDeckPlugin`/`.zip` files and `.sdPlugin` folders with a manifest, modified in the last 30 days, newest first, at most 20. Nothing watches the folder. |
| `POST /api/plugins/inspect` | `{path}` | What the package contains, before installing: `{path, kind, uuid, name, version, author, description, actions, encrypted, confidence, needsManifest, installed}`. `path` must resolve (after `canonicalize`) inside `downloads`. |
| `POST /api/plugins/install` | `{path, force?}` | `{uuid, dir, overlay, needsManifest, replaced}`. Inspects first to validate the UUID, then `plugman::install_package`; `force: true` replaces an existing install and leaves a `<uuid>.sdPlugin.bak-<timestamp>` backup. `409` when it already exists and `force` is not set. Recorded in `state.json` as installed by OrcaDeck. |
| `DELETE /api/plugins/{uuid}` | | Removes the plugin folder and its overlay, only when OrcaDeck installed it. `403` for built-ins and for plugins that were already there. |
| `GET /api/image?scene=&uuid=&img=&state=` | | Image bytes. A bare `img` name is looked up in `<scene>/Images/`. Otherwise it is taken relative to the plugin that owns `uuid` (an action or plugin uuid), trying `""`, `.png`, `@2x.png`, `.svg`, `.gif`, `.jpg` and `.jpeg`. An empty `img` falls back to the manifest's state image, then its `Icon`. |
| `GET /api/device` | | `{connected, model, serial, brightness, stub}` |
| `PUT /api/device` | `{brightness: 0..100}` | Updated device object. |

Scene validation follows `docs/research/manifest-profiles-admin.md` §2.3:

- The scene is an object, and `Name` is a string.
- `Actions` is an object keyed `"col,row"`. On `AKP153E` scenes `col` is 0..5 (column 5 is the side panel) and
  `row` is 0..2. Other models allow 0..15.
- Each action has a non-empty `UUID`. `Name`, `ActionID` and `Controller` are strings, `Settings` is an object,
  `States` is an array of objects, and `State` indexes `States`.
- Nested multi-action `Actions` are checked the same way.
- Unknown fields are allowed and kept.

Files are written the way Qt writes them: sorted keys, 4-space indent, no BOM. A BOM is accepted when reading.

## Store

`Store` is the seam for the daemon. `FsStore` covers plain folders, which tests point at a temp dir. The
daemon can wrap it or implement the trait itself to report the live device. Scene order and the current
scene go to `data/state.json`. Until that file has them, they are seeded read-only from Stream Dock's
`DataCache.db` (`SELECT Profile FROM "<model>"`, through a small built-in SQLite page reader in `src/sqlite.rs`)
and `StreamDockConfig.ini` (`[<model><serial>DeviceConfig] sdProfileName`, then `[<model>DeviceConfig]`).
Encrypted plugins load `data/manifests/<uuid>.json` when it exists, as in `docs/design/plugin-install-handoff.md`.

Installing is the only write into the plugins folder. `GET /api/plugins` only reads. The SPA has a
"Buscar en Descargas" button that calls `GET /api/plugins/inbox` once; there is no watcher thread and no poll.
The chosen package is inspected (read-only, zip-bomb limits in `plugman`) and shown before the user confirms;
`POST /api/plugins/install` does the copy. `state.json` keeps an `installed` array: only those UUIDs get
`removable: true`, so the DELETE endpoint can never remove a built-in or a plugin Stream Dock installed
itself. `state.json` is ours; losing it only makes old installs non-removable. `orcadeck edit` resolves
`plugins` to Stream Dock's own plugins folder, because that is where `orcadeck run` discovers plugins:
installs and removals are visible to both apps.

## Tests

`cargo test`: unit tests (HTTP parsing, SQLite reader against a synthetic fixture with interior b-tree pages, id
format) and `tests/api.rs` (real loopback socket, temp-dir fixture: security gate, scene order and current
scene, round trip with unknown fields, backup, validation, cell set/move/swap/copy/delete, plugin status,
inbox scan, inspect/install/remove with traversal and origin checks, images, device). `tests/fixtures/make_datacache.py`
regenerates the SQLite fixture. It holds no real data.

## Not done yet

- Device: `FsStore::device` is a stub (`connected: false`). Brightness is only persisted. The daemon provides
  the live device.
- Property inspectors in an iframe.
- Pages: every observed scene lists only itself in `Pages`, so a scene is edited as a single page.
- The editor does not delete scenes.
