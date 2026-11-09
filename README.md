# Orca Deck

Ajazz Stream Dock plugin (Windows). Two key types, both fill themselves in key order:

- **Orca agent**: one main Orca agent per key (top-level agent in a repo's root worktree, not an
  orchestration worker). Repo name, repo badge colour from Orca, state icon: orange spinner =
  working, question bubble = waiting for you, red warning = blocked or terminal down, green check =
  done, grey dot = idle. Press = focus that pane in Orca.
- **AI quota**: one account per key from `quota-monitor/quota-status.json`: 5h, weekly and monthly
  used-% bars. Read-only: `orca-dock-rs` writes the file, so both show the same numbers. Dimmed when the file is older than 10 min.
  Press = re-read.

Data comes from the Orca runtime named pipe (`%APPDATA%/orca/orca-runtime.json`): `worktree.ps`,
`terminal.list`, `repo.list`, `orchestration.workerList`, polled every 1 s; press = `terminal.focus`.

Install: `pwsh ./deploy.ps1`, then drag "Orca agent" / "AI quota" keys from the "Orca Deck" category.
Test: `node com.miguelpicado.orcadock.sdPlugin/test.js`. Log: `plugin.log` in the installed folder.

Icons in `imgs/` come from [1am/orca-agent-deck](https://github.com/1am/orca-agent-deck) (MIT).
