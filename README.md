# Orca Deck

Ajazz Stream Dock plugin (Windows). Two key types, both fill themselves in key order:

- **Orca agent**: one main Orca agent per key (top-level agent in a repo's root worktree, not an
  orchestration worker). Repo name, repo badge colour from Orca, state icon: orange spinner =
  working, question bubble = waiting for you, red warning = blocked or terminal down, green check =
  done, grey dot = idle. Bottom left: time in the current state; bottom right: ⚙N live workers
  of that orchestrator (`orchestration.runList` coordinator). Blinks (white ring) when it turns
  done (20 s) or waiting (60 s); pressing or a state change stops it sooner. Press = focus that pane in Orca.
- **Clock** / **Agent summary**: side-panel tiles (controller `Information` = the button-less
  right strip of the AKP153E, column 5 in the profile). Summary counts every live agent:
  working / waiting / done.
- **AI quota**: one account per key from `quota-monitor/quota-status.json`: 5h, weekly and monthly
  used-% bars. Read-only: `orca-dock-rs` writes the file, so both show the same numbers. Dimmed when the file is older than 10 min.
  Press = force a refresh: writes `quota-monitor/.wake` (orca-dock-rs picks it up within 5 s and
  refetches; its own 2-min timer only runs while Orca is maximized), then re-reads for up to 45 s.

Data comes from the Orca runtime named pipe (`%APPDATA%/orca/orca-runtime.json`): `worktree.ps`,
`terminal.list`, `repo.list`, `orchestration.workerList`, polled every 1 s; press = `terminal.focus`.

Install: `pwsh ./deploy.ps1`, then drag "Orca agent" / "AI quota" keys from the "Orca Deck" category.
Test: `node com.miguelpicado.orcadock.sdPlugin/test.js`. Log: `plugin.log` in the installed folder.

`tools/streamdock/` (one-off setup of the AKP153E, run with Stream Dock stopped):
- `profiles.mjs`: Orca scene arrows + side-panel clock/summary, and the "Grabación" scene (Zoom +
  Insta360 keys). Keys are `"col,row"`; column 5 is the side panel. Backs up each manifest first.
- `arrows.ps1`: draws `ARROW_LEFT.png` / `ARROW_RIGHT.png` (white on black).
- `insta360-manifest.json`: readable manifest for `com.insta360.webcam.sdPlugin`. Elgato ships it
  encrypted and Stream Dock cannot load that; copy this over `manifest.json` in Stream Dock's copy
  of the plugin (keep the original as `manifest.json.elgato`). Redo after updating the plugin.

`tools/plugin-install/` installs `.streamDeckPlugin`/`.sdPlugin` without the Elgato app. It does not
decrypt Marketplace manifests: it rebuilds the host manifest from the plugin's plaintext files and
writes an overlay to `<data>/manifests/<uuid>.json`. See `tools/plugin-install/README.md`; the
integration contract for the app is `docs/design/plugin-install-handoff.md`.

Icons in `imgs/` come from [1am/orca-agent-deck](https://github.com/1am/orca-agent-deck) (MIT).
