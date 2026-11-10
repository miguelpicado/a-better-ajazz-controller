# Native plugin installer

`install.mjs` installs a `.streamDeckPlugin` (zip) or `.sdPlugin` folder without the Elgato app.

Handoff for the app integration (data model, Rust port, acceptance criteria):
`../../docs/design/plugin-install-handoff.md`.

It does **not** decrypt Marketplace manifests. When `manifest.json` starts with `ELGATO`, it rebuilds
the host manifest from the plugin's own plaintext files and writes an overlay to
`<data>/manifests/<uuid>.json`. The plugin folder is left untouched, so Elgato updates and the
downloaded package stay intact.

## Usage

```powershell
# Show what the reconstructor can recover from an installed plugin
node tools/plugin-install/install.mjs inspect "$env:APPDATA\HotSpot\StreamDock\plugins\com.elgato.camerahub.sdPlugin"

# Verify a browser download before installing it
node tools/plugin-install/install.mjs inspect "$env:USERPROFILE\Downloads\plugin.streamDeckPlugin"

# Install a package into the host plugin folder
node tools/plugin-install/install.mjs install .\com.example.plugin.streamDeckPlugin `
  --plugins "$env:APPDATA\OrcaDeck\plugins" --data "$env:APPDATA\OrcaDeck"

# Install every .streamDeckPlugin already in a folder, then keep watching it
node tools/plugin-install/install.mjs watch "$env:USERPROFILE\Downloads" `
  --plugins "$env:APPDATA\OrcaDeck\plugins" --data "$env:APPDATA\OrcaDeck"

# Use a plain manifest published by the vendor when reconstruction is not enough
node tools/plugin-install/install.mjs install .\plugin.streamDeckPlugin `
  --plugins .\plugins --data .\data --manifest .\vendor-manifest.json
```

`--force` replaces an existing plugin folder and keeps the old one as `.bak-<timestamp>`.
`watch --once` scans the folder and exits.

## Getting packages without keeping the Elgato app open

The Marketplace download endpoint requires a signed-in account, so there is no public URL to
scrape. The practical workflow is:

1. Sign in to `marketplace.elgato.com` in the browser (the session persists).
2. Open the product and use **Download** in its actions menu, not "Open in Stream Deck".
   The browser saves a `.streamDeckPlugin` file.
3. Keep `install.mjs watch` running on the Downloads folder. It installs the package and builds
   the manifest overlay automatically.

That uses the user's own entitlement and does not need the Elgato app running. The public
`mp-gateway.elgato.com/products` API is useful for search and metadata, but its
`purchase-link` endpoint returns 401 without the account session; the tool does not bypass that.

Vendors that publish a plain build (GitHub release, own website, Stream Dock store) can be
installed directly, with no Marketplace account at all.

## What it recovers

| Field | Source |
|---|---|
| `CodePath` | `bin/plugin.js`, `index.html`, `source/main.html`, root `*.exe` |
| Action UUIDs | keys in `en.json`; `HKCU\Software\HotSpot\StreamDock\<uuid>.sdPlugin` value names; literals in JS/HTML/`.exe`/`.dll` as a last resort |
| `Name`, `Tooltip`, `States` | `en.json`; property-inspector filename as a fallback |
| `Icon` | `imgs/`, `images/`, `resources/actions/` (best effort) |
| `PropertyInspectorPath` | `pi/`, `source/actions/`, `resources/actions/` (best effort) |
| `Controllers` | `Keypad`; adds `Knob` when the action name says dial/knob/encoder |

If confidence is not `full`, the plugin is copied but no overlay is written. Re-run with
`--manifest <json>` and a plain manifest from the vendor.

Checked against the encrypted plugins on this machine (2026-11-09): camerahub (18 actions),
obsstudio (19), volume-controller (11), advancedlauncher (4) and windowscockpit (1) all
reconstruct fully.

## New machine without Stream Dock

The registry key is only a list of action UUIDs:

```
HKCU\Software\HotSpot\StreamDock\<uuid>.sdPlugin
    <actionUuid>    REG_SZ    true|false
```

It contains no names, icons or code paths, and only exists after Stream Dock has seen the plugin.
It is a useful fallback, not the portable source. The installer does not write that key: it is
Stream Dock's UI state, and the overlay JSON is the durable copy. To move reconstructed manifests
to a new machine:

```powershell
# On the old machine, after installing the plugins
node tools/plugin-install/install.mjs export --data "$env:APPDATA\OrcaDeck" --out .\overlays

# On the new machine, before or after installing the packages
node tools/plugin-install/install.mjs import .\overlays --data "$env:APPDATA\OrcaDeck"
```

Most plugins reconstruct from the package alone (`en.json` or code literals). Advanced Launcher is
one of them: its action UUIDs are in the `.exe`. Windowscockpit is the current exception: its only
action UUID (`com.vivremotion.windowscockpit.plugin`) appears only in the registry/encrypted
manifest, so export its overlay on the old machine or use a vendor manifest.

## Test

```powershell
node --test tools/plugin-install/install.test.mjs
```
