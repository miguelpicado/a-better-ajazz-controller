// Pure logic: which agents are "main", which key shows which agent, and how a key looks.

/** Orca agent state -> key state. waiting = question for you, blocked = error. */
const STATE = { working: 'working', waiting: 'question', blocked: 'error', done: 'done', idle: 'idle' };

/**
 * Main agents: top-level agents (no parent pane) in a root worktree (no parent worktree) whose
 * terminal is alive and is not an orchestration worker.
 */
export function mainAgents(ps, terminals, workers, repos) {
  const term = new Map(terminals.map((t) => [`${t.tabId}:${t.leafId}`, t]));
  const workerTerms = new Set(workers.map((w) => w.terminal).filter(Boolean));
  const repoById = new Map(repos.map((r) => [r.id, r]));
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

/** 144x144 key image as an SVG data URL. `agent` undefined = empty key. */
export function render(agent, frame = 0) {
  if (!agent) {
    return url(`<rect width="144" height="144" fill="#111"/><circle cx="72" cy="72" r="14" fill="none" stroke="#333" stroke-width="3" stroke-dasharray="5 6"/>`);
  }
  const fg = ink(agent.color);
  const label = agent.label.length > 12 ? agent.label.slice(0, 11) + '…' : agent.label;
  const size = label.length <= 7 ? 24 : label.length <= 10 ? 20 : 17;
  return url(
    `<rect width="144" height="144" fill="#000"/><rect x="2" y="2" width="140" height="140" rx="26" fill="${esc(agent.color)}"/>` +
      `<text x="72" y="34" text-anchor="middle" font-family="Segoe UI, Arial, sans-serif" font-weight="700" font-size="${size}" fill="${fg}">${esc(label)}</text>` +
      `<circle cx="72" cy="90" r="38" fill="#000" fill-opacity=".55"/>` +
      `<g transform="translate(46 64) scale(2.17)">${ICON[agent.state](frame)}</g>`,
  );
}

/**
 * Accounts from quota-status.json that have at least one number to show. Numbers are REMAINING %.
 * `resets`: optional `resetsRemaining` count written by quota-monitor (Codex), shown as ↻N.
 */
export function quotaItems(status) {
  const left = (w) => (typeof w?.usedPercent === 'number' ? 100 - w.usedPercent : null);
  return (status?.items ?? [])
    .map((i) => ({
      key: i.id,
      label: i.shortName ?? i.name ?? i.id,
      resets: typeof i.resetsRemaining === 'number' ? i.resetsRemaining : null,
      bars: [['5h', left(i.session)], ['sem', left(i.weekly)], ['mes', left(i.monthly)]].filter(([, v]) => v !== null),
    }))
    .filter((i) => i.bars.length);
}

const leftColor = (p) => (p <= 10 ? '#ef4444' : p <= 30 ? '#f59e0b' : '#22c55e');
const FONT = 'font-family="Segoe UI, Arial, sans-serif"';

/** Quota key, black: name on top, then one row per window sized to fit 1-3 rows. */
export function renderQuota(item, stale = false) {
  if (!item) return render(undefined);
  const label = item.label.length > 13 ? item.label.slice(0, 12) + '…' : item.label;
  const size = label.length <= 8 ? 22 : label.length <= 11 ? 18 : 15;
  const head = item.resets !== null ? `${label} ↻${item.resets}` : label;
  const n = item.bars.length;
  const rowH = 100 / n; // rows share y 40..140
  const big = n === 1 ? 46 : n === 2 ? 34 : 24;
  const barH = n === 1 ? 14 : n === 2 ? 9 : 7;
  const rows = item.bars
    .map(([name, p], k) => {
      const base = 40 + (k + 1) * rowH - barH - 6 - (n === 1 ? 18 : 0); // baseline above the bar; one row: centred
      const v = Math.min(100, Math.max(0, Math.round(p)));
      return (
        `<text x="10" y="${base}" ${FONT} font-size="${Math.round(big * 0.5)}" fill="#94a3b8">${name}</text>` +
        `<text x="134" y="${base}" text-anchor="end" ${FONT} font-weight="800" font-size="${big}" fill="${leftColor(v)}">${v}<tspan font-size="${Math.round(big * 0.5)}">%</tspan></text>` +
        `<rect x="10" y="${base + 4}" width="124" height="${barH}" rx="${barH / 2}" fill="#1f2937"/>` +
        `<rect x="10" y="${base + 4}" width="${(124 * v) / 100}" height="${barH}" rx="${barH / 2}" fill="${leftColor(v)}"/>`
      );
    })
    .join('');
  return url(
    `<rect width="144" height="144" fill="#000"/><g opacity="${stale ? 0.4 : 1}">` +
      `<text x="72" y="28" text-anchor="middle" ${FONT} font-weight="700" font-size="${size}" fill="#fff">${esc(head)}</text>${rows}</g>`,
  );
}

const url = (body) =>
  'data:image/svg+xml;charset=utf8,' +
  encodeURIComponent(`<svg xmlns="http://www.w3.org/2000/svg" width="144" height="144" viewBox="0 0 144 144">${body}</svg>`);
