# Security Policy

## Supported versions

Security fixes target the latest commit on `main` and the latest published release. Older releases
and arbitrary local builds are not supported; upgrade to the latest release or build `main`.

## Reporting a vulnerability

Please do not open a public issue for a security problem.

1. Go to the repository's **Security** tab.
2. Click **Report a vulnerability**.
3. Describe the issue with enough detail to reproduce it.

GitHub private vulnerability reporting is the only channel; this project does not publish an e-mail
address. A useful report includes the affected version, tag or commit, the Windows version, the
device model and firmware if relevant, reproduction steps and the impact you expect.

Do not attach firmware dumps, vendor installers or a live editor URL containing its token.

## Scope

In scope:

- The loopback web editor (`orcadeck edit`), its session token and its API.
- The control named pipe between `edit` and `run` (`\\.\pipe\orcadeck-ctl`).
- The plugin host WebSocket on `127.0.0.1:18618`: Origin checks, Property Inspector tokens and
  message limits.
- The Property Inspector sandbox.
- The plugin install flow (`orcadeck-plugman`, `register`, `install`).

Out of scope:

- The firmware tools under `docs/research/firmware/` and `tools/`. They are experimental and
  unsupported, and the patch has never been flashed by the author; flashing can permanently brick
  the device.
- The behaviour of third-party plugins themselves. They are separate programs; report their bugs to
  their authors. OrcaDeck only controls how they are launched and connected.
- The official Stream Dock app, AJAZZ hardware and firmware, and other vendor software.

## Security model

OrcaDeck's local services are loopback-only: the editor binds `127.0.0.1` and the plugin host binds
`127.0.0.1:18618`, so neither is reachable from another machine. The editor issues a fresh random
session token per start, exchanges it for an `HttpOnly; SameSite=Strict` cookie and requires it on
every `/api/*` call. Property Inspector sessions and staged plugin installs use separate one-use
tokens. The Property Inspector runs in a sandboxed iframe **without** `allow-same-origin` (opaque
origin), with a CSP that allows no external network, and receives its one-use token via `postMessage`,
never the editor token. The control pipe is created with `FILE_FLAG_FIRST_PIPE_INSTANCE`, a DACL that
grants access only to the current user SID and SYSTEM, and `PIPE_REJECT_REMOTE_CLIENTS`; the client
also checks the server process SID. The plugin installer never installs on its own: the package is
inspected, its path is kept behind a one-use token, and the user must press **Install plugin** and
confirm a replacement. WebSocket frames and messages are capped at 4 MiB (`MAX_WS_MESSAGE`).

The details are in [docs/design/ARCHITECTURE.md](docs/design/ARCHITECTURE.md) (§7-§9) and
[docs/design/inspector.md](docs/design/inspector.md); the code is in
[`daemon/src/ctl.rs`](daemon/src/ctl.rs), [`daemon/crates/host/src/host.rs`](daemon/crates/host/src/host.rs)
and [`daemon/crates/web`](daemon/crates/web).
