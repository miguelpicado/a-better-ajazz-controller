import assert from 'node:assert/strict';
import { mainAgents, assign, render } from './core.js';

const ps = [
  { repoId: 'r1', repo: 'laya', parentWorktreeId: null, agents: [
    { paneKey: 't1:l1', parentPaneKey: null, state: 'working' },
    { paneKey: 't1:l2', parentPaneKey: 't1:l1', state: 'working' }, // sub-agent: out
    { paneKey: 't1:l3', parentPaneKey: null, state: 'waiting' },
  ] },
  { repoId: 'r2', repo: 'anpa', parentWorktreeId: null, agents: [
    { paneKey: 't2:l1', parentPaneKey: null, state: 'blocked' },
    { paneKey: 't2:l9', parentPaneKey: null, state: 'done' }, // no live terminal: out
    { paneKey: 't2:l2', parentPaneKey: null, state: 'done' }, // orchestration worker: out
  ] },
  { repoId: 'r2', repo: 'anpa', parentWorktreeId: 'x', agents: [{ paneKey: 't3:l1', parentPaneKey: null, state: 'done' }] }, // child worktree: out
];
const terms = [
  { tabId: 't1', leafId: 'l1', handle: 'h1' }, { tabId: 't1', leafId: 'l2', handle: 'h2' },
  { tabId: 't1', leafId: 'l3', handle: 'h3' }, { tabId: 't2', leafId: 'l1', handle: 'h4' },
  { tabId: 't2', leafId: 'l2', handle: 'h5' }, { tabId: 't3', leafId: 'l1', handle: 'h6' },
];
const repos = [{ id: 'r1', displayName: 'laya', badgeColor: '#ff0000' }, { id: 'r2', displayName: 'anpa', badgeColor: '#00ff00' }];

const a = mainAgents(ps, terms, [{ terminal: 'h5' }], repos);
assert.deepEqual(a.map((x) => [x.handle, x.label, x.state, x.color]), [
  ['h4', 'anpa', 'error', '#00ff00'],
  ['h1', 'laya 1', 'working', '#ff0000'],
  ['h3', 'laya 2', 'question', '#ff0000'],
]);

// New agents fill free keys in order; existing ones keep their key.
let m = assign(['k1', 'k2'], a, new Map([['k2', 't1:l3']]));
assert.deepEqual([...m].sort(), [['k1', 't2:l1'], ['k2', 't1:l3']]);
// Agent gone -> key freed and refilled.
m = assign(['k1', 'k2'], a.filter((x) => x.key !== 't2:l1'), m);
assert.deepEqual([...m].sort(), [['k1', 't1:l1'], ['k2', 't1:l3']]);

assert.match(render(a[0]), /^data:image\/svg\+xml/);
assert.match(render(undefined), /^data:image\/svg\+xml/);
console.log('ok');

// Quota: accounts with no numbers are dropped; missing windows are skipped.
import { quotaItems, renderQuota } from './core.js';
const q = quotaItems({ items: [
  { id: 'c', shortName: 'Claude', session: { usedPercent: 13 }, weekly: { usedPercent: 50 } },
  { id: 'o', shortName: 'OpenCodeGo', session: { usedPercent: 0 }, weekly: { usedPercent: 0 }, monthly: { usedPercent: 71 } },
  { id: 'x', shortName: 'cursor', session: { usedPercent: null }, weekly: { usedPercent: null } },
] });
assert.deepEqual(q.map((i) => [i.key, i.bars.map((b) => b.name).join('/')]), [['c', '5h/sem'], ['o', '5h/sem/mes']]);
assert.match(renderQuota(q[1], true), /^data:image\/svg\+xml/);
console.log('ok quota');
