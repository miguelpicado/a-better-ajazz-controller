# Contributing to OrcaDeck

OrcaDeck is a small, Windows-only Rust project for the AJAZZ AKP153E. Contributions are welcome:
bug reports, fixes, documentation and carefully tested hardware work. Keep changes small and focused.

## Before you start

- **Windows 10/11 x64 only.** The code uses the Windows HID API and a named pipe; there is no Linux
  or macOS support. CI runs on `windows-latest`.
- Read the [README](README.md) and the [usage guide](docs/guide/USAGE.en.md) first.
- Build setup, toolchain and local test commands: [docs/guide/DEVELOPMENT.md](docs/guide/DEVELOPMENT.md)
  (in Spanish). The toolchain is pinned by `daemon/rust-toolchain.toml`; do not edit it.
- Design and developer documentation is in Spanish for now. The current architecture is
  [docs/design/ARCHITECTURE.md](docs/design/ARCHITECTURE.md).

## Development gate

Run the gate from `daemon/` before opening a pull request:

```powershell
cd daemon
cargo test --release --workspace
cargo fmt --all --check
```

`cargo test --release --workspace` is the repository gate: it builds the whole workspace (binary,
`host`, `plugman`, `builtin`, `web`) and runs the test suite. The CI workflow
(`.github/workflows/ci.yml`) runs the same tests in its `test` job and `cargo fmt --all --check` in
its `lint` job. Clippy is also reported there, but it is non-blocking.

## Testing without the device

The HID is only needed for `probe`, `scene`, `bright`, `watch`, `poll` and `run`. Without a keyboard:

1. Run the unit and integration tests: `cargo test --release --workspace`.
2. Render a profile offline: `orcadeck render <profile.sdProfile> .\out`. This writes `keyNN.jpg`
   files without opening the HID.
3. Run the editor against copies, never your real profiles, using `ORCADECK_PROFILES`,
   `ORCADECK_PLUGINS` and `ORCADECK_DATA` as shown in
   [docs/guide/DEVELOPMENT.md](docs/guide/DEVELOPMENT.md) (§5).
4. Exercise the installer with `orcadeck-plugman inspect` and a temporary
   `--plugins`/`--data` folder.

Tests that need real Stream Dock data print `skipped` and pass when it is absent. The real paint,
press and scene tests are manual and require the device; do not automate them.

## Hardware-affecting changes

State in the pull request whether the change was tested on a real **AKP153E** and which firmware
version was used. If it was not tested on hardware, say so explicitly. Do not claim hardware
validation you did not perform.

## Conventions

- **Conventional Commits**: `feat:`, `fix:`, `docs:`, `chore:`, `ci:`, `refactor:`, `test:`, with a
  short imperative subject.
- Small pull requests, one concern each.
- No new dependency without a measured need. The project targets a small binary with no async
  runtime and no resident UI.
- Do not lower `ORCADECK_GAP_MS` or reintroduce polling; see the rules in
  [docs/guide/DEVELOPMENT.md](docs/guide/DEVELOPMENT.md) (§6).
- Add a small test for new non-trivial logic.

## Never commit vendor binaries

Firmware dumps, AJAZZ installers and vendor packages (zips, `.rar`, the driver folder) are
third-party material and must never be committed. Keep them under `vendor/` or outside the
repository; `/vendor/`, `*.bin` and the firmware backup folders are gitignored for this reason.
Do not attach firmware dumps or vendor installers to issues or pull requests either.

Firmware work in this repository is analysis only. Never flash a device, never run
`FirmwareUpgradeTool.exe` with four arguments, and never send `CRT\0\0APP NEW`. See the hard rules in
[docs/guide/DEVELOPMENT.md](docs/guide/DEVELOPMENT.md) (§8).

## Licensing and provenance

Contributions are accepted under **MIT OR Apache-2.0**, at your option. By submitting a pull request
you agree that your contribution may be distributed under those terms.

Do not copy code from GPL-3.0, MPL-2.0 or unlicensed projects. From community projects, only factual
protocol data (VID/PID, key maps, offsets) may be used; no code. If you are unsure about a source,
ask before opening the pull request.

## Security

Do not open a public issue for a vulnerability. Follow [SECURITY.md](SECURITY.md) and use GitHub's
private vulnerability reporting.
