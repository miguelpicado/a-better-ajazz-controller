// Pure logic: which agents are "main", which key shows which agent, and how a key looks.

/** Orca agent state -> key state. waiting = question for you, blocked = error. */
const STATE = { working: 'working', waiting: 'question', blocked: 'error', done: 'done', idle: 'idle' };

/**
 * Main agents: top-level agents (no parent pane) in a root worktree (no parent worktree) whose
 * terminal is alive and is not an orchestration worker.
 */
export function mainAgents(ps, terminals, workers, repos, runs = []) {
  const term = new Map(terminals.map((t) => [`${t.tabId}:${t.leafId}`, t]));
  const workerTerms = new Set(workers.map((w) => w.agentTerminalHandle ?? w.terminal).filter(Boolean));
  const repoById = new Map(repos.map((r) => [r.id, r]));
  // Live workers per orchestrator: worker.runId -> run.coordinator_handle.
  const coord = new Map(runs.map((r) => [r.id, r.coordinator_handle]));
  const busy = {};
  for (const w of workers) {
    const h = coord.get(w.runId);
    if (h && (w.projection?.outcome ?? 'in_progress') === 'in_progress') busy[h] = (busy[h] ?? 0) + 1;
  }
  const out = [];
  for (const w of ps) {
    if (w.parentWorktreeId || w.isArchived) continue;
    const repo = repoById.get(w.repoId);
    for (const a of w.agents ?? []) {
      if (a.parentPaneKey) continue;
      const t = term.get(a.paneKey);
      if (!t || workerTerms.has(t.handle)) continue;
      out.push({
        key: a.paneKey,
        handle: t.handle,
        worktreeId: w.worktreeId,
        repo: repo?.displayName ?? w.repo ?? '?',
        color: repo?.badgeColor ?? '#444444',
        state: t.connected === false ? 'error' : (STATE[a.state] ?? 'idle'),
        since: a.stateStartedAt ?? a.mainAgent?.stateStartedAt ?? null,
        workers: busy[t.handle] ?? 0,
      });
    }
  }
  out.sort((x, y) => x.repo.localeCompare(y.repo) || x.key.localeCompare(y.key));
  // Same repo twice or more: number them so the keys can be told apart.
  const count = {};
  for (const a of out) count[a.repo] = (count[a.repo] ?? 0) + 1;
  const seen = {};
  for (const a of out) if (count[a.repo] > 1) a.label = `${a.repo} ${(seen[a.repo] = (seen[a.repo] ?? 0) + 1)}`;
  for (const a of out) a.label ??= a.repo;
  return out;
}

/**
 * Keeps each agent on the key it already has; new agents fill free keys in key order.
 * `slots` is the ordered list of key contexts, `prev` the last Map(context -> agentKey).
 * ponytail: agents beyond the number of keys are not shown; add a "+N" key if that bites.
 */
export function assign(slots, agents, prev) {
  const alive = new Set(agents.map((a) => a.key));
  const next = new Map();
  for (const s of slots) if (alive.has(prev.get(s))) next.set(s, prev.get(s));
  const taken = new Set(next.values());
  const free = agents.filter((a) => !taken.has(a.key));
  for (const s of slots) if (!next.has(s) && free.length) next.set(s, free.shift().key);
  return next;
}

const esc = (s) => s.replace(/[&<>"]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' })[c]);

/** Black or white text, whichever reads better on `hex`. */
function ink(hex) {
  const n = parseInt(hex.replace('#', '').padEnd(6, '0').slice(0, 6), 16);
  const lum = 0.299 * ((n >> 16) & 255) + 0.587 * ((n >> 8) & 255) + 0.114 * (n & 255);
  return lum > 160 ? '#111111' : '#ffffff';
}

// Icons drawn in a 24x24 box (Lucide-style strokes), placed in a dark disc so they read on any colour.
const ICON = {
  working: (frame) =>
    `<circle cx="12" cy="12" r="8" fill="none" stroke="#f97316" stroke-opacity=".25" stroke-width="3"/>` +
    `<path d="M12 4a8 8 0 0 1 8 8" fill="none" stroke="#f97316" stroke-width="3" stroke-linecap="round" transform="rotate(${frame * 45} 12 12)"/>`,
  question: () =>
    `<g fill="none" stroke="#60a5fa" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">` +
    `<path d="M7.9 20A9 9 0 1 0 4 16.1L2 22Z"/><path d="M9.09 9a3 3 0 0 1 5.83 1c0 2-3 3-3 3"/><path d="M12 17h.01"/></g>`,
  done: () =>
    `<circle cx="12" cy="12" r="10" fill="#22c55e"/>` +
    `<path d="M7.5 12.5l3 3 6-6.5" fill="none" stroke="#fff" stroke-width="2.6" stroke-linecap="round" stroke-linejoin="round"/>`,
  error: () =>
    `<g fill="none" stroke="#ef4444" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">` +
    `<path d="m21.73 18-8-14a2 2 0 0 0-3.48 0l-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.73-3"/><path d="M12 9v4"/><path d="M12 17h.01"/></g>`,
  idle: () => `<circle cx="12" cy="12" r="5" fill="#9ca3af"/>`,
};

/** Time in state: 45s, 12m, 1h05, 2d. */
export function ago(ms) {
  const s = Math.max(0, Math.floor(ms / 1000));
  if (s < 60) return `${s}s`;
  if (s < 3600) return `${Math.floor(s / 60)}m`;
  if (s < 86400) return `${Math.floor(s / 3600)}h${String(Math.floor((s % 3600) / 60)).padStart(2, '0')}`;
  return `${Math.floor(s / 86400)}d`;
}

/**
 * 144x144 key image as an SVG data URL. `agent` undefined = empty key.
 * Bottom row: time in the current state (left), live workers badge (right).
 * `blink` true + odd half-second: white ring, to catch the eye after waiting/done.
 */
export function render(agent, frame = 0, now = Date.now(), blink = false) {
  if (!agent) {
    return url(`<rect width="144" height="144" fill="#111"/><circle cx="72" cy="72" r="14" fill="none" stroke="#333" stroke-width="3" stroke-dasharray="5 6"/>`);
  }
  const fg = ink(agent.color);
  const label = agent.label.length > 12 ? agent.label.slice(0, 11) + '…' : agent.label;
  const size = label.length <= 7 ? 24 : label.length <= 10 ? 20 : 17;
  const on = blink && Math.floor(frame / 2) % 2 === 0;
  const time = agent.since ? ago(now - agent.since) : '';
  const badge = agent.workers
    ? `<rect x="${136 - (String(agent.workers).length * 11 + 26)}" y="110" width="${String(agent.workers).length * 11 + 26}" height="26" rx="13" fill="#fff"/>` +
      `<text x="${123 - String(agent.workers).length * 5.5}" y="130" text-anchor="middle" ${SANS} font-weight="800" font-size="19" fill="#111">⚙${agent.workers}</text>`
    : '';
  return url(
    `<rect width="144" height="144" fill="#000"/><rect x="2" y="2" width="140" height="140" rx="26" fill="${esc(agent.color)}"/>` +
      `<text x="72" y="30" text-anchor="middle" ${SANS} font-weight="700" font-size="${size}" fill="${fg}">${esc(label)}</text>` +
      `<circle cx="72" cy="74" r="32" fill="#000" fill-opacity=".55"/>` +
      `<g transform="translate(50.4 52.4) scale(1.8)">${ICON[agent.state](frame)}</g>` +
      (time ? `<text x="12" y="131" ${SANS} font-weight="700" font-size="20" fill="${fg}">${time}</text>` : '') +
      badge +
      (on ? `<rect x="4" y="4" width="136" height="136" rx="24" fill="none" stroke="#fff" stroke-width="7"/>` : ''),
  );
}

/** Side panel tile: HH:MM and short date. */
export function renderClock(d = new Date()) {
  const hm = d.toLocaleTimeString('es-ES', { hour: '2-digit', minute: '2-digit' });
  const day = d.toLocaleDateString('es-ES', { weekday: 'short', day: 'numeric', month: 'short' }).replace(/\./g, '');
  return url(
    `<rect width="144" height="144" fill="#000"/>` +
      `<text x="72" y="82" text-anchor="middle" ${SANS} font-weight="800" font-size="50" fill="#fff">${hm}</text>` +
      `<text x="72" y="118" text-anchor="middle" ${SANS} font-size="22" fill="#9ca3af">${esc(day)}</text>`,
  );
}

/** Counts every live agent (main and workers) by state. */
export function agentCounts(ps) {
  const c = { working: 0, question: 0, done: 0 };
  for (const w of ps) {
    if (w.isArchived) continue;
    for (const a of w.agents ?? []) {
      const s = STATE[a.state];
      if (a.hasLivePty !== false && s in c) c[s]++;
    }
  }
  return c;
}

/** Side panel tile: working / waiting / done, one row each with its state colour. */
export function renderSummary(c) {
  const row = (y, color, label, n) =>
    `<circle cx="22" cy="${y - 8}" r="9" fill="${color}"/>` +
    `<text x="40" y="${y}" ${SANS} font-size="22" fill="#d4d4d4">${label}</text>` +
    `<text x="134" y="${y + 1}" text-anchor="end" ${SANS} font-weight="800" font-size="30" fill="#fff">${n}</text>`;
  return url(
    `<rect width="144" height="144" fill="#000"/>` +
      row(40, '#f97316', 'activos', c.working) +
      row(84, '#60a5fa', 'espera', c.question) +
      row(128, '#22c55e', 'hechos', c.done),
  );
}

/**
 * Accounts from quota-status.json that have at least one number to show. Numbers are REMAINING %.
 * Each window: [name, remaining %, resetsAt ms|null, reset label]. `resets`: Codex resetsRemaining.
 */
const WINDOWS = [['session', '5h', 300], ['weekly', 'sem', 10080], ['monthly', 'mes', 43200]];
// Short key names (the key fits ~10 monospace chars next to the window tag).
const SHORT = { 'claude-claude-anpa': 'ANPA', 'claude-claude-coordinacion': 'COORD', 'agy-gemini': 'GEMINI', 'agy-claude-gpt': 'C&GPT', 'orca-opencodeGo': 'OCODE', 'agy-argon': 'ARGON' };
const STRIPE = { anthropic: '#d97757', openai: '#10a37f', google: '#4285f4', opencode: '#8b5cf6', antigravity: '#14b8a6' };

export function quotaItems(status) {
  return (status?.items ?? [])
    .map((i) => ({
      key: i.id,
      label: SHORT[i.id] ?? i.shortName ?? i.name ?? i.id,
      stripe: STRIPE[i.provider] ?? '#9ca3af',
      resets: typeof i.resetsRemaining === 'number' ? i.resetsRemaining : null,
      bars: WINDOWS.filter(([k]) => typeof i[k]?.usedPercent === 'number').map(([k, name, mins]) => {
        const at = i[k].resetsAt ? Date.parse(i[k].resetsAt) || Number(i[k].resetsAt) : null;
        return { name, left: 100 - i[k].usedPercent, at, mins, when: (i[k].resetsAtFormatted ?? '').replace(',', '').replace(/^-$/, '') };
      }),
    }))
    .filter((i) => i.bars.length);
}

const leftColor = (p) => (p <= 10 ? '#ef4444' : p <= 30 ? '#f59e0b' : '#22c55e');
const SANS = 'font-family="Segoe UI, Arial, sans-serif"';
const MONO = 'font-family="Consolas, Cascadia Mono, monospace"';
const clamp = (v) => Math.min(100, Math.max(0, Math.round(v)));

/**
 * Card key: colour stripe per provider, NAME + window tag, big remaining % of the main window
 * (5h when present), a second line (other windows, or the reset for a single window), a bar with a
 * white marker = time left in the window, and the reset time at the bottom.
 */
export function renderQuota(item, stale = false, now = Date.now()) {
  if (!item) return render(undefined);
  const [main, ...rest] = item.bars;
  const v = clamp(main.left);
  const tag = main.name.toUpperCase() + (item.resets !== null ? ` ↻${item.resets}` : '');
  const tagW = tag.length * 8.5 + 10;
  const name = item.label.toUpperCase();
  const fit = Math.floor((126 - tagW) / 8.5);
  const nameTxt = name.length > fit ? name.slice(0, fit - 1) + '…' : name;
  // Line 2: other windows (5h+week, 5h+week+month) or, for a single window, its reset.
  const line2 = rest.length
    ? rest.map((b) => `<tspan fill="${leftColor(clamp(b.left))}">${b.name} ${clamp(b.left)}%</tspan>`).join(' ')
    : `<tspan fill="#9ca3af">${esc(main.when ? `↺ ${main.when}` : '')}</tspan>`;
  const line2Size = rest.length > 1 ? 13 : 18;
  const timeLeft = main.at ? Math.min(1, Math.max(0, (main.at - now) / (main.mins * 60000))) : null;
  const barX = 16, barW = 118, barY = 104;
  const marker = timeLeft === null ? '' : `<rect x="${barX + barW * timeLeft - 2}" y="${barY - 5}" width="4" height="18" rx="2" fill="#fff"/>`;
  const bottom = rest.length ? (main.when ? `↺ ${main.when}` : '') : '';
  return url(
    `<rect width="144" height="144" fill="#000"/><g opacity="${stale ? 0.4 : 1}">` +
      `<rect x="0" y="0" width="6" height="144" fill="${item.stripe}"/>` +
      `<text x="16" y="24" ${MONO} font-size="15" fill="#e5e7eb">${esc(nameTxt)}</text>` +
      `<rect x="${136 - tagW}" y="9" width="${tagW}" height="21" rx="4" fill="#262626"/>` +
      `<text x="${136 - tagW / 2}" y="24" text-anchor="middle" ${MONO} font-size="14" fill="#a3a3a3">${esc(tag)}</text>` +
      `<text x="14" y="68" ${SANS} font-weight="800" font-size="38" fill="#fff">${v}%</text>` +
      `<text x="16" y="92" ${MONO} font-size="${line2Size}">${line2}</text>` +
      `<rect x="${barX}" y="${barY}" width="${barW}" height="8" rx="4" fill="#262626"/>` +
      `<rect x="${barX}" y="${barY}" width="${(barW * v) / 100}" height="8" rx="4" fill="${leftColor(v)}"/>${marker}` +
      `<text x="16" y="136" ${SANS} font-size="18" fill="#9ca3af">${esc(bottom)}</text></g>`,
  );
}

const url = (body) =>
  'data:image/svg+xml;charset=utf8,' +
  encodeURIComponent(`<svg xmlns="http://www.w3.org/2000/svg" width="144" height="144" viewBox="0 0 144 144">${body}</svg>`);
