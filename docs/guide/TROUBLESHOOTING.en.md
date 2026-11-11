# Troubleshooting and diagnostics

> Español: [TROUBLESHOOTING.md](TROUBLESHOOTING.md).

Quick check order, from the most common to the least common. If the problem is a build failure, go
straight to §10.

```powershell
# ¿Ve el teclado? (solo lectura)
.\orcadeck.exe probe

# ¿Quién ocupa el HID o el puerto?
Get-Process orcadeck, node, "Stream Dock AJAZZ" -ErrorAction SilentlyContinue
Get-NetTCPConnection -LocalPort 18618 -ErrorAction SilentlyContinue | Select-Object State, OwningProcess

# Log del autoarranque
Get-Content "$env:LOCALAPPDATA\OrcaDeck\orcadeck.log" -Tail 50
```

Rule that overrides all others: **do not flash the firmware**. See §9.

## 1. `-1073741515` on startup (libunwind.dll)

- Symptom: the process dies instantly; in PowerShell `$LASTEXITCODE` is `-1073741515`
  (`0xC0000135`, `STATUS_DLL_NOT_FOUND`).
- Cause: the gnullvm binary needs `libunwind.dll` and it is not on `PATH` or next to the `.exe`. This
  happens when running it from Explorer, a scheduled task or a console without the llvm-mingw `PATH`.
- Fix:
  ```powershell
  $env:PATH="C:\Tools\llvm-mingw-20261104-ucrt-x86_64\bin;$env:PATH"
  .\orcadeck.exe probe
  ```
  Or copy `libunwind.dll` next to `orcadeck.exe`. The autostart script already copies it to
  `%LOCALAPPDATA%\OrcaDeck\bin`.
- It is not a code or device failure.

## 2. "device not found" / `probe` finds nothing

Message from `Dock::open`: `device not found (monitors on? Stream Dock closed?)`.

1. **Monitors on**: the keyboard hangs off the monitor's USB hub; with the monitors off it does not
   appear in the system.
2. **Stream Dock closed**: if the official app has the HID open, `orcadeck` cannot use it.
   ```powershell
   schtasks /end /tn StreamDock
   ```
   If the process survives, close it from an elevated console.
3. **Cable/hub**: try another USB port directly on the PC.
4. **Another `orcadeck run` instance**: it also holds the HID.
5. `hidwatch` does not open the device: use it to see whether the USB appears and disappears, and to
   detect `EEEF:2929` (bootloader).

## 3. Stream Dock must be stopped (same HID and port 18618)

`orcadeck run` and "Stream Dock AJAZZ" cannot coexist: they share the HID device and port 18618 of
the plugin host.

- Stop: `schtasks /end /tn StreamDock`.
- Restart: `schtasks /run /tn StreamDock`.
- Check: `Get-ScheduledTask -TaskName StreamDock | Select-Object TaskName, State`.
- If `run` fails at startup with `plugin host: …` and the port is `LISTEN`, Stream Dock is almost
  certainly still alive. Also close any orphaned `node20` processes it left behind.

## 4. Port 18618 is busy

Only one process can listen on `127.0.0.1:18618`. `run` uses it fixed; `edit` does **not**.

```powershell
Get-NetTCPConnection -LocalPort 18618 -State Listen |
  Select-Object LocalAddress, LocalPort, OwningProcess
Get-Process -Id <OwningProcess>
```

- If the owner is `Stream Dock AJAZZ` or one of its `node20` processes: stop the official app.
- If it is another `orcadeck`: a `run` is already in progress; do not start two.
- `edit` uses a free port (or the one passed with `--port`), so it does not collide with 18618.

## 5. Presses are lost

This is a known firmware problem on the AKP153E v1, not an app problem.

- While the device receives images, it **drops presses**. Measured: painting per key every ~120 ms
  gave zero presses in 80 s.
- Each image costs a fixed 25–30 ms of HID writing, even if it is small.
- `run` mitigates it with batches (all pending keys together) and a gap of 250 ms
  (`ORCADECK_GAP_MS`; `ORCADECK_GAP_MS=400` restores the conservative value), plus JPEG q75.
- If presses are still lost, **raise** the gap, do not lower it:
  ```powershell
  $env:ORCADECK_GAP_MS=500
  ```
- A scene change paints all keys in one batch; an immediate press may fall in that gap. This is
  normal.
- Remember: the event arrives **on release**, not on press. If you test "hold and watch", you will
  see nothing until you release. There is no long press or down/up with this firmware.

## 6. The press arrives late or a built-in key stutters

- Plugin actions depend on the plugin; built-in actions depend on the `builtin` thread.
- That thread calls Orca: `orchestration.workerList` takes ~450 ms and is queried every 10 s. During
  that call the spinner may advance in jumps; this is the documented trade-off so the HID loop is
  not slowed down. Restarting `run` does not improve it.
- Agent keys focus an Orca pane; if Orca is busy or the pipe does not respond, the action may take
  time or return an alert.
- `ORCADECK_TRACE=1` shows inputs and batches, not the RPC time.

## 7. Scene Shift does nothing or changes to the wrong scene

1. Check `Settings.ProfileUUID` in the editor inspector:
   - `""` or absent → next; `"PreviousScene"` → previous; another value → direct id.
2. A direct id must exist as `<id>.sdProfile\manifest.json` and be a safe id. If it does not exist,
   `run` writes `scene: …` to the console and changes nothing.
3. If the target is the current scene, it is a deliberate no-op.
4. The next/previous order comes from `state.json.order`. If it is empty or does not contain the
   current scene, the corresponding end is used. The editor can reorder; `run` reads the order at
   startup.
5. If you added the key to the scene, restart `run`. If you change the target of a key in the current
   scene, also restart: `run` does not re-read that scene until it enters it again. The target of
   another scene is read when jumping to it.
6. The change paints all affected keys; it may take one batch (~250 ms). Do not test it with Stream
   Dock running.

Status: implemented with unit tests, **pending physical testing** with the keyboard
([ROADMAP](../project/ROADMAP.md)).

## 8. The quota looks dimmed or shows dashes

- `quota-status.json` is older than **10 minutes** or does not exist: the key dims or stays empty if
  there is no data.
- `QUOTA_DIR` must point to the `quota-monitor` folder. Without it, `run` reads
  `%APPDATA%\OrcaDeck\quota` and the key stays empty. Define it before starting `run` (or in the
  autostart task).
- `orca-dock-rs` must be running to refresh the file. The quota key writes `.wake`; `orca-dock-rs`
  checks it every 5 s and refreshes. `run` re-reads every 3 s for up to 45 s.
- The app is read-only: it does not invent or correct figures; if the file is wrong, the problem is
  in the producer.

## 9. `hidwatch` shows `EEEF:2929` (bootloader)

- `EEEF:2929` is the AKP153E bootloader; `0300:1010` is the normal app.
- If the bootloader appears: the device is in update mode. **Do not run `FirmwareUpgradeTool.exe`
  and do not send `APP NEW`.** Unplug and replug the USB; if it stays like that, there is no
  documented recovery without hardware intervention.
- `hidwatch` only enumerates: it does not open the device, write or flash. It is the permitted
  instrument for watching the transition.
- Context and risk of the patch: [`../research/firmware/patch-emulation.md`](../research/firmware/patch-emulation.md).
  Hard rule: do not flash.

## 10. Build failures

| Message | Cause / fix |
|---|---|
| `linker 'link.exe' not found` or `lld-link not found` | llvm-mingw is not on `PATH`; add `C:\Tools\llvm-mingw-20261104-ucrt-x86_64\bin` |
| `toolchain 'stable-x86_64-pc-windows-gnullvm' is not installed` | use `RUSTUP_TOOLCHAIN=gnullvm-local` on this machine; do not edit `rust-toolchain.toml` |
| `error: failed to run custom build command` in `hidapi` | the llvm-mingw C compiler or `PATH` is missing; check the previous step |
| `cargo test` cannot find `node` | the `real_plugin` test is skipped; it is not a failure |
| `real_folders` tests skipped | there are no real Stream Dock profiles; this is expected |
| `-1073741515` when running tests or the binary | `libunwind.dll` is missing; see §1 |

Commands:

```powershell
cd daemon
$env:PATH="C:\Tools\llvm-mingw-20261104-ucrt-x86_64\bin;$env:USERPROFILE\.cargo\bin;$env:PATH"
$env:RUSTUP_HOME="%USERPROFILE%\.rustup"
$env:CARGO_HOME="%USERPROFILE%\.cargo"
$env:RUSTUP_TOOLCHAIN="gnullvm-local"
cargo test --release --workspace
```

## 11. The editor does not open or returns 403

- `edit` prints `OrcaDeck editor: http://127.0.0.1:<port>/?token=…`. Open that exact URL; the token
  changes on every start.
- `--no-open` only avoids launching the browser; the server keeps listening.
- If the port passed with `--port` is busy, choose another or use the default free port.
- A 403 is usually token/Host/Origin: use the freshly printed URL, not an old tab.
- The server listens only on `127.0.0.1`; it is not reachable from another machine.

## 12. Editor changes do not appear in `run`

With `run` alive, `edit` runs inside `run` and saving a cell hot-reloads that key. If `edit` started
standalone (or the `ctl` channel is not available), there is no hot reload:

- Restart `run`.
- Change to another scene and back (`SceneSet` reloads the target manifest).
- For plugin settings, `run` writes them itself with a 1 s debounce; the editor and `run` can
  overwrite each other if you edit the same cell at the same time. Avoid it.

## 13. A plugin paints nothing

1. Is the plugin in the scene? At startup, only plugins used by the active scene are launched. Also,
   `run` only discovers `%APPDATA%\HotSpot\StreamDock\plugins` (or `ORCADECK_PLUGINS`); plugins in
   `Program Files` are listed in `edit` but not launched.
2. Does the action UUID start with the plugin id (`<plugin>.`)? If not, `place_scene` writes
   `no handler for …`.
3. Is the manifest encrypted with no overlay? **Plugins** will show `needs-manifest`. Install it with
   a vendor manifest or use a reconstructible package.
4. Is it `.html`? It is not executed; the host only launches `.exe` and `.js`/`.mjs`/`.cjs` with
   `node`.
5. Is `node` missing from `PATH`? The launch fails and the plugin does not appear.
6. Is the process connected? If `node` is missing or the plugin did not start, there will be no
   image; fix the environment and restart `run`.

Plugin `stderr` is discarded (`Stdio::null`). For the plugin lifecycle, `run` only prints
`plugin <id> started` or `plugin <id>: <error>` to the console; the host uses `log`, but the binary
does not install a logger, so those messages are not visible.

## 14. High CPU or memory usage

- The target is 0 % CPU when idle; the HID loop wakes every 30 ms and `builtin` every 400 ms, but the
  work per wake is minimal.
- If CPU is high, look first at plugins (especially Node) and the editor open in the browser.
- `edit` sleeps in `accept` when idle; the last recorded measurement is ~7 MB RSS and 0 s CPU.
- Measure before optimizing:
  ```powershell
  Get-Process orcadeck | Select-Object CPU, WorkingSet, Id
  ```
- `run` keeps plugins from previous scenes alive on purpose; if they are too heavy, restart `run`.

## 15. Processes left alive

- When you kill `run` (Ctrl+C, closing the console, `Stop-Process`), Windows does not automatically
  kill the plugin child processes. This is expected: the host does not install an orderly shutdown.
- Check `Get-Process node, SystemMonitor` and kill only the ones you know were OrcaDeck's.
- The autostart script kills `orcadeck` before reinstalling, but not the plugins.

## 16. Diagnostics with tracing

```powershell
$env:ORCADECK_TRACE=1
.\orcadeck.exe run <perfil.sdProfile>
```

Expected output:

- `device ready, N = 512`
- `in …` for each HID input (with `ORCADECK_TRACE`)
- `painted N keys … B in … ms` for each batch
- `device: …; retrying in 2 s` if the USB is lost

If `painted` takes much longer than ~30 ms per key or batches come out very close together, check the
gap and the plugins that paint the most.
