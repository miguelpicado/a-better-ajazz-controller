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
 * Each window: [name, remaining %, resetsAt ms|null, reset label]. `resets`: Codex resetsRemaining.
 */
const WINDOWS = [['session', '5h', 300], ['weekly', 'sem', 10080], ['monthly', 'mes', 43200]];
const STRIPE = { anthropic: '#d97757', openai: '#10a37f', google: '#4285f4', opencode: '#8b5cf6', antigravity: '#14b8a6' };

export function quotaItems(status) {
  return (status?.items ?? [])
    .map((i) => ({
      key: i.id,
      label: i.shortName ?? i.name ?? i.id,
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
  const name = item.label.toUpperCase();
  const nameTxt = name.length > 11 ? name.slice(0, 10) + '…' : name;
  const tag = main.name.toUpperCase() + (item.resets !== null ? ` ↻${item.resets}` : '');
  const tagW = tag.length * 8 + 10;
  // Line 2: other windows (5h+week, 5h+week+month) or, for a single window, its reset.
  const line2 = rest.length
    ? rest.map((b) => `<tspan fill="${leftColor(clamp(b.left))}">${b.name} ${clamp(b.left)}%</tspan>`).join('<tspan fill="#555"> · </tspan>')
    : `<tspan fill="#9ca3af">${esc(main.when ? `↺ ${main.when}` : '')}</tspan>`;
  const line2Size = rest.length > 1 ? 12 : 16;
  const timeLeft = main.at ? Math.min(1, Math.max(0, (main.at - now) / (main.mins * 60000))) : null;
  const barX = 16, barW = 118, barY = 104;
  const marker = timeLeft === null ? '' : `<rect x="${barX + barW * timeLeft - 2}" y="${barY - 5}" width="4" height="18" rx="2" fill="#fff"/>`;
  const bottom = rest.length ? (main.when ? `↺ ${main.when}` : '') : '';
  return url(
    `<rect width="144" height="144" fill="#000"/><g opacity="${stale ? 0.4 : 1}">` +
      `<rect x="0" y="0" width="6" height="144" fill="${item.stripe}"/>` +
      `<text x="16" y="24" ${MONO} font-size="13" letter-spacing="1" fill="#e5e7eb">${esc(nameTxt)}</text>` +
      `<rect x="${136 - tagW}" y="10" width="${tagW}" height="19" rx="4" fill="#262626"/>` +
      `<text x="${136 - tagW / 2}" y="24" text-anchor="middle" ${MONO} font-size="12" fill="#a3a3a3">${esc(tag)}</text>` +
      `<text x="14" y="76" ${SANS} font-weight="800" font-size="46" fill="#fff">${v}%</text>` +
      `<text x="16" y="96" ${MONO} font-size="${line2Size}">${line2}</text>` +
      `<rect x="${barX}" y="${barY}" width="${barW}" height="8" rx="4" fill="#262626"/>` +
      `<rect x="${barX}" y="${barY}" width="${(barW * v) / 100}" height="8" rx="4" fill="${leftColor(v)}"/>${marker}` +
      `<text x="16" y="134" ${SANS} font-size="14" fill="#9ca3af">${esc(bottom)}</text></g>`,
  );
}

const url = (body) =>
  'data:image/svg+xml;charset=utf8,' +
  encodeURIComponent(`<svg xmlns="http://www.w3.org/2000/svg" width="144" height="144" viewBox="0 0 144 144">${body}</svg>`);
