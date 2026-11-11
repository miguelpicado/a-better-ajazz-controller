# OrcaDeck E2E Playwright Suite

End-to-end regression test suite for the OrcaDeck local web editor (`orcadeck edit`), running in headless Chromium via Playwright against completely isolated temporary directories and synthetic fixtures (zero hardware device required, zero vendor assets).

## Scenarios

1. **Editor Boot & Authentication**: Validates that navigating to the editor with a valid one-use token URL issues the session cookie and loads the scenes grid with fixture actions. Also asserts that unauthenticated requests without the session token/cookie are rejected with `401 Unauthorized`.
2. **Cell Inspection & Manifest Mutation**: Selects a cell (`0,0`), updates its title and state image via the editor UI, triggers save, and verifies that the `manifest.json` on disk is updated only for that cell while leaving other cells and top-level fields untouched.
3. **Staged Plugin Installation**: Tests `orcadeck install <package>` staging flow. The CLI starts the editor with a one-use staged token URL (`?install=<token>`), the UI renders plugin details (name, UUID, version), requests explicit user confirmation (`window.confirm`), installs into the isolated plugins folder, and verifies that the staged token cannot be reused a second time.
4. **Plugin Uninstallation**: Selects an installed plugin from the Plugins view, clicks "Quitar plugin", confirms removal, and asserts that the plugin directory is deleted from disk and the state updated.
5. **Property Inspector (PI)**: Explicitly skipped with documented justification: the Property Inspector bridge is only provided by a live `orcadeck run` process (port 18618), which requires physical AKP153E USB HID hardware (`Dock::open`). Standalone `orcadeck edit` leaves `FsStore::inspector` as `None` (HTTP 501 fallback).
6. **Accessibility & Console Smoke**: Asserts the page title (`OrcaDeck`), main landmarks (`<main id="keys-view">`, `<main id="plugins-view">`), and verifies that zero console or unhandled runtime errors occur during happy-path navigation.

## Prerequisites

- Node.js >= 20
- Rust `stable-x86_64-pc-windows-gnullvm` and llvm-mingw toolchain (to build `orcadeck.exe`)

## Running Locally

1. Build `orcadeck.exe` (if not already built):
   ```powershell
   $env:PATH="C:\Tools\llvm-mingw-20261104-ucrt-x86_64\bin;$env:USERPROFILE\.cargo\bin;$env:PATH"
   $env:RUSTUP_TOOLCHAIN="gnullvm-local"
   cd ../../daemon
   cargo build --release
   cd ../tools/e2e
   ```

2. Install npm dependencies and Playwright browser:
   ```bash
   cd tools/e2e
   npm ci
   npx playwright install chromium
   ```

3. Run the test suite:
   ```bash
   npx playwright test
   ```

## Environment Variables

- `ORCADECK_BIN`: Path to `orcadeck.exe` binary. Defaults to `../../daemon/target/release/orcadeck.exe` (or `../../daemon/target-e2e/release/orcadeck.exe`).

## CI Integration

In a GitHub Actions or continuous integration workflow, run:

```bash
cd tools/e2e
npm ci
npx playwright install chromium
npx playwright test
```

Failure artifacts (screenshots and Playwright traces) are written to `tools/e2e/test-results/` (retained on failure).
