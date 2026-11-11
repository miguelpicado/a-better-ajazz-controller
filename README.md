# A Better AJazz Controller

An extremely light, native replacement for the "Stream Dock AJAZZ" control app, written in Rust
(binary and crates are called **OrcaDeck**: `orcadeck.exe`). Designed for the **AJAZZ AKP153E**, built to
run natively next to **Orca** for agentic workflows with several accounts and many parallel workers.

> Español: [README.es.md](README.es.md)

## Why

The official app is a heavy Qt/CEF/Electron-style stack that sits in memory all day. This project aims for
the same *useful* functionality with almost no footprint:

- **One native Rust executable**: HID driver, Stream Deck plugin host and, only on demand, the web editor.
- **No async runtime, no resident UI, no admin rights.** 0 % CPU when idle, a few MB of RAM, and HID
  traffic sent in batches so the device is never flooded.
- **Native Stream Deck plugin support** (SDK v2, Node-based plugins): install a `.streamDeckPlugin`,
  place it on the deck, configure it with its own Property Inspector.
- **Built for agentic workflows with Orca**: keys that show each main agent's live state (animated
  spinner while working, flashing white ring and a seconds timer when a worker asks something or
  finishes, live worker count), one key per AI account with 5 h / weekly / monthly quota bars, a global
  summary and a clock. Pressing a key focuses that agent's pane in Orca. It is meant for setups with
  multiple accounts and multiple parallel workers.

## Compatibility

**Designed exclusively for, and only tested on, the AJAZZ AKP153E** (15 keys + side panel, firmware v1).
The protocol layer should work with other AJAZZ / Stream Dock devices that speak the same HID protocol,
but nothing outside the AKP153E is verified: expect to adjust key counts, image sizes and quirks.

Windows 10/11 x64 only for now.

## Status

| Piece | State |
|---|---|
| `orcadeck run` (driver + plugin host + built-in keys) | tested live on an AKP153E |
| Built-in keys: agent, quota, clock, summary | working; animations, alert ring and timers tuned offline |
| Web editor `orcadeck edit` (loopback only) | working: scenes, cells, plugins, brightness, Property Inspector |
| Double-click install of `.streamDeckPlugin` / `.sdPlugin` (`orcadeck register`) | implemented; **not yet tested end-to-end on a real machine** |
| Scene Shift (profile rotation) | implemented; **not yet tested on the device** |
| Optional threaded HID reader (`ORCADECK_HID_READER=thread`) | implemented, off by default, **not yet tested on the device** |
| Press/release firmware patch | designed and emulated only; **never flashed** |

Roadmap and open items: [docs/project/ROADMAP.md](docs/project/ROADMAP.md).

## Quick start

```powershell
# 1. Build (see docs/guide/DEVELOPMENT.md for the toolchain)
cd daemon
cargo build --release

# 2. Stop the official app: it shares the HID device and port 18618
schtasks /end /tn StreamDock

# 3. Check the keyboard (read-only)
.\target\release\orcadeck.exe probe

# 4. Run with your profile
.\target\release\orcadeck.exe run "$env:APPDATA\HotSpot\StreamDock\profiles\<ID>.sdProfile"

# 5. Edit scenes in the browser (inside `run` when it is alive)
.\target\release\orcadeck.exe edit
```

Never run the official app and this one at the same time. Full usage and the `ORCADECK_*` variables:
[docs/guide/USAGE.md](docs/guide/USAGE.md). Architecture: [docs/design/ARCHITECTURE.md](docs/design/ARCHITECTURE.md).

## Documentation

- [Usage](docs/guide/USAGE.en.md) — commands, `ORCADECK_*` variables, Scene Shift, plugins, autostart.
- [Troubleshooting](docs/guide/TROUBLESHOOTING.en.md) — common errors and diagnostics.
- [Security policy](SECURITY.md) — supported versions, private reporting, security model.
- [Contributing](CONTRIBUTING.md) — build, test gate, conventions, licensing.
- [Roadmap](docs/project/ROADMAP.md) — known limits and open work.

Developer and design docs are in Spanish for now.

## Firmware (very experimental)

This repository also carries research and a **patcher** for an improved AKP153E firmware (press/release
events, which the stock v1 firmware lacks). Read [docs/research/firmware/README.md](docs/research/firmware/README.md).

- **No vendor firmware is distributed here.** The patcher takes *your own* firmware dump and writes a
  patched copy; AJAZZ's binaries are not included and must not be added to this repository.
- The patch has been **designed and emulated only. It has never been flashed to hardware by the author.**
- **Flashing can permanently brick your device.** Do not flash anything unless you accept that risk and have
  a recovery plan (a second device is the only safe one). Never run the vendor's `FirmwareUpgradeTool.exe`
  with four arguments, and never send `CRT\0\0APP NEW`.
- The safe alternative for press/release is an AKP153E rev. 2 (PID `0x3010`), which emits it natively.

## Disclaimer

This software and the firmware tools are provided **"as is", without warranty of any kind**. You use them
**at your own risk**. The authors are **not responsible for any damage** to your device, computer, data or
accounts, bricked hardware, lost profiles or any other loss, direct or indirect. This is an independent,
unofficial project: it is **not affiliated with, endorsed by or supported by AJAZZ, HotSpot, Elgato or
Orca**; all names and trademarks belong to their owners. Interoperability with the hardware and plugins was
obtained by observing the device's behaviour and analysing the vendor software for compatibility; no vendor
code or firmware is included.

## License

`MIT OR Apache-2.0`, at your option: [LICENSE-MIT](LICENSE-MIT), [LICENSE-APACHE](LICENSE-APACHE).
Third-party notices: [THIRD-PARTY-LICENSES.md](THIRD-PARTY-LICENSES.md).

Provenance: the code is original. From the community projects analysed
([docs/research/community/](docs/research/community/)) only factual protocol data (VID/PID, key map, offsets)
was used; no code was copied from GPL-3.0, MPL-2.0 or unlicensed repositories.
