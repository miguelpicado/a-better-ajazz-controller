# Stream Dock (AJAZZ / HotSpot / MiraBox) plugin host: protocol and replacement-host spec

Researched 2026-11-09 against **Stream Dock AJAZZ Global 3.10.191.0421** (Qt 5.15.2 + QCefView 1.1.0 / Chrome 109),
device **AKP153E** (fw V2.293S.00.003). Read-only investigation, nothing on the install, AppData or the device was changed.

Tags used below:
- **OBSERVED**: seen directly in a file, a log line, a live socket or process. The evidence is cited.
- **STRINGS**: the literal is in the binary's string table (`Stream Dock AJAZZ.exe`, file offset given). The order of
  strings in a Qt `.rdata` block follows the source, so neighbouring literals usually belong to the same function.
  That shows the name exists and roughly where it is used, not its exact semantics.
- **INFERRED**: a conclusion drawn from the above, not verified at runtime.
- **ELGATO-REF**: the public Elgato Stream Deck SDK protocol (docs.elgato.com/streamdeck/sdk), quoted from knowledge,
  used for comparison.

Main sources:
- `E` = `C:\Program Files (x86)\Stream Dock AJAZZ Global\Stream Dock AJAZZ.exe`. It holds `SDPluginManager`
  (`..\src\sdpluginmanager.cpp`) and `SDPluginServer` (`..\src\sdpluginserver.cpp`). **The plugin protocol lives here,
  not in SDLibrary1.dll.**
- `L` = `%APPDATA%\HotSpot\StreamDock\logs\log-*.txt` (1037 files, 2024-02 to 2026-11). Main sample:
  `log-2026-11-09-16-32-49.txt`.
- `SM` = `%APPDATA%\HotSpot\StreamDock\plugins\com.hotspot.streamdock.system.monitor.sdPlugin\log.txt`. This native
  plugin logs its own argv on every start, which makes it the best record of the launch command line.
- `ORCA` = `com.miguelpicado.orcadock.sdPlugin\plugin.js` and `plugin.log` (our plugin, working).
- `SDK-HTML` = `com.hotspot.streamdock.time.sdPlugin\static\plugin.js` and `static\action.js` (HotSpot's own
  "Plugin 2.5.0" HTML SDK, dated 2023-10).

---

## 0. TL;DR

1. Stream Dock is an **Elgato SDK v2 clone**. It uses the same argv (`-port -pluginUUID -registerEvent -info`), the
   same WebSocket JSON envelope (`{event, action, context, device, payload}`), the same register events and almost
   the same event names. Plain Elgato plugins (C#/BarRaider, C++, Node `@elgato/streamdeck`, HTML) run unmodified.
   Logs show 37 different plugins connecting, Elgato and BarRaider ones included.
2. There is **one WebSocket server on `127.0.0.1:18618`** (fixed port; it listens on both IPv4 and IPv6 loopback).
   Plugins and property inspectors both connect to it.
3. Code is launched in one of three ways, picked by file extension:
   - `.exe`: spawned as a process.
   - `.js` / `.mjs` / `.cjs`: run with the bundled `node\node20.exe`, enabled by the `Nodejs` manifest key.
   - `.html`: loaded into a hidden CEF view. The host then calls `connectElgatoStreamDeckSocket(...)` in the page
     (or `connectMiraBoxSDSocket`, or `connectSocket`).
4. Stream Dock extensions over Elgato:
   - Controllers `Information`, `SecondaryScreen` and `Knob`.
   - Extra commands: `sendToDevice`, `getScreenshot`, `getSystemAudioVolume`, `startAudioCapture` /
     `stopAudioCapture`, `getDetectedSensorsData`, `setText`.
   - Folder-profile hooks.
   - `application.version` is set to the plugin's own `Software.MinimumVersion`, so the plugin's version check
     always passes.
   - Some ELGATO-REF APIs are missing: `setFeedbackLayout`, `setTriggerDescription`, `didReceiveDeepLink`,
     resources and secrets, and `touchTap`'s `tapPos`/`hold`.
5. Persistence:
   - Action settings live inside the profile: `profiles\<id>.sdProfile\manifest.json` → `Actions["col,row"].Settings`.
   - Global settings go through QtKeychain, i.e. the Windows Credential Manager (STRINGS). The exact entry name is
     not confirmed.
6. Elgato marketplace plugins with **encrypted manifests** (file starts with `ELGATO\x01\x00`) still load in Stream
   Dock. A replacement host must solve this separately (§8).

---

## 1. Process model and launch

### 1.1 Discovery
- **OBSERVED**: user plugins are folders `%APPDATA%\HotSpot\StreamDock\plugins\<uuid>.sdPlugin\` (20 present).
- **OBSERVED**: built-ins are under `<install>\plugins\` (27, all `com.hotspot.streamdock.*`). They have no
  `CodePath`, carry `PrivateAPI` (27 manifests) and are implemented natively in `E`. The log shows them via
  `SDPluginManager::LoadPrivatePlugin` (STRINGS @0x25ca5f8). A replacement does not need them.
- **OBSERVED**: Stream Dock does **not** scan `%APPDATA%\Elgato\StreamDeck\Plugins`. The vendor's helper
  `<install>\Stream Deck.bat` (also under `<install>\plugins\plugins elgato\`) runs
  `xcopy %APPDATA%\Elgato\StreamDeck\Plugins %APPDATA%\HotSpot\StreamDock\plugins /d/e/c/i/k/o/y` and deletes
  `storecache\storecache.json`. In other words, Elgato plugins are copied over by hand.
- **OBSERVED**: removing a plugin leaves a `plugins-removed-YYYYMMDD\` folder behind.

### 1.2 Code path and runner selection
STRINGS @0x25ca5f8 (in `SDPluginManager::LoadPrivatePlugin` / `loadPlugin`), in order:
`Can't find the code path file`, `.exe`, `.js`, `.mjs`, `.cjs`, `registerPlugin`, `\u0027`, `APIVersion`,
`connectMiraBoxSDSocket('%1','%2','%3','%4')`, `connectElgatoStreamDeckSocket('%1','%2','%3','%4')`, `Nodejs`,
`Debug`, `node`, `-port`, `-pluginUUID`, `-registerEvent`, `-info`, `PATH`, `RunAsAdministrator`, `win`.

| CodePath ends in | Runner | Evidence |
|---|---|---|
| `.exe` | `QProcess` spawns it directly, cwd = plugin folder (INFERRED) | OBSERVED children of `Stream Dock AJAZZ.exe` (pid 56004): `SystemMonitor.exe`, `sdzoomplugin.exe`, `streamdeck-batplug.exe`, `com.zaphop.nvidiabroadcast.exe`, `streamdockSwitchAudio.exe` |
| `.js` `.mjs` `.cjs` | `<install>\node\node20.exe <CodePath> <args>`. Needs `"Nodejs": {"Version":"20"}`. `Nodejs.Debug` is read too (INFERRED: maps to `--inspect`, like Elgato). `PATH` is set or extended for the child (STRINGS) | OBSERVED two `node20.exe` children: orcadock and insta360. `ORCA` header comment: `node plugin.js -port P -pluginUUID U -registerEvent E -info J` |
| `.html` | QCefView, hidden. Several HTML plugins share one `CefViewWing.exe` renderer | OBSERVED: one `CefViewWing.exe` (pid 13416) holds 6 sockets to :18618 |

- `CodePathWin` / `CodePathMac` override `CodePath` per OS (STRINGS @0x25b42f8; `win` literal). Seen in practice:
  `com.krabs.windowsgizmos` uses only `CodePathWin=bin/streamdeck-batplug.exe` and runs fine (OBSERVED log
  "is now connected").
- `RunAsAdministrator` is honoured (STRINGS). Note that `Stream Dock AJAZZ.exe` itself runs elevated here, which
  is why `Win32_Process.CommandLine` comes back empty for it and its children (OBSERVED). Command lines were
  therefore taken from the plugins' own logs.

### 1.3 Command line (OBSERVED, `SM` 2026-11-09 16:32:56)
```
<plugin>\SystemMonitor.exe -port 18618 -pluginUUID 6GJ1V598VX5B65Q4GJT7UR1ENPWAFJM4 -registerEvent registerPlugin -info <json>
```
- `-pluginUUID` is a **new random 32-char `[A-Z0-9]` string on every launch**, not the manifest UUID (OBSERVED:
  it differs each time in `L`: "Plugin <32> com.x.sdPlugin is now connected").
- `-registerEvent` is always `registerPlugin`.
- `-info` (the SM logger strips the quotes, so the key names below are reconstructed). For Node plugins it is valid
  JSON: `@elgato/streamdeck` inside insta360 parses it with `JSON.parse` and works.
```json
{"application":{"font":"HarmonyOS Sans","language":"es","platform":"windows","platformVersion":"10.0.26300","version":"2.9.121"},
 "colors":{"buttonMouseOverBackgroundColor":"#464646FF","buttonPressedBackgroundColor":"#303030FF",
           "buttonPressedBorderColor":"#646464FF","buttonPressedTextColor":"#969696FF","highlightColor":"#0078FFFF"},
 "devicePixelRatio":1,
 "devices":[{"id":"58dbee7a577d76c19a4d16da2f85672e","name":"AKP153E","size":{"columns":6,"rows":3},"type":0}],
 "plugin":{"uuid":"com.hotspot.streamdock.system.monitor","version":"1.3.1"}}
```
- **Divergence (OBSERVED):** `application.version` is **the plugin's own `Software.MinimumVersion`**, not the Stream
  Dock version:
  - SystemMonitor: min 2.9.121 → `2.9.121`.
  - BarRaider `com.zaphop.nvidiabroadcast`: min 4.1 → "ApplicationInfo: Version: 4.1" (`pluginlog.log`).
  - Advanced Launcher → `4.9`.

  So "requires Stream Deck ≥ X" checks always pass.
- `devices[].type` is `0` (Elgato `kESDSDKDeviceType_StreamDeck`). BarRaider decodes it as `StreamDeckClassic`
  6×3 (OBSERVED). **The AKP153E is reported as 6 columns × 3 rows: 5×3 keys plus a column 5 of 3 side "Information"
  cells.**
- `devices` can be `[]` or `[{}]` if the device is not open yet (OBSERVED in older `SM` lines and in Advanced
  Launcher's "Devices:" being empty). Plugins must then wait for `deviceDidConnect`.
- `colors` values are hardcoded literals in `E` (STRINGS @0x25e2488). The `font` literal is not in `E`, so it comes
  from config or skin (INFERRED).

### 1.4 HTML plugins (CEF)
- Loaded as a local file URL (`file:///` literal exists, `allow-file-access-from-files`). CEF switches are in STRINGS
  @0x25e0da8: `allow-file-access-from-files allow-running-insecure-content disable-web-security no-sandbox
  no-user-gesture-required autoplay-policy remote-allow-origins use-mock-keychain enable-media-stream disable-gpu`.
  User agent: `... QCefView/1.1.0 Chrome/109.0.5414.120`.
- After load the host injects `connectElgatoStreamDeckSocket('<port>','<uuid>','registerPlugin','<info>')`. Single
  quotes in the args are escaped as `\u0027` (STRINGS). It calls `connectMiraBoxSDSocket(...)` instead, with the
  same 4 args, for MiraBox-API plugins. The `APIVersion` literal sits next to it, so the INFERRED selector is a
  manifest/API version.
- HotSpot's SDK-HTML sets `const connectSocket = connectElgatoStreamDeckSocket` (OBSERVED), so either name works.
- CEF DevTools: older logs show "Remote debugging server started successfully...". Current `debug.log` shows the
  bind failing ("Cannot start http server for devtools") on every start (OBSERVED).

### 1.5 Lifecycle
- Crash → `"The plugin '%1' crashed: %2"` then `SDPluginManager::restartPlugin "<path>" <n>` (OBSERVED in `L`,
  e.g. volume-controller crashed 36×). The trailing int ranges over -1..2 and is INFERRED to be the exit code or the
  attempt count.
- When the device sleeps the host keeps plugins alive and sends `systemDidWakeUp` (STRINGS "The computer goes to
  sleep").
- Host shutdown or socket close: plugins are expected to exit when their socket closes. `ORCA` does
  `ws.on('close', () => process.exit(0))`.
- PI close logs `尝试重新加载插件或者移除相关数据` ("trying to reload plugin or remove data") followed by
  `SDPluginManager::removePluginCode QWebSocket(...)` (OBSERVED).

### 1.6 Other local servers (not needed for plugins)
- `Store websocket server` on :56018 (store UI).
- `Windows controller websocket server` on :23823 (`SDWebsocketServer` in SDLibrary1.dll).
- `SDTcpServer` in SDLibrary1.dll emits `sigClientKeyDown/KeyUp/DialPressed/DialRotate`. INFERRED: a remote or
  virtual controller input channel.
- Single instance via `QLocalServer` `@MiraBox_StreamDock_LocalServer_Name_` (STRINGS).
- Ports are from `L` ("... listen successful with port: N").

---

## 2. Manifest fields honoured

From real manifests (42 parsed, 6 unreadable because they are encrypted) plus the manifest-key string block in `E`
(STRINGS @0x25b42f8): `MinimumVersion Platform MultiActionImage Name Title ShowTitle TitleColor TitleAlignment
FontStyle FontUnderline coordinates ActionID Actions Settings PropertyInspectorPath States Controllers
SupportedInMultiActions OnlySupportedInMultiActions Tooltip UUID DisableCaching VisibleInActionsList UserTitleEnabled
Author Category CategoryIcon CodePath CodePathMac CodePathWin Description Profiles Readonly
DontAutoSwitchWhenInstalled DefaultWindowSize URL SDKVersion Software ApplicationsToMonitor mac windows PrivateAPI
State Pages`. Also `Icon FontFamily FontSize Image Nodejs Version` exist as separate literals.

**Top level**

| Field | Status | Notes |
|---|---|---|
| `UUID` | used | Optional for Stream Dock: only 3 of 42 manifests have it. The plugin id falls back to the folder name minus `.sdPlugin` (INFERRED from `info.plugin.uuid`) |
| `Name`, `Author`, `Description`, `Version`, `URL`, `Icon`, `Category`, `CategoryIcon` | UI | `Version` → `info.plugin.version` (OBSERVED) |
| `CodePath`, `CodePathWin`, `CodePathMac` | launch | §1.2 |
| `Nodejs` `{Version, Debug}` | launch | Only `"20"` seen; one runtime (`node20.exe`) |
| `SDKVersion` | read | 1 = HotSpot HTML SDK, 2 = Elgato v2. No visible behaviour difference |
| `Software.MinimumVersion` | **echoed** as `info.application.version` | §1.3 |
| `OS[] {Platform, MinimumVersion}` | install gate | `mac`/`windows` literals |
| `PropertyInspectorPath` | PI | Top-level default; per-action value overrides it |
| `DefaultWindowSize` | PI/window size | |
| `ApplicationsToMonitor {windows:[...], mac:[...]}` | → `applicationDidLaunch/Terminate` | `MonitorApplicationsTool` in `L` |
| `Profiles`, `Readonly`, `DontAutoSwitchWhenInstalled` | bundled profiles | |
| `RunAsAdministrator` | launch | Stream Dock-only (seen in 1 manifest) |
| `PrivateAPI` | built-ins only | Native implementation in `E` |
| `StreamdeckID`, `MpProductID`, `MpVariantID`, `MpVersionID`, `IconEditorPath`, `IconMenuItem` | ignored (INFERRED) | Not in the `E` key block |
| `APIVersion` | INFERRED: selects `connectMiraBoxSDSocket` | §1.4 |

**Per action**

| Field | Notes |
|---|---|
| `UUID` (`action` in messages), `Name`, `Tooltip`, `Icon`, `PropertyInspectorPath` | as Elgato |
| `States[] {Image, MultiActionImage, Name, Title, ShowTitle, TitleColor, TitleAlignment, FontFamily, FontSize, FontStyle, FontUnderline}` | as Elgato. HotSpot manifests also use `FSize`/`FFamily`/`xFontSize` (ignored, INFERRED) |
| `Controllers[]` | **Stream Dock values: `Keypad`, `Information`, `SecondaryScreen`, `Knob`** (counts across manifests: 40/24/14/15). Elgato's `Encoder` appears in 2 Elgato manifests (Neewer) and has **no literal in `E`** → INFERRED not mapped to knobs. `Knob` ≈ Elgato `Encoder` |
| `Encoder {layout, background, Icon, StackColor, value, TriggerDescription}` | **ignored** (no `Encoder`/`layout`/`TriggerDescription` literal in `E`) |
| `SupportedInMultiActions`, `OnlySupportedInMultiActions`, `VisibleInActionsList`, `UserTitleEnabled`, `DisableCaching`, `DisableAutomaticStates` | as Elgato (first five STRINGS; `DisableAutomaticStates` seen in 11 manifests) |
| `Settings` | default settings for a new instance (Stream Dock extension, 17 manifests) |

---

## 3. WebSocket protocol

### 3.1 Transport and handshake
- **OBSERVED** `ws://127.0.0.1:18618` (sockets `127.0.0.1:18618` and `[::1]:18618` owned by pid 56004). Text frames,
  one JSON object per frame. No subprotocol, no auth.
- First frame from a plugin: `{"event":"registerPlugin","uuid":"<pluginUUID>"}` (OBSERVED `SM` log and `ORCA`
  line 235). Host log: `SDPluginServer::onTextMessageReceived "Plugin <uuid> <folder>.sdPlugin is now connected"`.
- First frame from a PI: `{"event":"registerPropertyInspector","uuid":"<piUUID>"}`. Host log: `Property inspector
  <uuid> is now connected`. Each PI open gets a **fresh** uuid and closes on deselect (OBSERVED: 8 open/close pairs
  16:35–16:38).
- The `uuid` is how the host knows which plugin a socket belongs to. Frames sent before registration are ignored
  (INFERRED).

### 3.2 Host → plugin events

Envelope (ELGATO-REF, same in Stream Dock):
`{"event", "action":"<action UUID>", "context":"<instance id>", "device":"<device id>", "payload":{...}}`

| Event | Payload keys | Status |
|---|---|---|
| `willAppear` / `willDisappear` | `settings, coordinates{column,row}, controller, state, isInMultiAction` | OBSERVED (`ORCA`: `payload.coordinates`, `payload.controller` = `Keypad` for cols 0-4, `Information` for col 5 rows 0/2) |
| `keyDown` / `keyUp` | `settings, coordinates{column,row}, state, userDesiredState, isInMultiAction` | OBSERVED (`ORCA` acts on keyDown). STRINGS @0x25b5b40 shows the exact key list `action keyDown event context device settings column row state userDesiredState isInMultiAction payload keyUp`. Note from `ORCA`: **keyUp arrives late**, act on keyDown |
| `dialDown` / `dialUp` / `dialRotate` | `settings, coordinates, controller, ticks, pressed` | STRINGS @0x25cb250 (`dialDown dialUp dialRotate ticks pressed`), fired for `Knob` |
| `touchTap` | INFERRED `settings, coordinates, controller` | STRINGS; **no `tapPos`/`hold` literal** |
| `didReceiveSettings` | `settings, coordinates, controller, state?, isInMultiAction?` | STRINGS (`didReceiveSettings controller`); SDK-HTML reads `payload.settings` |
| `didReceiveGlobalSettings` | `settings` | STRINGS |
| `titleParametersDidChange` | `settings, coordinates, state, title, titleParameters{fontFamily,fontSize,fontStyle,fontUnderline,showTitle,titleAlignment,titleColor}` | STRINGS (exact list) |
| `deviceDidConnect` / `deviceDidDisconnect` | `deviceInfo{name,type,size{columns,rows}}` | STRINGS |
| `applicationDidLaunch` / `applicationDidTerminate` | `application` | STRINGS |
| `systemDidWakeUp` | — | STRINGS |
| `propertyInspectorDidAppear` / `propertyInspectorDidDisappear` | — | STRINGS |
| `sendToPlugin` (PI → plugin, relayed) | `payload` = whatever the PI sent | STRINGS; SDK name `didReceivePropertyInspectorMessage` |

Missing compared with ELGATO-REF (no literal in `E`): `didReceiveDeepLink`, `deviceDidChange`, `dialPress`
(deprecated anyway), `didReceiveResources`, `didReceiveSecrets`.

### 3.3 Plugin → host commands

| Command | Shape | Status |
|---|---|---|
| `setImage` | `{context, payload:{image:"data:image/png;base64,…" or svg data URI, target:0\|1\|2, state?}}` | OBSERVED (`ORCA` sends SVG data URIs; SDK-HTML sends PNG data URLs). `E` decodes `png/jpg/jpeg/bmp/svg+xml/webp/gif/x-icon` data URIs (STRINGS @0x25e2488 region). **Rate limit: updates faster than about 400 ms per key are dropped** (`ORCA` `SPIN_MS` comment) |
| `setTitle` | `{context, payload:{title, target, state?}}` | OBSERVED (SDK-HTML) |
| `setState` | `{context, payload:{state}}` | OBSERVED (SDK-HTML) |
| `showAlert` / `showOk` | `{context}` | OBSERVED (`ORCA`) |
| `setSettings` / `getSettings` | `{context, payload:{…}}` / `{context}` → `didReceiveSettings` | OBSERVED (SDK-HTML) |
| `setGlobalSettings` / `getGlobalSettings` | `{context:<pluginUUID>, payload}` / `{context}` → `didReceiveGlobalSettings` | STRINGS |
| `openUrl` | `{payload:{url}}` | OBSERVED (SDK-HTML, no context) |
| `logMessage` | `{payload:{message}}` | STRINGS (INFERRED: written to `L`) |
| `switchToProfile` | `{context, device, payload:{profile}}` | STRINGS |
| `sendToPropertyInspector` | `{action, context, payload}` | OBSERVED (SDK-HTML) |
| `setFeedback` | `{context, payload:{…}}` | STRINGS; INFERRED for Knob/SecondaryScreen screens |
| `setText` | ? | STRINGS, Stream Dock-only, next to `setFeedback`. INFERRED: text for SecondaryScreen |
| **`sendToDevice`** | ? | STRINGS, Stream Dock-only (raw device payload) |
| **`getScreenshot`** | ? → reply | STRINGS, Stream Dock-only |
| **`getSystemAudioVolume`** | → `{leftChannel, rightChannel}` | STRINGS, Stream Dock-only |
| **`startAudioCapture` / `stopAudioCapture`** | ? | STRINGS, Stream Dock-only |
| **`getDetectedSensorsData`** | ? | STRINGS, Stream Dock-only (used by SystemMonitor) |
| folder-profile hooks | `onSwitchToFolderProfile` / `onSwitchFromFolderProfile` | STRINGS (between `switchToProfile` and `sendToPropertyInspector`) |

Missing compared with ELGATO-REF: `setFeedbackLayout`, `setTriggerDescription`, `getResources`/`setResources`,
`getSecrets`. Insta360's bundled `@elgato/streamdeck` sends `setFeedbackLayout`/`setTriggerDescription` and still
works (OBSERVED), so INFERRED: unknown events are dropped silently.

### 3.4 Controllers and geometry (the side panel)
- **OBSERVED** (`ORCA plugin.log` 16:32:54): `appear clock col=5 row=0 controller=Information` and `appear summary
  col=5 row=2 controller=Information`. Columns 0-4 report `controller=Keypad`.
- The profile stores the slot as key `"5,0"` with `"Controller": ""`, so the controller is **derived from the slot**,
  not stored (OBSERVED in `profiles\R0KZ54Z0-...sdProfile\manifest.json`).
- An action only shows in a slot type listed in its `Controllers` (INFERRED from the UI). For a side cell, list
  `"Information"`.
- Side cells take `setImage` like keys (OBSERVED: clock and summary render).
- `SecondaryScreen` (touch strip on N4-class devices) and `Knob` (dials) are not present on the AKP153E, so they
  could not be checked at runtime.

---

## 4. Property inspector

- **Host side**: `SDPropertyInspectorWidget` embeds a QCefView called `PropertyInspectorWebView` in the main window
  (STRINGS @0x25d2190). A non-HTML-plugin action (vMix) gets a native Qt PI instead.
- The page is `PropertyInspectorPath` relative to the plugin folder, loaded as a local file (INFERRED from
  `file:///` and `allow-file-access-from-files`). It is created when the action is selected and destroyed on
  deselect (OBSERVED: PI connect/disconnect pairs in `L`).
- **Bridge**: after load the host evaluates **one** of these (STRINGS @0x25d2108):
  - `connectElgatoStreamDeckSocket('%1','%2','%3','%4','%5')` → `(port, piUUID, "registerPropertyInspector", info,
    actionInfo)`, as in ELGATO-REF.
  - `connectMiraBoxSDSocket('%1','%2','%3','%4','%5')`.
  - `connectSocket('%1','%2','%3','%4','%5')`.
- `actionInfo` = `{action, context, device, payload:{settings, coordinates}}`. SDK-HTML `action.js` reads
  `info.action` and `info.context` (OBSERVED).
- **Divergence (INFERRED)**: SDK-HTML sends `sendToPlugin` with `context: <piUUID>` but `setSettings` with
  `context: <actionInfo.context>`. So in Stream Dock the PI uuid is **not** the action context (Elgato makes them
  equal), and the host maps piUUID → action instance.
- PI commands: `setSettings`, `getSettings`, `setGlobalSettings`, `getGlobalSettings`, `sendToPlugin`, `openUrl`,
  `logMessage`, `setState`/`setImage` (SDK-HTML does both from the PI).
- PI receives: `didReceiveSettings` (right after register, OBSERVED via SDK-HTML which waits for it before showing
  UI), `didReceiveGlobalSettings`, `sendToPropertyInspector`.
- Stream Dock-only: SDK-HTML defines `onFilePickerReturn(url)` as a callback for `<input type=file>` (OBSERVED in
  `action.js`; not found as a literal in `E`, so it is probably built at runtime).

What a replacement must serve for PIs:
- An HTTP or `file://` origin for the plugin folder.
- A browser context with permissive CORS. Elgato PIs (sdpi-components) pull from CDN and local files.
- The injected `connectElgatoStreamDeckSocket(...)` call.
- PI uuid = action context. That works for both conventions as long as `sendToPlugin` routes by registered socket,
  not by the `context` field.

---

## 5. Persistence

| What | Where | Status |
|---|---|---|
| Per-action settings | `%APPDATA%\HotSpot\StreamDock\profiles\<PROFILEID>.sdProfile\manifest.json` → `Actions["col,row"] = {ActionID, Controller, Name, Settings, State, States[], UUID}`. Top level: `AppIdentifier*, DeviceModel, DeviceSerialNumber, DeviceUUID ("AKP153E"), Name, Pages{Current, Pages[]}, Version` | OBSERVED. Folder profiles are nested `<id>.sdProfile\` with `col,row\CustomImages\state0.gif` |
| Which profile is active per device | `%APPDATA%\HotSpot\StreamDock\config\StreamDockConfig.ini` `[AKP153E355499441494DeviceConfig] sdProfileName/sdProfilePath` | OBSERVED |
| Profile list per device model | `config\DataCache.db` (SQLite) table `AKP153E(Profile)`. Also `AJAZZ(Device, serialNumber)` and `OBSServerInfo` | OBSERVED |
| Action-list visibility | `HKCU\Software\HotSpot\StreamDock\<plugin>.sdPlugin\<actionUUID> = "true"/"false"` and INI `<lang>\ActionState\...` | OBSERVED |
| **Global settings** | QtKeychain → Windows Credential Manager. `E` `SDActionCanvasWidget::onGetGlobalSettings` sits next to `keychain.streamdock.project.app`, `pluginName`, `Read key failed: %1`; SDLibrary1 `KeyChainClass::{read,write,delete}Key`. Credential entries `com.insta360.webcam/com.insta360.webcam` and `com.elgato.keycreator/...` exist, but they could have been written by Elgato's own app, which is installed. No `keychain.streamdock.project.app/*` entry exists today | STRINGS + INFERRED. Not confirmed: needs one `setGlobalSettings` test followed by `cmdkey /list` |
| Plugin-private data | the plugin's own folder (e.g. `plugin.log`, `logs\`) | OBSERVED |
| CEF localStorage of HTML plugins/PIs | `%APPDATA%\HotSpot\StreamDock\WebCache\Local Storage` | OBSERVED (dir) |

---

## 6. Elgato vs Stream Dock

| Area | Elgato SDK v2 | Stream Dock 3.10 |
|---|---|---|
| Port | random per launch | **fixed 18618** |
| `-pluginUUID` | random | random 32 `[A-Z0-9]` |
| `info.application.version` | real app version | **plugin's `Software.MinimumVersion`** |
| `info.devices[].type` | real model enum | `0` for the AKP153E, size 6×3 |
| Controllers | `Keypad`, `Encoder` | `Keypad`, **`Information`**, **`SecondaryScreen`**, **`Knob`** (no `Encoder`) |
| Encoder layouts (`setFeedbackLayout`, `Encoder.layout`) | yes | no |
| `setTriggerDescription`, deep links, resources, secrets | yes (SDK 6.5–7.1) | no |
| `touchTap.tapPos/hold` | yes | no |
| Extra commands | — | `sendToDevice`, `getScreenshot`, `getSystemAudioVolume`, `start/stopAudioCapture`, `getDetectedSensorsData`, `setText` |
| PI bridge | `connectElgatoStreamDeckSocket` 5 args | same, plus `connectMiraBoxSDSocket`/`connectSocket` |
| PI uuid | = action context | separate id (INFERRED) |
| Encrypted manifests (`ELGATO\x01`) | yes | loads them (mechanism unknown, §8) |
| Plugin folder | `%APPDATA%\Elgato\StreamDeck\Plugins` | `%APPDATA%\HotSpot\StreamDock\plugins` |
| Image rate | ~10–30 fps | drops faster than ~2.5/s per key |
| keyUp timing | prompt | late |

---

## 7. Proposed minimal replacement host

Goal: run unmodified Elgato v2 plugins, Stream Dock plugins and ours against our own device driver. Node is already
in the stack, so the host is one Node process with `ws`.

**Process model**
1. On start, read every `*.sdPlugin\manifest.json` from the configured roots:
   - `%APPDATA%\HotSpot\StreamDock\plugins`
   - `%APPDATA%\Elgato\StreamDeck\Plugins`
   - our own folder.

   Skip manifests with `PrivateAPI` or no code path. Keep a map `pluginId → {dir, manifest}`, where pluginId is
   `manifest.UUID` or the folder name minus `.sdPlugin`.
2. Listen on `127.0.0.1:<port>`. Use 18618 so it is a drop-in for Stream Dock; it must not run alongside Stream Dock
   (or pick a free port).
3. Per plugin, generate a 32-char uuid and build `info` (§1.3):
   - `application.version`: `max(Software.MinimumVersion, "6.9")`, which satisfies Elgato SDK feature gates and
     Stream Dock plugins alike.
   - `devices`: the real grid, e.g. `{columns:6, rows:3}` for the AKP153E.
4. Launch:
   - `.exe`: `spawn(exe, args, {cwd: dir})`.
   - `.js/.mjs/.cjs`: `spawn(nodeBin, [codePath, ...args], {cwd: dir})`. `nodeBin` = Stream Dock's `node20.exe` or
     the system node, `Nodejs.Version` is advisory. Map `Nodejs.Debug` to `--inspect=127.0.0.1:<n>`.
   - `.html`: a hidden headless Chromium page (Playwright/Electron/WebView2) at `file:///<dir>/<CodePath>`, then
     `page.evaluate("connectElgatoStreamDeckSocket('port','uuid','registerPlugin','<info json>')")`.

   args = `-port P -pluginUUID U -registerEvent registerPlugin -info <json>`, passed as an argv array so that no
   shell quoting is needed. Restart on exit with backoff.
5. Sockets: on the first frame `{event, uuid}`, bind the socket to a plugin (`registerPlugin`) or to a PI
   (`registerPropertyInspector`). Close the socket on an unknown uuid.

**State the host owns**
- `instances: context → {pluginId, action, device, column, row, controller, settings, state}`. Load it from our own
  profile file. JSON, same `"col,row"` keyed shape as Stream Dock so profiles can be imported.
- `globalSettings: pluginId → object` in a JSON file (not the credential store).
- `controller` comes from the slot: the side column → `"Information"`; dials → `"Knob"` for Stream Dock manifests,
  `"Encoder"` for Elgato manifests (pick based on which name the action's `Controllers` lists).

**Routing (the whole core)**
- Device input → plugin:
  - Key press → `keyDown`/`keyUp {settings, coordinates, state, userDesiredState, isInMultiAction:false}`.
  - Dial → `dialRotate {ticks, pressed}`, `dialDown`, `dialUp`.
  - Touch → `touchTap`.
- Page or profile change: for each slot, send `willDisappear` to the old instance and `willAppear` to the new one.
  `willAppear` must arrive before any other event for that context.
- Plugin → host:
  - `setImage`/`setTitle`/`setState` → render to the device. Accept data URIs (png/jpeg/svg/gif/webp), coalesce
    per key, and cap the rate per the driver's limit.
  - `setSettings` → persist, then echo `didReceiveSettings` to the PI.
  - `getSettings` → `didReceiveSettings`.
  - `setGlobalSettings` → persist, then `didReceiveGlobalSettings` to the PI and the plugin. Elgato echoes to the
    *other* side.
  - `getGlobalSettings` → reply.
  - `showOk`/`showAlert` → overlay a glyph.
  - `openUrl` → `start <url>`.
  - `logMessage` → host log.
  - `switchToProfile` → profile switch.
  - `sendToPropertyInspector` → the PI socket whose context matches.
- PI → host: `sendToPlugin` → plugin socket, as `{event:"sendToPlugin", action, context, payload}`. Settings
  commands as above.
- System: `deviceDidConnect`/`Disconnect`, `systemDidWakeUp` (from `powerMonitor` or WM_POWERBROADCAST), and
  `applicationDidLaunch`/`Terminate` for names listed in `ApplicationsToMonitor` (poll the process list every 2 s).
- Unknown commands: drop them and log once per name. That covers `setFeedbackLayout`, `setTriggerDescription`
  and the other Elgato-only calls.

**Must implement to run unmodified Elgato plugins**: argv and `info`; register; `willAppear`/`willDisappear`;
`keyDown`/`keyUp`; `setImage`/`setTitle`/`setState`; settings and global settings with their `didReceive*` replies;
`sendToPlugin`/`sendToPropertyInspector`; `showOk`/`showAlert`; `openUrl`; `logMessage`; PI hosting with
`connectElgatoStreamDeckSocket`.

**Can skip at first**: dials/touch (the AKP153E has neither), `titleParametersDidChange` (only matters with a title
editor UI), multi-actions, `switchToProfile`, deep links/resources/secrets, and the Stream Dock-only extras. Of
those, only SystemMonitor (sensors) and `getSystemAudioVolume`-style built-ins need them.

**Property-inspector UI**: optional for a headless host. Plugins work without a PI as long as their settings exist.
Add one later as a small local web page that iframes the PI and injects the bridge.

---

## 8. Open items / risks

1. **Encrypted Elgato manifests.** camerahub, obsstudio, volume-controller, advancedlauncher and windowscockpit have
   `manifest.json` = `ELGATO\x01\x00` + ciphertext (OBSERVED, identical bytes to the Elgato copies). Stream Dock
   still launches them: camerahub "is now connected" 26× in the last 40 logs. No `ELGATO` literal appears in `E` or
   any install DLL, so decryption is somewhere unidentified (a magic compared as an integer, or OpenSSL in
   `libcrypto-1_1-x64.dll`). Options:
   - (a) RE that path. A follow-up decompile of `SDPluginManager::loadPlugin` in `E` is the shortest route (not done,
     to stay inside the brief's budget).
   - (b) Reconstruct the needed fields from evidence: action UUIDs from `HKCU\Software\HotSpot\StreamDock\<plugin>`
     value names, CodePath from the folder contents (`index.html`, `bin/plugin.js`, `*.exe`).
   - (c) Exclude those plugins.
2. Global-settings store not confirmed (§5). One `setGlobalSettings` from our plugin plus `cmdkey /list` would settle
   it.
3. Exact payloads of the Stream Dock extras (`sendToDevice`, `getScreenshot`, `setText`, `setFeedback` on
   `SecondaryScreen`) are unknown. They need a decompile of `SDPluginServer::onTextMessageReceived` (string block
   @0x25cb250–0x25cb4b8) or a WebSocket capture. A capture would mean a logging shim on 18618, which is invasive
   while Stream Dock runs.
4. INFERRED items to confirm with a test plugin that logs every frame:
   - the PI uuid ≠ context mapping;
   - `touchTap` payload;
   - whether `keyDown` carries `state`.
