// Orca Deck for Ajazz Stream Dock (or Elgato). Two key types, both fill themselves in key order:
//  - agent: one main Orca agent per key (repo name, repo colour, state icon); press = focus its pane.
//  - quota: one account per key from quota-monitor's quota-status.json (5h / weekly / monthly bars).
// Host runs `node plugin.js -port P -pluginUUID U -registerEvent E -info J`.
import { spawn } from 'node:child_process';
import { createConnection } from 'node:net';
import { randomUUID } from 'node:crypto';
import { appendFileSync, writeFileSync, readFileSync, statSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import WebSocket from 'ws';
import { mainAgents, assign, render, quotaItems, renderQuota, renderClock, agentCounts, renderSummary } from './core.js';

const here = dirname(fileURLToPath(import.meta.url));
const LOG = join(here, 'plugin.log');
writeFileSync(LOG, '');
const log = (m) => appendFileSync(LOG, `${new Date().toISOString()} ${m}\n`);

const args = {};
for (let i = 2; i < process.argv.length; i += 2) args[process.argv[i].replace(/^-/, '')] = process.argv[i + 1];

const QUOTA_DIR = process.env.QUOTA_DIR ?? './quota';
const QUOTA_FILE = join(QUOTA_DIR, 'quota-status.json');
const POLL_MS = 1000;
const SPIN_MS = 400; // Stream Dock drops faster image updates
const QUOTA_MS = 30000;
const QUOTA_STALE_MS = 10 * 60000; // older than this: keys dim (orca-dock-rs not running)

// Orca runtime RPC over its named pipe (~2-25 ms; the orca CLI costs ~700 ms per call).
const RUNTIME_FILE = join(process.env.APPDATA ?? "", "orca/orca-runtime.json");
let meta = null;
function rpcOnce(method, params) {
  meta ??= JSON.parse(readFileSync(RUNTIME_FILE, "utf8"));
  const pipe = meta.transports?.find((t) => t.kind === "named-pipe")?.endpoint;
  if (!pipe) throw new Error("no named pipe in orca-runtime.json");
  return new Promise((res, rej) => {
    const s = createConnection(pipe);
    let buf = "";
    const timer = setTimeout(() => (s.destroy(), rej(new Error(`${method} timeout`))), 5000);
    s.setEncoding("utf8");
    s.on("connect", () => s.write(JSON.stringify({ id: randomUUID(), authToken: meta.authToken, method, params }) + '\n'));
    s.on("data", (c) => {
      buf += c;
      for (let i = buf.indexOf('\n'); i >= 0; i = buf.indexOf('\n')) {
        const f = JSON.parse(buf.slice(0, i));
        buf = buf.slice(i + 1);
        if (f._keepalive) continue;
        clearTimeout(timer);
        s.destroy();
        return f.ok ? res(f.result) : rej(new Error(`${method}: ${f.error?.code} ${f.error?.message ?? ""}`));
      }
    });
    s.on("error", (e) => (clearTimeout(timer), rej(e)));
  });
}
/** One retry with a re-read runtime file (Orca restarted: new pipe and token). */
const rpc = (method, params = {}) => rpcOnce(method, params).catch(() => ((meta = null), rpcOnce(method, params)));

const keys = new Map(); // context -> { kind: 'agent'|'quota'|'clock'|'summary', device, row, col }
const slot = { agent: new Map(), quota: new Map() }; // context -> item key
const items = { agent: new Map(), quota: new Map() }; // item key -> item
// agent key -> blink end (ms). Set when an agent turns waiting/done; cleared on press, next change or timeout.
const blinking = new Map();
const BLINK_MS = { done: 20000, question: 60000 };
const isBlinking = (k) => (blinking.get(k) ?? 0) > Date.now();
let counts = { working: 0, question: 0, done: 0 };
let quotaStale = false;
let repos = [];
let reposAt = 0;
const shown = new Map(); // context -> last image, to skip unchanged sends
let frame = 0;

const ws = new WebSocket(`ws://127.0.0.1:${args.port}`);
const send = (event, context, payload) => ws.readyState === 1 && ws.send(JSON.stringify({ event, context, payload }));

function image(kind, context) {
  if (kind === 'clock') return renderClock();
  if (kind === 'summary') return renderSummary(counts);
  const it = items[kind].get(slot[kind].get(context));
  return kind === 'agent' ? render(it, frame, Date.now(), isBlinking(it?.key)) : renderQuota(it, quotaStale);
}

function paint(context) {
  const kind = keys.get(context)?.kind;
  if (!kind) return;
  const img = image(kind, context);
  if (shown.get(context) === img) return;
  shown.set(context, img);
  send('setImage', context, { image: img, target: 0 });
}

function relayout(kind) {
  if (!slot[kind]) {
    for (const [c, k] of keys) if (k.kind === kind) paint(c);
    return;
  }
  const slots = [...keys.entries()]
    .filter(([, k]) => k.kind === kind)
    .sort(([, a], [, b]) => a.device.localeCompare(b.device) || a.row - b.row || a.col - b.col)
    .map(([c]) => c);
  slot[kind] = assign(slots, [...items[kind].values()], slot[kind]);
  for (const c of slots) paint(c);
}

// workerList is slow (~450 ms) and changes rarely: refresh it (and runs) at most every 10 s.
let workers = { workers: [] };
let runs = { runs: [] };
let workersAt = 0;
function workersPromise() {
  if (Date.now() - workersAt < 10000) return workers;
  workersAt = Date.now();
  rpc('orchestration.runList').then((r) => (runs = r)).catch(() => {});
  return rpc('orchestration.workerList').then((w) => (workers = w)).catch(() => workers);
}

async function pollAgents() {
  try {
    if (Date.now() - reposAt > 30000) {
      repos = (await rpc('repo.list')).repos;
      reposAt = Date.now();
    }
    const [ps, tl, wl] = await Promise.all([
      rpc('worktree.ps'),
      rpc('terminal.list'),
      workersPromise(),
    ]);
    const prev = items.agent;
    const fmt = (m) => [...m.values()].map((x) => `${x.label}=${x.state}${x.workers ? `+${x.workers}w` : ''}`).join(' ');
    const before = fmt(prev);
    items.agent = new Map(mainAgents(ps.worktrees, tl.terminals, wl.workers ?? [], repos, runs.runs ?? []).map((a) => [a.key, a]));
    for (const a of items.agent.values()) {
      const was = prev.get(a.key)?.state;
      if (was === undefined || was === a.state) continue;
      if (BLINK_MS[a.state]) blinking.set(a.key, Date.now() + BLINK_MS[a.state]);
      else blinking.delete(a.key);
    }
    for (const k of blinking.keys()) if (!items.agent.has(k) || !isBlinking(k)) blinking.delete(k);
    counts = agentCounts(ps.worktrees);
    const after = fmt(items.agent);
    if (after !== before) log(`agents: ${after}`);
    relayout('agent');
  } catch (e) {
    log(`agents poll failed: ${e.message}`);
  } finally {
    relayout('clock');
    relayout('summary');
    setTimeout(pollAgents, POLL_MS);
  }
}

function pollQuota() {
  try {
    const age = Date.now() - statSync(QUOTA_FILE).mtimeMs;
    quotaStale = age > QUOTA_STALE_MS;
    items.quota = new Map(quotaItems(JSON.parse(readFileSync(QUOTA_FILE, 'utf8'))).map((q) => [q.key, q]));
  } catch (e) {
    quotaStale = true;
    log(`quota read failed: ${e.message}`);
  }
  // ponytail: read-only; orca-dock-rs owns the file, so both always show the same numbers.
  relayout('quota');
}

// Spinner and blink: repaint only agent keys that are working or blinking.
setInterval(() => {
  frame = (frame + 1) % 8;
  for (const [c, k] of slot.agent) if (items.agent.get(k)?.state === 'working' || blinking.has(k)) paint(c); // has(): one last paint after timeout clears the ring
}, SPIN_MS);

/** Brings Orca to the front: restores it, switches virtual desktop. Long-lived raise.ps1 helper. */
let raiser = null;
function startRaiser() {
  if (!raiser || raiser.exitCode !== null) {
    raiser = spawn('powershell.exe', ['-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass', '-File', join(here, 'raise.ps1')], {
      windowsHide: true,
      stdio: ['pipe', 'pipe', 'pipe'],
    });
    raiser.stdout.on('data', (d) => log(`raise: ${String(d).trim()}`));
    raiser.stderr.on('data', (d) => log(`raise error: ${String(d).trim()}`));
  }
}
function raiseOrca() {
  startRaiser();
  raiser.stdin.write('raise\n');
}

/**
 * Forces a quota refresh: orca-dock-rs checks QUOTA_DIR/.wake every 5 s and refetches every source
 * (its own timer only runs while Orca is maximized). Re-read the file every 3 s for up to 45 s.
 */
let waking = null;
function forceQuota() {
  try {
    writeFileSync(join(QUOTA_DIR, '.wake'), new Date().toISOString());
  } catch (e) {
    log(`quota wake failed: ${e.message}`);
  }
  clearInterval(waking);
  const before = (() => { try { return statSync(QUOTA_FILE).mtimeMs; } catch { return 0; } })();
  let tries = 0;
  waking = setInterval(() => {
    pollQuota();
    let now = 0;
    try { now = statSync(QUOTA_FILE).mtimeMs; } catch {}
    if (now !== before || ++tries >= 15) {
      clearInterval(waking);
      log(now !== before ? 'quota refreshed' : 'quota wake: no new data in 45 s (orca-dock-rs running?)');
    }
  }, 3000);
}

async function press(context) {
  const kind = keys.get(context)?.kind;
  if (kind === 'quota') {
    send('showOk', context);
    return forceQuota();
  }
  if (kind !== 'agent') return;
  const a = items.agent.get(slot.agent.get(context));
  if (!a) return;
  blinking.delete(a.key);
  paint(context);
  const t0 = Date.now();
  try {
    raiseOrca();
    await rpc('terminal.focus', { terminal: a.handle });
    log(`focus ${a.label} ${Date.now() - t0}ms`);
  } catch (e) {
    log(`focus failed ${a.label}: ${e.message}`);
    send('showAlert', context);
  }
}

ws.on('open', () => {
  ws.send(JSON.stringify({ event: args.registerEvent, uuid: args.pluginUUID }));
  log(`registered, runtime=${RUNTIME_FILE}, quota=${QUOTA_FILE}`);
  startRaiser(); // warm up now: PowerShell + Add-Type take ~1.3 s
  pollAgents();
  pollQuota();
  setInterval(pollQuota, QUOTA_MS);
});
ws.on('message', (raw) => {
  const m = JSON.parse(raw);
  const kind = m.action?.split('.').pop(); // agent | quota | clock | summary
  if (m.event === 'willAppear') {
    const { row = 0, column = 0 } = m.payload?.coordinates ?? {};
    keys.set(m.context, { kind, device: m.device ?? '', row, col: column });
    log(`appear ${kind} col=${column} row=${row} controller=${m.payload?.controller ?? '-'}`);
    shown.delete(m.context);
    relayout(kind);
  } else if (m.event === 'willDisappear') {
    keys.delete(m.context);
    slot[kind]?.delete(m.context);
    shown.delete(m.context);
  } else if (m.event === 'keyDown') { // keyDown: Stream Dock delivers keyUp late
    press(m.context);
  }
});
ws.on('close', () => process.exit(0));
ws.on('error', (e) => log(`ws error: ${e.message}`));
