# Smoke test

`smoke.ps1` runs a built `orcadeck.exe` without the device: `help` exits 0 with the usage,
`render` writes 15 valid JPEGs (<= 9000 B) from a synthetic profile, and `edit --port <free>
--no-open` answers 200 on `/` and 401/403 on `/api/` without a token before it is stopped.

It runs in the `test` job of `ci.yml` and the `build` job of `release.yml`:

```powershell
pwsh -File tools/smoke/smoke.ps1 -Exe daemon/target/release/orcadeck.exe
```

Temp files and processes are removed at the end; no HID, no `run`, no port 18618.
