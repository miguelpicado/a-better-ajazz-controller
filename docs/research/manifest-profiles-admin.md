# Encrypted Elgato manifests, Stream Dock profile format, and why Stream Dock runs elevated

Researched 2026-11-09 on this machine (Windows 11 Pro 10.0.26300), static inspection only: files, PE tables, strings,
registry reads, SQLite reads from copies in the scratchpad, and a Ghidra decompile of four functions of the Stream Dock
exe. Nothing on the installs, AppData, the scheduled task or
the device was changed, and no process was stopped.

Versions: **Stream Dock AJAZZ Global 3.10.191.0421** (`C:\Program Files (x86)\Stream Dock AJAZZ Global`, Qt 5.15.2),
**Elgato Stream Deck** (`C:\Program Files\Elgato\StreamDeck\StreamDeck.exe`, Qt 6, 28 MB), device AKP153E
serial 355499441494.

Tags:
- **OBSERVED**: seen directly in a file, a PE table, a registry value or a log line. The evidence is cited.
- **STRINGS**: the literal is in a binary's string table (file offset given). It shows a name exists and roughly where
  it is used, not its exact behaviour.
- **DECOMPILED**: read from Ghidra pseudocode of the binary (function address given).
- **INFERRED**: a conclusion drawn from the above, not verified at runtime.

Companion doc: `docs/research/plugin-host-protocol.md` (WebSocket protocol, launch command line, plugin manifest keys).

---

## 0. TL;DR

1. **Encrypted manifests are Elgato Marketplace DRM.** `manifest.json` starts with `ELGATO 01 00`, a u64 file size, a
   u64 block length, then ciphertext. `StreamDeck.exe` decrypts it in memory (`esd::crypto::scheme`,
   `x25519-hkdf-sha256-aes-256-gcm`, an Ed25519 signature, per-file checksums). No key ships in the plugin, and no
   plain copy is ever written to disk. A replacement host cannot decrypt these legitimately. It has to use a
   **reconstructed plain manifest (option c)**, as already done for Insta360, or the vendor's own plain build.
   The plugins themselves tolerate a missing manifest: Elgato's JS SDK treats an unparsable one as "DRM protected".
   Stream Dock cannot read these manifests either. Its decompiled loader parses `manifest.json` as plain JSON and
   skips the plugin when the result is empty (§1.7).
2. **Profiles are plain JSON** and readable as they are: `profiles\<ID>.sdProfile\manifest.json`, with `Actions` keyed
   `"col,row"`. Images are in `Images\`. The device to scene list mapping is in `config\DataCache.db`, table `<Model>`.
   The current scene per device is in `config\StreamDockConfig.ini`, section `[<Model><Serial>DeviceConfig]`.
3. **Admin is not needed by the core app.** It is forced by an AppCompat layer `~ RUNASADMIN` on `Stream Dock AJAZZ.exe`
   in HKLM. The `StreamDock` task ("StreamDock sin UAC", created by the user in 2024) exists only to skip the UAC
   prompt that layer causes. HID, hooks, SendInput, volume, clipboard, screen capture and foreground-app detection all
   work unelevated. Three things genuinely want admin: CPU/GPU sensors (the SystemMonitor plugin loads the WinRing0
   driver), injecting input into **elevated** windows (UIPI), and writing HKLM Run or Program Files.

---

## 1. Q1: Elgato manifest encryption

### 1.1 Which plugins are encrypted (OBSERVED)

Scan of every `manifest.json` in `%APPDATA%\Elgato\StreamDeck\Plugins` and `%APPDATA%\HotSpot\StreamDock\plugins`:

| Plugin | Size | Format | Files in plugin dir | Bytes per file |
|---|---|---|---|---|
| com.barraider.advancedlauncher | 8 112 | ELGATO v1 | 51 | 159 |
| com.elgato.camerahub | 44 288 | ELGATO v1 | 315 | 140 |
| com.elgato.obsstudio | 56 016 | ELGATO v1 | 339 | 165 |
| com.elgato.volume-controller | 25 760 | ELGATO v1 | 129 | 200 |
| com.insta360.webcam (Elgato copy) | 218 864 | ELGATO v1 | 1 497 | 146 |
| com.vivremotion.windowscockpit | 9 632 | ELGATO v1 | 71 | 136 |
| keycreator, tutorial, zoom, neewer, obsbot, web-requests, bluetooth.monitor, commandsender | 0.7 to 4.7 KB | plain JSON | | |

- The encrypted bytes are byte-identical between the Elgato and Stream Dock copies (same SHA-256 prefix), because
  `Stream Deck.bat` xcopies one tree onto the other (§3.4).
- Insta360 in Stream Dock: `manifest.json` (1 544 B) is now identical to the repo's
  `tools/streamdock/insta360-manifest.json`. The original is kept as `manifest.json.elgato` (218 864 B,
  SHA-256 `37b7ed2055952665…`, same as the Elgato copy).
- The plugins Elgato ships itself (`Program Files\Elgato\StreamDeck\Distributables\*.streamDeckPlugin`: keycreator,
  tutorial) have **plain** manifests inside the zip. Only Marketplace-distributed plugins are encrypted.

### 1.2 File format (OBSERVED layout, INFERRED field meaning)

```
off  size  value
0    6     "ELGATO"                       magic
6    2     u16 LE = 1                     format version (all 6 files)
8    8     u64 LE = total file size       matches the file length exactly in all 6 files
16   8     u64 LE = N                     160, 448, 320, 640, 256, 192: always a multiple of 32
24   N     high-entropy block             INFERRED: key/header envelope (ephemeral X25519 key, nonce, signature, ...)
24+N rest  high-entropy body              length is always ≡ 8 (mod 16); whole-file entropy 7.98 to 8.00 bits/byte
```

- Size grows with the number of files in the plugin, at about 136 to 200 bytes per file (table above). A manifest alone
  would be 1 to 25 KB. **INFERRED:** the encrypted payload holds the manifest **plus a per-file checksum list** for the
  whole plugin folder. That matches the strings `Files`, `hash mismatch`, `checksum error: %s - %s` and
  `esd::plugins::verify_checksum` (§1.3).

### 1.3 Who decrypts it: the Stream Deck app at load time (STRINGS in `StreamDeck.exe`)

| Offset | Literal | Meaning |
|---|---|---|
| 0x19d7f70… | RTTI `esd::crypto::scheme::{Decryptor, Decryptor1, Signature, Signature_Ed25519, ArchiveReader, FileStream, DataStream, MemoryStream}`, `esd::crypto::Blake3_Hasher` | in-app crypto module, scheme version "1" |
| 0x16ae1d8 | `x25519-hkdf-sha256-aes-256-gcm` | the scheme: X25519 key agreement, HKDF-SHA256, AES-256-GCM |
| 0x16ae300 | `esd::crypto::scheme::make_reader`, `Setup error: %s`, `Reader` | |
| 0x16ae528… | `Failed to derive shared secret`, `Invalid public or private key size`, `EVP_DecryptFinal_ex failed`, `Invalid tag length`, `Error fetching HKDF KDF`, `salt`, `Invalid signature size`, `Failed to initialize signature verification operation` | OpenSSL 3 EVP implementation (statically linked; `Licenses\openssl.txt`, `Sodium.txt`, `blake3.txt`) |
| 0x16ae208, 0x16ae240 | `cipher`, `signing` next to `BJData`, `CBOR`, `MessagePack`, `BSON`, `UBJSON` | INFERRED: the decrypted envelope is a binary-JSON document (nlohmann::json) |
| 0x16aea68… | `esd::crypto::memory_vault::store/restore`, `memory encryption error` | decrypted data is kept encrypted in RAM |
| 0x1682250 | `Failed to setup private manifest reader`, `Failed to deserialize manifest` | the manifest goes through the crypto reader |
| 0x16822c8, 0x16824f8 | `Marketplace plugin is missing identity`, `Marketplace plugin is missing codesign`, `Marketplace plugin has invalid SDK version` | Marketplace-only checks |
| 0x16824c8 | `Failed to peek plugin manifest: {}`, `Files` | |
| 0x1681bf8 | `hash mismatch`, `checksum error: %s - %s`, `esd::plugins::verify_checksum` | per-file integrity check |
| 0x16823f0 | `Failed to load secrets at {}: {}`, `esd::dom::plugin::pvt::import_secrets`, `secrets.json` | Marketplace plugins can also carry encrypted secrets |

The answer is therefore **the Stream Deck app**, at plugin load, not the installer and not the plugin. Supporting
evidence:
- **OBSERVED:** the plugin folders contain only code, assets and locale JSON. There is no key, certificate or
  signature file.
- **OBSERVED:** no plain copy of an encrypted manifest exists anywhere checked: the plugin dirs,
  `%APPDATA%\Elgato\StreamDeck\Cache.db` (only an `actions` telemetry table), `Marketplace\cache.json`
  (`product_mapping[{ProductID, Type, UUID, VariantID}]` plus `user_items{signature, timestamp, variants}`),
  `Data\application_cache.json` and `%LOCALAPPDATA%\Elgato`.
- **OBSERVED:** `Marketplace\user.json` holds the signed-in account (`signature`, `user{id, email, …}`) and
  `cache.json` a signed entitlement list. **INFERRED:** these tie decryption to an entitled Elgato install.

**Key source.** X25519 is asymmetric. The file is encrypted to a public key, and the matching private key lives with the
Stream Deck app, either embedded in it or obtained through the signed-in account. It is **not** observable from the
plugin files. I deliberately did not look for or extract it: doing so would mean circumventing a technical protection
measure (DRM), which is outside the brief.

### 1.4 The plugins are designed to run without a readable manifest (OBSERVED)

Insta360 `bin/plugin.js` bundles Elgato's `@elgato/streamdeck` SDK:

```js
const manifest$1 = new Lazy(() => { ... JSON.parse(readFileSync(join(process.cwd(), "manifest.json"))) ...
    catch (e) { if (e instanceof SyntaxError) { return null; } ... } });
/** @returns The manifest; otherwise `null` when the plugin is DRM protected. */
function getManifest() { ... }
```

So a plugin process started with a plain, reconstructed manifest, or with none at all, behaves the same at runtime. The
manifest matters only to the **host**: CodePath, Nodejs version, the action list, Controllers, States and icons.

### 1.5 What Stream Dock does with them (OBSERVED, mechanism unresolved)

Stream Dock logs `Plugin … com.elgato.camerahub.sdPlugin is now connected` and the same for advancedlauncher,
windowscockpit, volume-controller and obsstudio, with their manifests encrypted on disk. Almost every full session today up
to 15:31 shows this (not 09:25, and not two sessions with 0 plugins). From 15:32 on, those five no longer connect (session sizes drop from 21–22 plugins to 13–17).
Insta360 never connected until its manifest was replaced (first connect 15:59, birth time of that copy 15:32).

- **STRINGS:** `Stream Dock AJAZZ.exe` has no `ELGATO`, `x25519`, `hkdf` or `aes-256-gcm` literal. Its only OpenSSL
  import is `LIBEAY32.dll` (1.0.x, by ordinal, 14 functions); OpenSSL 1.0 has no X25519. SDLibrary1 and QCefView do not
  import OpenSSL. **INFERRED:** Stream Dock does not decrypt Elgato manifests.
- **OBSERVED:** `HKCU\Software\HotSpot\StreamDock\<pluginId>` holds the plugin's action UUIDs as value names, for
  example 16 for camerahub and 19 for obsstudio. Those plugins were first installed in 2024 or early 2025, so likely
  while Elgato still shipped them plain. On today's xcopy the manifests' ctime is 2026-11-09 14:12 UTC (15:12 local).
- **DECOMPILED (§1.7):** Stream Dock's loader parses `manifest.json` with a plain `QJsonDocument::fromJson`. An
  `ELGATO` file gives an empty object, and the plugin is skipped. So the "connected" lines for encrypted plugins
  between 15:12 and 15:31 are **not** Stream Dock reading Elgato manifests. INFERRED, unverified: plugin processes
  that survived a Stream Dock restart and reconnected (BarRaider/Elgato SDKs auto-reconnect), until they finally died
  around 15:32.

### 1.6 Options for a replacement host

| Option | Verdict |
|---|---|
| (a) Decrypt it | **No.** It needs Elgato's private key or a running Elgato app as an oracle. That is DRM circumvention, not interoperability from files the user has. |
| (b) Consume a plain manifest produced at install time | **Not available.** Elgato never writes one (§1.3). Possible only when the vendor publishes a plain build: a GitHub release, or the Stream Dock "Space" store build of the same plugin. |
| (c) Conversion tool (reconstruct) | **Yes, recommended.** This is what `tools/streamdock/insta360-manifest.json` already is. Automate it as below. |

Reconstruction recipe. Every field below comes from the plugin's own unencrypted files:
- `UUID`: the folder name without `.sdPlugin`.
- `Name`, `Description`, and each action's `Name` and `Tooltip`: from `en.json`, keyed by action UUID (OBSERVED for
  insta360, camerahub with 18 actions, obsstudio 19, volume-controller 11).
- Action UUIDs: from the `en.json` keys. As a cross-check, use the `UUID: "…"` literals in `bin/plugin.js`. Insta360 has
  4: `setter`, `adjust`, `reset`, and `com.elgato.test.action` (SDK sample, drop it). As a fallback, use
  `HKCU\Software\HotSpot\StreamDock\<pluginId>` value names.
- `CodePath`:
  - `bin/plugin.js` means a Node plugin. Set `"Nodejs": {"Version": "20"}` (Stream Dock ships `node\node20.exe`).
  - `index.html` at the root means an HTML plugin (camerahub, windowscockpit).
  - `<uuid>.exe` or another root `*.exe` means a native plugin (advancedlauncher, windowscockpit).
- `PropertyInspectorPath`: `ui/*.html` or `pi/index.html`. Icons come from the `imgs/`/`images/` tree, without
  extension, as the SDK expects.
- `Controllers`: `["Keypad"]` by default. Add `"Knob"` when the plugin's code handles `dialRotate` (insta360 `adjust`).
- Plugins with no locale files and compiled code (advancedlauncher, windowscockpit): take the action UUIDs from the
  vendor's public docs/source or from existing profiles (`UUID` fields, §2), and treat them as best effort.

Ship the result as an overlay (`<host-data>/manifests/<uuid>.json`) and leave the plugin folder untouched. This keeps
the original file, keeps Elgato updates working, and keeps the copy the user has bought byte-identical.

### 1.7 Stream Dock's own plugin loader (DECOMPILED, Ghidra 12.1.4 via `rea trace`/`rea decompile`)

Target `Stream Dock AJAZZ.exe` (sha256 `f16438d9…`). The literal `CodePathWin` (VA 0x1425b52f8) is referenced from
three functions: `FUN_1400376a0` (manifest key parser), `FUN_140161a80` = `SDPluginManager::LoadPrivatePlugin`, and
`FUN_140169280` = `SDPluginManager::loadPlugin` (both names come from their `QMessageLogger` strings).

1. **Manifest reader `FUN_14028a340(dir)`:**
   - If `<dir>\manifest.loadingPackage` exists, it calls `FUN_140276300`. That function reads the file and requires
     more than 0x4e8 bytes. It drops 0x274 bytes from the head and from the tail, then decrypts the rest with
     `QtAESHandler::decryptECB` and a **hard-coded 16-byte key literal**, and parses the result as JSON. This is
     MiraBox's own store-package obfuscation, **unrelated to Elgato's scheme**. The key is a constant inside E; it is
     not reproduced here.
   - Otherwise it reads `<dir>\manifest.json` with a plain `QJsonDocument::fromJson`.
   - It then round-trips the object through JSON. Both callers stop when the object `isEmpty()`.
   - So an Elgato-encrypted manifest leads to a parse failure, an empty object, and **the plugin is not loaded**.
     There is no fallback. No `manifest.loadingPackage` exists on this machine.
2. **CodePath resolution** (both loaders): it reads `CodePathWin`, falling back to `CodePath`, as `<dir>\<CodePath>`.
   If that path does not exist, it tries appending `.html`, `.exe`, `.js`, `.mjs` and `.cjs` in that order. Otherwise
   it logs `Can't find the code path file` (critical; never seen in the 1 037 logs). The SDK version decides between
   `connectElgatoStreamDeckSocket` and `connectMiraBoxSDSocket`. `Nodejs.Version` selects node, and
   `RunAsAdministrator` is a per-plugin manifest key. A replacement host should mirror this extension probing,
   because Stream Dock-targeted manifests rely on it (for example `"Icon": "imgs/action"` style paths without extension).
3. `loadPlugin` also checks some plugins (`FUN_140291f70`) against a store list (`plugins[].download`, matched by file
   base name) and refuses them if they are not listed. INFERRED: this is a store-entitlement check for MiraBox paid
   plugins.

**Implication:** a manifest overlay is also what made Insta360 work in Stream Dock. That is the only path for Elgato DRM
plugins in either host.

---

## 2. Q2: Stream Dock profile and scene format

### 2.1 Files and what they do

| Path (under `%APPDATA%\HotSpot\StreamDock`) | Content |
|---|---|
| `profiles\<ID>.sdProfile\manifest.json` | one scene (profile page): metadata + `Actions` |
| `profiles\<ID>.sdProfile\Images\` | custom key images; files named with 27 random `[0-9A-Z]` + `.png` (also user-copied names such as `ARROW_LEFT.png`) |
| `profiles\<ID>.sdProfile\profiles\` | empty in all observed scenes. INFERRED: child "folder" scenes (`profile.openchild`) |
| `profiles\<ID>.sdProfile\<col,row>\CustomImages\state0.gif` + nested `<GUID>.sdProfile\` | legacy layout, seen only in two imported " Copia de" scenes (from an Elgato `.streamDeckProfile`) |
| `config\StreamDockConfig.ini` | QSettings INI: current device, current scene per device, brightness, screen-off, language, UI state |
| `config\DataCache.db` | SQLite: device list and **per-device ordered scene list** |
| `HKCU\Software\HotSpot\StreamDock\<pluginId>` | registry: one `REG_SZ "true"/"false"` per action UUID (INFERRED: action-list visibility) |
| `plugins\<uuid>.sdPlugin\` | third-party plugins. Built-ins are in `<install>\plugins\` (27, `com.hotspot.streamdock.*`) |

The `*.bak` files next to some manifests were written by `tools/streamdock/profiles.mjs`. The app ignores them.

### 2.2 Scene ID (OBSERVED)

`XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX` in uppercase **base36** (`R0KZ54Z0-063U-947X-31BI-GG95R8F4HT88`), not hex. The
folder is `<ID>.sdProfile`. References use the bare ID (`ProfileUUID`) or `<ID>.sdProfile` (`Pages`, ini,
DataCache). STRINGS in E: `%1-%2-%3-%4-%5.sdProfile`.

### 2.3 `manifest.json` schema (OBSERVED across all 11 manifests)

```jsonc
{
  "Name": "Orca",                       // scene name shown in the app
  "Version": "1.0",
  "DeviceUUID": "AKP153E",              // device model key (= ini DeviceUUID = DataCache table name)
  "DeviceModel": "20GBA9901",           // model code; 20GBA9901 with AKP153E, 20GAA9901 on the AKP815/imported scenes
  "DeviceSerialNumber": "355499441494", // present on newer scenes; binds the scene to one physical unit
  "AppIdentifier":  "*",                // foreground-app auto-switch: "*" = default/any, "" = none,
  "AppIdentifier0": "*",                //   or an absolute exe path with forward slashes,
  "AppIdentifier1": "",                 //   e.g. "C:/Users/migue/AppData/Roaming/Zoom/bin/Zoom.exe" (Grabación)
  "AppIdentifier2": "",                 // INFERRED: up to 3 apps (0..2); AppIdentifier mirrors AppIdentifier0
  "Pages": {                            // multi-page container; every observed scene lists only itself
    "Current": "R0KZ54Z0-063U-947X-31BI-GG95R8F4HT88.sdProfile",
    "Pages":  ["R0KZ54Z0-063U-947X-31BI-GG95R8F4HT88.sdProfile"]
  },
  "Actions": {
    "2,0": {                            // key "col,row", 0-based
      "ActionID": "p2hytdeh-bl1e-k0qx-de5v-jl2434ogywsv", // per-instance id (lowercase base36 8-4-4-4-12; hex UUIDs from profiles.mjs are also present and load)
      "UUID": "com.hotspot.streamdock.profile.rotate",     // action UUID from the plugin manifest
      "Name": "Scene Shift",
      "Controller": "",                 // "" = keypad in every observed action; absent in legacy nested scenes
      "Settings": { "DeviceUUID": "", "ProfileUUID": "H33G4VTZ-25Z8-PYN7-R34M-ST39SX8TRT2Q" },
      "State": 0,                       // index into States
      "States": [ { "Image": "ARROW_RIGHT.png" } ],
      "Actions": [ /* only for com.hotspot.streamdock.multiactions.routine: nested action objects, same shape */ ]
    }
  }
}
```

Older scenes (AKP815, imported copies) have no `DeviceSerialNumber` and no `Pages`. The two " Copia de" scenes are only
`{"AppIdentifier":"", "Name":"…"}` plus a nested `<GUID>.sdProfile\manifest.json` (`DeviceModel`, `Name`, `Version`,
`Actions`) in the legacy layout.

**`States[]` fields seen** (all optional): `Image`, `Title`, `ShowTitle` (bool), `TitleAlignment`
(`top|middle|bottom|center`), `TitleColor` (`#rrggbb`), `FontSize` (**number or string**, for example `10` and `"10"`),
`FontFamily`, `FontStyle` (`Bold`), plus legacy `FFamily`, `FSize`, `FStyle`, `FUnderline` (`off`) and `TitleShow`. A
reader must accept both spellings and both types.

**Key positions on AKP153E** (OBSERVED in R0KZ/H33/3I0U): columns 0–4 × rows 0–2 are the 15 LCD keys. Column 5
(`5,0`, `5,2` in R0KZ) holds the **side-panel tiles**, which the Orca Deck plugin targets as the `Information`
controller (commit d0daf2d; `profiles.mjs` comment "side panel"). INFERRED: `5,0..5,2` are the three side tiles, top
to bottom.

**Image resolution** (OBSERVED values, INFERRED rules):
1. A bare file name (`ARROW_RIGHT.png`, `9694Q4PE….png`) is `<scene>\Images\<name>`.
2. A relative path **without extension** (`imgs/action`, `images/actions/CpuUsageImage`, `Images/btn_switchProfile`)
   is relative to the **owning plugin's folder**: `%APPDATA%\…\plugins\` for third-party, `<install>\plugins\` for
   built-ins. Try `.png`, `.gif`, `.jpg`, `.svg`, and `@2x`. OBSERVED: `Images/btn_custom_open_website` exists as
   `.gif` and `.jpg` in `<install>\plugins\com.hotspot.streamdock.system.website.sdPlugin\Images\`.
3. A relative path **with extension** (`static/img/index.jpg`, `imgs/actions/setter/aiZoom/halfBody_off_connect.svg`)
   is plugin-relative, used as written. These are often the last `setImage` a plugin persisted.
4. `""` means use the plugin manifest's default state image.
5. Legacy nested scenes: `state0.gif` is `<col,row>\CustomImages\state0.gif`.

### 2.4 Scene-switch and navigation actions

| UUID | Settings | Meaning |
|---|---|---|
| `com.hotspot.streamdock.profile.rotate` ("Scene Shift") | `{DeviceUUID: "", ProfileUUID: "<ID>"}` | go to that scene. `ProfileUUID: ""` = next scene in DataCache order (title "Next Scene", INFERRED) |
| `com.elgato.streamdeck.profile.rotate` ("Switch Profile") | `{DeviceUUID: "", ProfileUUID: "<Elgato GUID>"}` | imported from Elgato; the GUIDs do not exist in Stream Dock (dangling) |
| `com.hotspot.streamdock.profile.openchild` / `.backtoparent`, `com.hotspot.streamdock.page` | not seen in the user's scenes | built-ins exist in `<install>\plugins` |
| `com.hotspot.streamdock.multiactions.routine` | nested `Actions[]` | multi action |

Other Settings are plugin-private and passed to the plugin as `payload.settings` unchanged. Examples:
`system.hotkey` `{Coalesce, Hotkeys[{KeyCmd, KeyCtrl, KeyShift, KeyOption, KeyModifiers, NativeCode, QTKeyCode, VKeyCode, RKey*}]}`,
`system.website` `{openInBrowser, path}`, `system.multimedia` `{actionIdx}`,
`insta360.webcam.setter` `{option, secondOption, product}`.

### 2.5 Device to scene mapping (OBSERVED)

`config\DataCache.db` (SQLite, read from a copy):

| Table | Columns | Rows |
|---|---|---|
| `AJAZZ` | `Device TEXT, serialNumber TEXT` | `("AJAZZ AKP153E", NULL)`, `("AJAZZ AKP153E_1494", "355499441494")` |
| `AKP153E` (one table per model) | `Profile TEXT` | **ordered scene list**: `3I0U64IN….sdProfile` (Monitorización), `R0KZ54Z0….sdProfile` (Orca), `H33G4VTZ….sdProfile` (Grabación) |
| `UnknownDevice` | `Profile TEXT` | empty |
| `OBSServerInfo` | `ServerHost, ServerPort, ServerPassword, ConnectionAvailable` | 154 rows, **plaintext passwords**: treat as a secret, do not copy |

`config\StreamDockConfig.ini`:

```ini
[StreamDockCurrentDevice]
Device=AKP153E
AKP153E\SerialNumber=355499441494
[AKP153E355499441494DeviceConfig]        ; <Model><Serial>: the scene currently shown on that unit
DeviceUUID=AKP153E
sdProfileName=R0KZ54Z0-063U-947X-31BI-GG95R8F4HT88.sdProfile
sdProfilePath=C:\\Users\\migue\\AppData\\Roaming\\HotSpot\\StreamDock\\profiles\\R0KZ54Z0-….sdProfile
[AKP153EDeviceConfig]                     ; serial-less legacy entry; points to UGNF54G2-… which no longer exists (stale)
[DeviceBrightness]  AKP153E=100, AKP153E355499441494=100
[ScreenOffTime]     time=-10087, AKP153E355499441494=-10087   ; INFERRED: negative = disabled
[StreamDockLanguage] language=3
[AKP153E]           481 keys "<lang>\ActionState\<category>\<pluginId>=0"   ; action-list tree UI state
```

Six of the nine scene folders are not listed in any device table: the AKP815 scenes and the imported copies. They stay
on disk and the AKP153E never shows them.

**Reader algorithm for the replacement app:**
1. Find the device over HID (VID 0x0300 = 768, PID 0x1010 = 4112 per the log `tmpVid: 768 tmpPid: 4112`), get model and
   serial.
2. Scene order is `SELECT Profile FROM "<Model>"` (DataCache.db, opened read-only). Current scene is ini
   `[<Model><Serial>DeviceConfig] sdProfileName`, falling back to `[<Model>DeviceConfig]` and then to the first row.
3. Load each `<ID>.sdProfile\manifest.json` (strip a UTF-8 BOM if there is one). Render `Actions["c,r"]` with
   `States[State]`. Resolve images as in §2.3.
4. Scene Shift goes to `ProfileUUID`. Auto-switch: on a foreground change, pick the scene whose `AppIdentifier0..2`
   equals the foreground exe path (case-insensitive, `/` vs `\` normalised). Otherwise use the scene with `"*"`.
   Stream Dock only does this while minimised, according to its UI string.
5. Do not write these files while Stream Dock is running: it rewrites them (see the `profiles.mjs` header "Run with
   Stream Dock stopped"). For the replacement, read only and keep its own state elsewhere until cut-over.

### 2.6 `tools/streamdock/profiles.mjs` (OBSERVED)

The script edits the Orca (`R0KZ…`) and Grabación (`H33G…`) scenes in place. It backs up to
`manifest.json.pre-orcadeck.bak`, copies arrow PNGs into `<scene>\Images\`, and builds actions as
`{ActionID: randomUUID(), Controller: '', Name, Settings, State: 0, States: [{Image}], UUID}` with keys `"col,row"`.
Scene links use `com.hotspot.streamdock.profile.rotate` with `{DeviceUUID: '', ProfileUUID}`. This matches §2.3. The
only difference is hex `ActionID`s instead of base36, and those load. Note: the live R0KZ file now has the summary at
`5,2`, while the script writes `5,1`. The file was changed after the script ran.

---

## 3. Q3: why it runs elevated, and what needs it

### 3.1 How elevation is forced (OBSERVED)

| Evidence | Value |
|---|---|
| `Stream Dock AJAZZ.exe` embedded manifest | `requestedExecutionLevel level='asInvoker' uiAccess='false'` (also ScreenCaptureTool, FirmwareUpgradeTool, SplashScreen, CefViewWing, node20) |
| `HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion\AppCompatFlags\Layers` | `C:\Program Files (x86)\Stream Dock AJAZZ Global\Stream Dock AJAZZ.exe = ~ RUNASADMIN` |
| `HKCU\…\AppCompatFlags\Layers` | `…\com.hotspot.streamdock.system.monitor.sdPlugin\SystemMonitor.exe = ~ RUNASADMIN` (and that exe's own manifest is `requireAdministrator`) |
| Scheduled task `\StreamDock` | Author `TEAMBIM-DESKTOP\Miguel`, created 2024-05-02, Description **"StreamDock sin UAC"**, LogonTrigger, RunLevel Highest, `"…\Stream Dock AJAZZ.exe" /background` |
| `HKCU\…\Run` | `Stream Dock AJAZZ = …\Stream Dock AJAZZ.exe --RunInBackground` (also a copy under `Run\AutorunsDisabled`) |
| STRINGS E | config key `RunAsAdministrator`. SDLibrary1 exports `UtilitiesWindows::runApplicationAsAdministrator` (uses `runas`) and `UtilitiesWindows::administratorDragEnable` (reads `HKLM\…\Policies\System\EnableLUA`) |

INFERRED chain: the app's "run as administrator" option, or the installer, wrote the HKLM `RUNASADMIN` layer, so every
launch hits UAC. Windows does not start elevated programs from the Run key at logon, so the user added a
Highest-privilege logon task to get a prompt-free start. **The binary itself never asks for admin.**
`administratorDragEnable` exists because an elevated window cannot receive drag-and-drop from an unelevated Explorer
(UIPI). Elevation breaks dropping files onto keys.

Side effect (INFERRED, important for security): every plugin Stream Dock spawns inherits a High-integrity token. That
covers `node20.exe` running third-party JS, the plugin exes and the CEF HTML plugins. Any plugin bug becomes an admin
bug.

Elgato for comparison (OBSERVED): `StreamDeck.exe` is `asInvoker` and starts from HKCU Run, **unelevated**. Its
helper `hotkeys_handler.exe` is `asInvoker` with **`uiAccess="true"`** and imports `SendInput`. That is Microsoft's
supported way to inject input into elevated windows without admin.

### 3.2 Import evidence (OBSERVED, `rea inspect-pe-imports`)

- `Stream Dock AJAZZ.exe`: USER32 `SetWindowsHookExW, CallNextHookEx, SendInput, SetForegroundWindow, SwitchToThisWindow, EnumWindows, GetWindowThreadProcessId, RegisterDeviceNotificationW, PostThreadMessageW`;
  KERNEL32 `OpenProcess, CreateToolhelp32Snapshot, Process32First/NextW, SetPriorityClass`; ADVAPI32 `RegCreateKeyExW, RegDeleteTreeW`;
  SHELL32 `ShellExecuteW`; WTSAPI32 `WTSRegisterSessionNotification`; POWRPROF `PowerGetActiveScheme, PowerReadACValue`; WINMM `waveOut*`; LIBEAY32 (14 ordinals).
- `SDLibrary1.dll`: USER32 `SetWindowsHookExW, UnhookWindowsHookEx, SendInput, keybd_event, mouse_event, MapVirtualKeyW, ActivateKeyboardLayout, GetForegroundWindow, SetForegroundWindow, FindWindowW, GetWindowThreadProcessId`;
  KERNEL32 `OpenProcess, QueryFullProcessImageNameW, CreateToolhelp32Snapshot, DeviceIoControl, CreateFileW, LoadLibraryW, CreateFileMappingW`;
  ADVAPI32 only `CredReadW/CredWriteW/CredDeleteW/CredFree`; `d3d11.D3D11CreateDevice`; GDI32 `BitBlt`; ole32 `CoCreateInstance`. STRINGS `hid.dll` (hidapi loads it dynamically), `IMMDevice Activate`, `IDXGIOutput6`, `DuplicateOutput`.
- **Absent everywhere:** `AdjustTokenPrivileges`, `OpenProcessToken`, `CreateService`/`OpenSCManager`, `WriteProcessMemory`, `CreateRemoteThread`, `SetupDi*` (static). Nothing in the import tables needs a privilege that a standard user lacks.
- `FirmwareUpgradeTool.exe`: libusb-1.0, `DeviceIoControl`, `RegOpenKeyExA`. `ScreenCaptureTool.exe`: Qt and `SetWindowPos` only.

### 3.3 Feature by feature

| Feature (SDLibrary1 export / evidence) | Unelevated? | Notes for a non-admin replacement |
|---|---|---|
| **HID device I/O** (hidapi: `hid.dll` via LoadLibrary, `CreateFileW`, `DeviceIoControl`; log `openHidDevice`) | **Yes** | HID class devices are openable by the interactive user. Use `node-hid`/hidapi. No driver install. |
| **Firmware upgrade** (`FirmwareUpgradeTool.exe`, libusb) | Yes, if a WinUSB/libusb driver is already bound | Binding a driver (Zadig-style) is a one-time admin step. Not needed for normal use. |
| **Keyboard/mouse hooks** `Hook::enableKeyboardHook/enableMouseHook/getCurrentMouseCoordinate` (`SetWindowsHookExW`, LL hooks) | **Yes** | Used to record hotkeys and pick mouse coordinates. A low-IL hook gets **no events while an elevated window is foreground** (UIPI). Accept that, or use `RegisterHotKey` for global hotkeys. |
| **Simulated input** `SimulatedKeyboardEvent::sendKey*/sendString/sendUnicode`, `SimulatedMouseEvent::simulate*` (`SendInput`, `keybd_event`, `mouse_event`) | **Yes**, except into elevated windows | UIPI silently drops input aimed at a higher-integrity foreground window (Task Manager, admin terminals, installers, UAC). Options: (1) accept it; (2) a small signed **`uiAccess="true"` helper** installed under Program Files, as Elgato does with `hotkeys_handler.exe` (needs a code-signing cert and an install-time admin step, but runs unelevated); (3) an optional elevated helper launched on demand. Never elevate the whole host. |
| **Foreground-app detection** `MonitorApplicationsTool`, `MonitorDisplayFrontApplicationTool`, `UtilitiesWindows::getTheForegroundWindow/getProcessFullName` (`GetForegroundWindow`, `OpenProcess`, `QueryFullProcessImageNameW`, Toolhelp; log "MonitorApplicationsTool - start") | **Yes** | Use `SetWinEventHook(EVENT_SYSTEM_FOREGROUND, WINEVENT_OUTOFCONTEXT)` instead of polling. `OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION)` + `QueryFullProcessImageNameW` works on elevated processes from medium IL. Only protected processes fail. Feeds `AppIdentifier` switching and the SDK `applicationDidLaunch/Terminate` (`ApplicationsToMonitor`). |
| **Volume / mute / default device** `VolumeController::*`, `switchAudioDevice`, `AudioDeviceEventNotificationClient` (Core Audio `IMMDevice`, `IAudioEndpointVolume`) | **Yes** | Per-user COM. Switching the default device uses the undocumented `IPolicyConfig`, which also works unelevated. |
| **Clipboard** `WinClipboard::copy/paste/clearClipboard` | **Yes** | Paste is done by simulating Ctrl+V, so it has the same UIPI limit as SendInput. |
| **Screen capture** `ScreenCapturer`, `D3dManager`, `DxgiManager` (`D3D11CreateDevice`, `IDXGIOutput6`, `DuplicateOutput`, `BitBlt`) | **Yes** | DXGI Desktop Duplication works per user. It returns black or fails on the secure desktop (UAC, lock screen), with or without admin. Consider `Windows.Graphics.Capture`. |
| **Launch apps / open URL** (`ShellExecuteW/ExW`) | **Yes** | Use `runas` only for an action the user explicitly marks "as admin" (UAC prompt per launch). |
| **Kill process** `UtilitiesWindows::killProcessByName/PID` | Partly | Own-user processes yes. Elevated or other-user processes need admin. Report failure instead of elevating. |
| **Lock / sleep / power** `computerLockWorkStation`, `computerSleep`, POWRPROF | **Yes** | Standard users hold SeShutdownPrivilege (enabled per call). |
| **Session events** `WTSRegisterSessionNotification` | **Yes** | |
| **Credentials** `CredRead/Write` (`QKeychain`/`KeyChainClass`) | **Yes** | Per-user Credential Manager. |
| **Autostart** (STRINGS `HKLM\…\CurrentVersion\Run`, `HKCU\…\CurrentVersion\Run`, UI "auto-start setup failed, try running as administrator") | HKCU yes, HKLM no | Use HKCU Run (or a per-user logon task without Highest). That is the only reason the app's own UI asks for admin. |
| **Installed-software list** `getInstalledSoftware_x64/x86` (HKLM `…\Uninstall`, read) | **Yes** | HKLM is readable by users. |
| **CPU/GPU temperature, clocks, package power** (SystemMonitor plugin: `OpenHardwareMonitorLib.dll` with the embedded **WinRing0** driver, `HWiNFO64.dll`; exe is `requireAdministrator`; used by the Monitorización scene) | **No** | Loading a kernel driver needs admin, and WinRing0 is on Microsoft's vulnerable-driver blocklist. Non-admin options: CPU/RAM/disk/network via PDH or `GetSystemTimes`; GPU utilisation via PDH `\GPU Engine(*)\Utilization Percentage`; GPU temperature via NVML/ADL (vendor driver, no admin); CPU temperature only via an already-running service (LibreHardwareMonitor/HWiNFO shared memory). Otherwise drop it. |
| **`Stream Deck.bat`** (§3.4) | Elevates for no real need | `xcopy … /o` (copy owner and ACL) needs SeRestorePrivilege, and `>null` writes a file named `null` into the install dir (OBSERVED: `<install>\null` 101 110 B, `<install>\plugins\plugins elgato\null` 484 B). Without `/o` and with `>nul`, it needs no admin. |

### 3.4 `Stream Deck.bat` (OBSERVED, `<install>\Stream Deck.bat`)

It self-elevates: `NET FILE` tests for admin, and if that fails it writes a VBS that calls `ShellExecute … "runas"`.
Then it runs:

```bat
xcopy %userprofile%\AppData\Roaming\Elgato\StreamDeck\Plugins %userprofile%\AppData\Roaming\HotSpot\StreamDock\plugins /d/e/c/i/k/o/y >null
del %userprofile%\AppData\Roaming\HotSpot\StreamDock\storecache\storecache.json
```

This is how the encrypted Elgato manifests end up in Stream Dock (§1.1). For the replacement host: scan the Elgato
plugin folder **in place** (read-only), add the overlay manifests from §1.6, and do not copy anything.

---

## 4. Implications for the replacement app (summary)

1. Run **unelevated** (`asInvoker`, HKCU Run). Remove the HKLM `RUNASADMIN` layer and the `StreamDock` task only at
   cut-over, and only by the user's decision (not done here).
2. Plugin discovery: scan `%APPDATA%\Elgato\StreamDeck\Plugins` and `%APPDATA%\HotSpot\StreamDock\plugins`. For each
   `manifest.json`, check the first 6 bytes. If they are `ELGATO`, use `<host-data>/manifests/<uuid>.json` (the
   reconstructed overlay). If no overlay exists, list the plugin as "needs manifest" and do not launch it.
3. Scenes: read the existing Stream Dock profile tree and DataCache.db unchanged (§2.5). Write host state elsewhere.
4. Input to elevated windows: v1 accepts the UIPI limit. Add a signed `uiAccess` helper later if it matters.
5. Hardware sensors: drop CPU temperature/power, or read from an existing sensor service. Keep load and usage via PDH.

Skipped: decrypting the Elgato envelope or locating its key (out of scope by design); the Elgato ProfilesV3 format
(only shape noted: `{Device{Model,UUID}, Name, Pages{Current, Default, Pages[]}, Version "3.0"}` + `Profiles/<GUID>/manifest.json`
with `Controllers[{Type, Actions}]`). Add it when importing Elgato scenes is needed.
