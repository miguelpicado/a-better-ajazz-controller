# Using OrcaDeck

> Español: [USAGE.md](USAGE.md).

`orcadeck` is the lightweight replacement for "Stream Dock AJAZZ" for the AKP153E keyboard. It reads
Stream Dock profiles as they are and runs SDK v2 plugins (`.exe` and Node) plus built-in Orca actions.

- Architecture and the reason for the timings: [ARCHITECTURE.md](../design/ARCHITECTURE.md).
- Building and testing: [DEVELOPMENT.md](DEVELOPMENT.md).
- Failures and error messages: [TROUBLESHOOTING.en.md](TROUBLESHOOTING.en.md).
- Status and open items: [HANDOFF.md](../project/HANDOFF.md), [STATUS.md](../project/STATUS.md), [ROADMAP.md](../project/ROADMAP.md).

## 1. Before you start

1. **Stop Stream Dock** if it is running: `orcadeck run` uses the same HID and the same port 18618.
   ```powershell
   schtasks /end /tn StreamDock
   ```
   To give control back: `schtasks /run /tn StreamDock`.
2. Locate your profile:
   ```powershell
   Get-ChildItem "$env:APPDATA\HotSpot\StreamDock\profiles" -Directory
   ```
   The profile is the `<ID>.sdProfile` folder; pass that path, not `manifest.json`.
3. Check that the device shows up (read-only, sends nothing):
   ```powershell
   .\orcadeck.exe probe
   ```
   With the monitors off, the USB hub does not power the keyboard and `probe` will not find it.

## 2. Subcommands

All are implemented in [`daemon/src/main.rs`](../../daemon/src/main.rs); `run` and `edit` delegate to
[`app.rs`](../../daemon/src/app.rs) and [`edit.rs`](../../daemon/src/edit.rs).

| Command | Syntax | What it does | Touches the device? |
|---|---|---|---|
| `render` | `render <profile.sdProfile> <folder>` | Renders the scene to `keyNN.jpg` without opening the HID. Ideal for reviewing a profile. | No |
| `probe` | `probe` | Enumerates the AKP153E, prints the HID descriptor and the deduced `N`. Read-only. | No (read-only) |
| `scene` | `scene <profile.sdProfile>` | Starts the HID, clears, paints every key and sends an `STP`. | Yes |
| `run` | `run <profile.sdProfile>` | The full app: driver + plugin host + built-in actions. | Yes |
| `edit` | `edit [--port <N>] [--no-open]` | Starts the web editor on loopback and opens it in the browser. | No |
| `bright` | `bright <0-100>` | Changes the brightness (`LIG`). Values above 100 are clamped. | Yes |
| `watch` | `watch [mode]` | Init, optional `MOD` and a raw dump of the input; heartbeat every 10 s. Experimental. | Yes |
| `poll` | `poll [input\|feature]` | Reads `GET_REPORT` every 30 ms and shows changes. Experimental. | Yes (read) |
| `hidwatch` | `hidwatch` | Enumerates HID every 100 ms, without opening or writing. Flags `EEEF:2929` (bootloader). | No (enumerates only) |
| `install` | `install <package>` | Opens the editor with the package staged for install; nothing installs by itself. | No |
| `register` | `register` | Associates `.streamDeckPlugin` and `.sdPlugin` with OrcaDeck for the current user (no admin). | No |
| `unregister` | `unregister` | Undoes the association and restores the previous one. | No |
| `help` | `help`, `-h`, `--help` | Prints the command list. | No |

Notes:

- `render` creates the output folder if it does not exist and writes `key01.jpg`…`key18.jpg`. Keys
  1–15 without an action are painted black; side keys 16–18 only if the scene uses them.
- `scene`, `watch` and `poll` call `init(80)`: `DIS`, `LIG` and `CLE ff`, so they **clear the screen**
  before continuing. `bright` only sends `LIG`.
- `watch` accepts a numeric mode (`watch 2` sends `CRT MOD` with that value), but on the AKP153E v1
  `MOD` does not change the behaviour: the event still arrives on release and with `state = 0`.
- `poll feature` returns "Función incorrecta" (wrong function) on this unit; this is expected. `poll input`
  returns the version string and does not reflect a held key.
- `orcadeck help` (also `-h`/`--help`) prints the command list. `edit --help` also exists.

## 3. Usage flow

### 3.1 Normal use

```powershell
# 1. Parar la app oficial (mismo HID y puerto 18618)
schtasks /end /tn StreamDock

# 2. Arrancar OrcaDeck con la escena Orca
cd daemon
.\target\release\orcadeck.exe run "$env:APPDATA\HotSpot\StreamDock\profiles\R0KZ54Z0-063U-947X-31BI-GG95R8F4HT88.sdProfile"
```

`run` prints `device ready, N = …` and stays in the foreground. Ctrl+C or closing the console terminates
the process (there is no orderly shutdown of child plugins). If the device disconnects, it retries
every 2 s and repaints the last image of each key when it comes back.

What is started:

- The plugin host on `127.0.0.1:18618`, fixed.
- Only the plugins used by the scene; if a plugin was already alive from a previous scene, it is
  reused (not restarted) so its settings are not lost.
- The built-in actions thread (agent, quota, clock, summary), even if the scene does not use them.

### 3.2 Stopping and returning to the official app

```powershell
# Ctrl+C en la consola de orcadeck, o cerrar la ventana
schtasks /run /tn StreamDock
```

Do not run `run` while Stream Dock is open: the HID `open` or the 18618 `bind` will fail.

## 4. Environment variables

The daemon paths hang off `%APPDATA%`; the editor also looks at `%USERPROFILE%\Downloads` and
autostart uses `%LOCALAPPDATA%`. The `ORCADECK_*` variables are read at the point indicated; there is
no configuration file for them.

| Variable | Default | Effect | Where it is read |
|---|---|---|---|
| `ORCADECK_REPORT_LEN` | HID descriptor; if it fails, 512 | Forces the `N` size of the output payload. Useful if the descriptor cannot be read. | [`daemon/src/main.rs`](../../daemon/src/main.rs) |
| `ORCADECK_PLUGINS` | `%APPDATA%\HotSpot\StreamDock\plugins` | Plugin folder for `run`, `render`, `scene` and `watch`; also the editor's plugin root. | [`main.rs`](../../daemon/src/main.rs), [`edit.rs`](../../daemon/src/edit.rs) |
| `ORCADECK_PROFILES` | `%APPDATA%\HotSpot\StreamDock\profiles` | Profiles folder **for `edit` only**. `run` receives the profile as an argument. | [`edit.rs`](../../daemon/src/edit.rs) |
| `ORCADECK_DATA` | `%APPDATA%\OrcaDeck` | `state.json` and overlays **for `edit` only**. `run` always uses `%APPDATA%\OrcaDeck`. | [`edit.rs`](../../daemon/src/edit.rs) |
| `ORCADECK_BUILTIN_PLUGINS` | empty; Stream Dock `plugins` folders are added if they exist | Extra read-only plugin roots for `edit`, separated by `;`. | [`edit.rs`](../../daemon/src/edit.rs) |
| `ORCADECK_GAP_MS` | `250` | Minimum milliseconds between paint batches. Read when each device session is opened. **Do not lower it.** | [`app.rs`](../../daemon/src/app.rs) |
| `ORCADECK_TRACE` | off (any value enables it) | Writes each HID input and each batch duration to stderr. | [`app.rs`](../../daemon/src/app.rs) |
| `ORCADECK_CMD` | off | `watch` experiment hook: `VERB:0a=01,0b=00` (offsets and values in hexadecimal). Sends a `CRT` and continues. | [`main.rs`](../../daemon/src/main.rs) |
| `ORCADECK_SCENE` | off | With `ORCADECK_CMD`: after the command, sends `LIG`, `CLE`, the images of that profile (with `STP` per key) and `CONNECT`. | [`main.rs`](../../daemon/src/main.rs) |
| `QUOTA_DIR` | `%APPDATA%\OrcaDeck\quota` (if there is no `APPDATA`: `<temp>\OrcaDeck\quota`) | Folder for `quota-status.json` and `.wake` used by the quota keys. | [`builtin/src/actions.rs`](../../daemon/crates/builtin/src/actions.rs) |
| `APPDATA` | the Windows user's | Base for all default paths. | `main.rs`, `app.rs`, `edit.rs`, `orca.rs` |

`ORCADECK_CMD` and `ORCADECK_SCENE` are probing tools, not normal usage options. `watch` also runs
`init`, so images or screen clearing may appear.

## 5. `run` in detail

- **Port**: 18618 fixed, no option. If it is busy, `run` fails when starting the host.
- **Profile**: read at startup and when changing scenes. Editor edits are not reloaded by themselves,
  except the hot reload of one key when `edit` runs inside `run` (see §7).
- **Scene Shift**: yes, it reloads the target scene manifest (see §6).
- **Batches**: up to the wait gap between batches (250 ms by default, `ORCADECK_GAP_MS`; setting
  `ORCADECK_GAP_MS=400` restores the conservative gap). This is deliberate: the firmware drops
  presses if it is painted without pauses.
- **Heartbeat**: `CONNECT` every 10 s.
- **Reconnection**: if the USB disappears, `device: …; retrying in 2 s`; when it comes back, `init`
  and a repaint.
- **Plugin settings**: `setSettings` is written to the cell's `manifest.json` with a 1 s debounce;
  `setGlobalSettings` to `%APPDATA%\OrcaDeck\globals\<plugin>.json`.
- **Feedback**: `showOk`/`showAlert` overlay a check or cross for 600 ms; then the current image
  returns.
- **Titles**: 14 characters maximum; the rest is truncated with `…`.
- **`.html` plugins**: not launched. `.js` needs `node` on `PATH`.

## 6. Scene Shift

The `com.hotspot.streamdock.profile.rotate` action ("Scene Shift") is implemented by `run` itself,
not by a plugin. In the Orca scene it occupies `2,0` (direct jump to Recording) and `4,2` (direct
jump to Monitoring).

`Settings.ProfileUUID` decides the target:

| Value | Target |
|---|---|
| absent or `""` | next scene |
| `"PreviousScene"` | previous scene |
| any other id | that scene (accepted with or without `.sdProfile`) |

- The order is the device list order in `%APPDATA%\OrcaDeck\state.json` (`order`). If that file does
  not exist yet, it is taken from `DataCache.db` (read-only). A direct id only works if
  `<id>.sdProfile\manifest.json` exists and the id is safe.
- "Next/previous" wraps around at either end. If the current scene is not in the list, "next" is the
  first scene and "previous" the last.
- On change: it loads the scene, writes `state.json.current`, removes the old contexts, places the
  new ones, launches missing plugins and repaints every key that may change in one batch.
- It does not kill the previous scene's plugins: they stay alive to keep their settings.
- If the target is the current scene, it does nothing. There is no multi-page support:
  `PageIndex` is ignored.
- **Status**: implemented with unit tests; physical testing with the keyboard is pending (see
  [ROADMAP](../project/ROADMAP.md)). The design and evidence are in
  [`design/scene-shift.md`](../design/scene-shift.md).

To change the target from the editor: select the key and use **Go to scene** (Next scene or a
specific scene), or edit `Settings.ProfileUUID` in **JSON settings**. The dropdown does not offer
"previous"; for that, type `PreviousScene` by hand:

```json
{ "DeviceUUID": "", "ProfileUUID": "PreviousScene" }
```

Restart `run` if you added or removed the key, and also if you change the target of a key in the
current scene: `run` does not re-read that scene until it enters it again. If you change the target
of another scene, it is read when jumping to it.

## 7. Web editor: `edit`

```powershell
cd daemon
.\target\release\orcadeck.exe edit
# o:
.\target\release\orcadeck.exe edit --port 8765 --no-open
```

- It listens only on `127.0.0.1`; the default port is a free one. It opens the browser unless
  `--no-open`.
- The printed URL carries a session token; it is exchanged for an `HttpOnly` cookie and required on
  every API call. Do not share that URL.
- Ctrl+C stops the server and frees the port.
- It does not touch the HID directly and does not need the keyboard. If `run` is alive, `edit` runs
  inside `run` (named pipe `ctl.rs`); if not, it starts standalone. In the first case, saving a cell
  hot-reloads that key; in the second, changes are not reloaded.
- Two views: **Keys** (scenes, 5×3 grid + side panel, drag actions, title/JSON inspector, brightness)
  and **Plugins** (status, manifest, install/remove).
- The brightness saved by the editor is a stub: `run` starts at 80 and the real brightness is changed
  with `orcadeck bright`.
- The plugin Property Inspector loads in an isolated iframe (sandbox without `allow-same-origin`)
  with a one-use token via `postMessage`; if the plugin has no PI, the inspector shows title and
  settings JSON.

To test without touching your real profiles, point the editor at copies:

```powershell
$env:ORCADECK_PROFILES = "C:\temp\perfiles-copia"
$env:ORCADECK_PLUGINS  = "C:\temp\plugins-copia"
$env:ORCADECK_DATA     = "C:\temp\datos-copia"
.\orcadeck.exe edit
```

## 8. Installing and removing plugins from `edit`

The editor uses `orcadeck-plugman`; it does not need the Elgato app and does not decrypt Marketplace
manifests.

### Double-click a package: `register`, `install`, `unregister`

```powershell
.\target\release\orcadeck.exe register              # una vez, sin admin
.\target\release\orcadeck.exe install C:\ruta\MiPlugin.streamDeckPlugin
.\target\release\orcadeck.exe unregister            # deshacer
```

- `register` writes only to `HKCU\Software\Classes` (user, no admin): it creates the `OrcaDeck.Plugin`
  ProgID with the command `<path to orcadeck.exe> install "%1"` and associates the
  `.streamDeckPlugin` and `.sdPlugin` extensions. From then on, double-clicking the package (or
  "Open with" → OrcaDeck) opens the editor on the install page with that package staged.
- `install <package>` does the same from the console. If `orcadeck run` is alive, it asks the editor
  over the named pipe (the install shows up live); if not, it starts a standalone editor. Ctrl+C
  closes the editor, as in `edit`.
- The page shows name, UUID, version, author and whether a version is already installed. **Nothing
  installs by itself**: you must press **Install plugin** (and confirm if replacing). After that,
  drag its actions to a key from the **Keys** view.
- The file path does not travel in the URL: the server keeps it behind a one-use token. If you
  install and reload the page, the link no longer works.
- If you move `orcadeck.exe`, run `register` again.
- `unregister` deletes the ProgID and returns each extension to the default value it had before the
  first `register` (stored as `PreviousDefault.<ext>` under the ProgID). If Windows still opens
  another app, choose OrcaDeck in "Open with…" or reset the association in Settings: Windows may
  keep its own per-user choice (`UserChoice`) above this entry.

1. Download the `.streamDeckPlugin` with your browser session (Marketplace → Download). A `.sdPlugin`
   (folder or zip) or a `.zip` also works. With the association registered, double-click the file
   and skip to step 3.
2. In **Plugins**, press **Search Downloads**. `%USERPROFILE%\Downloads` is scanned **only when
   pressed**: `.streamDeckPlugin`/`.zip` packages and `.sdPlugin` folders with a manifest, modified
   in the last 30 days, at most 20. There is no watcher or polling.
3. It is inspected before installing: UUID, actions, whether the manifest is encrypted and whether a
   manual manifest is needed.
4. **Install plugin** copies to `%APPDATA%\HotSpot\StreamDock\plugins\<uuid>.sdPlugin` (the same
   folder `run` reads). If it already exists, the editor asks for confirmation and reinstalls with
   `force`, leaving `<uuid>.sdPlugin.bak-<timestamp>`.
5. If the manifest is `ELGATO` and the reconstruction is `full`, the overlay
   `%APPDATA%\OrcaDeck\manifests\<uuid>.json` is written. If it is `partial`/`none`, the plugin is
   copied but not launched until a vendor manifest is supplied with
   `orcadeck-plugman install --manifest <json>`.
6. **Remove plugin** is only available for plugins installed by OrcaDeck (`state.json` →
   `installed`). It does not delete built-ins or plugins that were already there. To remove one that
   `run` is using, stop `run` first.

`.js` needs `node` on `PATH`; `.html` is not executed. Format details and the reference installer:
[`design/plugin-install-handoff.md`](../design/plugin-install-handoff.md) and
[`tools/legacy/plugin-install/README.md`](../../tools/legacy/plugin-install/README.md).

## 9. Autostart

[`tools/autostart/orcadeck-task.ps1`](../../tools/autostart/orcadeck-task.ps1) registers a normal-user
task (no admin) that starts `orcadeck run` at sign-in.

```powershell
# Copia el exe y libunwind.dll a %LOCALAPPDATA%\OrcaDeck\bin y registra la tarea DESACTIVADA
.\tools\autostart\orcadeck-task.ps1 install

# Actívala solo cuando la tarea de Stream Dock esté desactivada
# (esto último exige una consola elevada):
schtasks /change /tn StreamDock /disable
.\tools\autostart\orcadeck-task.ps1 enable

.\tools\autostart\orcadeck-task.ps1 status
.\tools\autostart\orcadeck-task.ps1 disable
.\tools\autostart\orcadeck-task.ps1 uninstall
```

- Default profile: `R0KZ54Z0-063U-947X-31BI-GG95R8F4HT88` (the Orca scene). Change it with
  `-Profile <id>`.
- `-Build` points to `daemon\target\release` by default.
- It copies `libunwind.dll` next to the executable: this avoids error `-1073741515` when starting
  without the llvm-mingw `PATH`.
- Log: `%LOCALAPPDATA%\OrcaDeck\orcadeck.log` (appended, never rotated).
- The `OrcaDeck` task is registered disabled on purpose; enabling it while Stream Dock is active
  breaks both (same HID and port 18618).

## 10. Hard rules

- **Do not flash the firmware.** Do not run `FirmwareUpgradeTool.exe` with 4 arguments (it flashes
  instantly, without confirmation), and do not send `CRT\0\0APP NEW` or update commands. Only with an
  explicit user order and a recovery plan.
- **Do not run Stream Dock and OrcaDeck at the same time.**
- Do not delete or move the large files in the repository root: they belong to the user.
