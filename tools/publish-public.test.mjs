import { test } from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync, spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import {
  ALWAYS_DROP,
  emailsToMap,
  pathsToDrop,
  publish,
  verify,
} from './publish-public.mjs';

const NOREPLY = '1+fixture@users.noreply.github.com';
const FILTER_REPO = spawnSync('git', ['filter-repo', '--version'], { encoding: 'utf8' }).status === 0;

const git = (cwd, args) => execFileSync('git', ['-C', cwd, ...args], { encoding: 'utf8' });

function makeRepo(t, { binary = true, email = 'real-person@example.com', readme = 'fixture repo\n' } = {}) {
  const dir = mkdtempSync(join(tmpdir(), 'publish-public-test-'));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  git(dir, ['init', '-b', 'main', '--quiet']);
  git(dir, ['config', 'user.name', 'Fixture']);
  git(dir, ['config', 'user.email', email]);
  writeFileSync(join(dir, 'README.md'), readme);
  writeFileSync(join(dir, 'LICENSE-MIT'), 'MIT\n');
  writeFileSync(join(dir, 'LICENSE-APACHE'), 'Apache\n');
  git(dir, ['add', '-A']);
  git(dir, ['commit', '-m', 'add docs', '--quiet']);
  if (binary) {
    mkdirSync(join(dir, 'firmware'));
    writeFileSync(join(dir, 'firmware', 'device.bin'), Buffer.from([0, 1, 2]));
    git(dir, ['add', '-A']);
    git(dir, ['commit', '-m', 'add vendor firmware', '--quiet']);
    rmSync(join(dir, 'firmware'), { recursive: true });
    git(dir, ['add', '-A']);
    git(dir, ['commit', '-m', 'drop vendor firmware', '--quiet']);
  }
  return dir;
}

test('pathsToDrop returns every binary path plus the always-dropped firmware', () => {
  const drop = pathsToDrop('docs/notes.md\nfirmware/one.bin\n\nbin/app.exe\nlib/data.DLL\n');
  for (const path of [
    'firmware/one.bin',
    'bin/app.exe',
    'lib/data.DLL',
    ...ALWAYS_DROP,
  ]) {
    assert.ok(drop.includes(path), `${path} should be dropped`);
  }
  assert.ok(!drop.includes('docs/notes.md'));
});

test('pathsToDrop honours the binary allow-list', () => {
  const drop = pathsToDrop('fixtures/small.bin\n', new Set(['fixtures/small.bin']));
  assert.ok(!drop.includes('fixtures/small.bin'));
});

test('emailsToMap keeps noreply addresses out and reports the rest once', () => {
  const mapped = emailsToMap(
    [
      'real@example.com',
      '123+octo@users.noreply.github.com',
      'noreply@anthropic.com',
      'real@example.com',
    ].join('\n'),
  );
  assert.deepEqual(mapped, ['real@example.com']);
});

test('verify rejects a .bin that survived in history', (t) => {
  const repo = makeRepo(t);
  const result = verify(repo);
  assert.equal(result.ok, false);
  assert.ok(
    result.failures.some((failure) => failure.includes('firmware/device.bin')),
    result.failures.join('\n'),
  );
});

test('verify rejects foreign e-mails without printing them', (t) => {
  const repo = makeRepo(t, { binary: false });
  const result = verify(repo);
  assert.equal(result.ok, false);
  assert.ok(result.failures.some((failure) => failure.includes('e-mail')));
  assert.ok(!result.failures.join('\n').includes('real-person@example.com'));
});

test('verify rejects a denylisted string in a tracked file', (t) => {
  const repo = makeRepo(t, { binary: false, email: NOREPLY, readme: 'contains SEKRET in prose\n' });
  const result = verify(repo, { denylist: ['SEKRET'] });
  assert.equal(result.ok, false);
  assert.ok(result.failures.some((failure) => failure.includes('README.md')));
});

test('verify accepts a clean history', (t) => {
  const repo = makeRepo(t, { binary: false, email: NOREPLY });
  const result = verify(repo);
  assert.deepEqual(result.failures, []);
});

test(
  'publish --dry-run drops the .bin and maps the e-mail',
  { skip: FILTER_REPO ? false : 'git filter-repo is not installed' },
  (t) => {
    const repo = makeRepo(t);
    const result = publish({
      sourceRepo: repo,
      dryRun: true,
      keepTmp: true,
      noreplyEmail: NOREPLY,
      log: () => {},
    });
    t.after(() => rmSync(result.tmpDir, { recursive: true, force: true }));

    assert.ok(result.dropped.includes('firmware/device.bin'));
    const clone = join(result.tmpDir, 'clone');
    assert.ok(!/\.bin$/im.test(git(clone, ['log', '--all', '--name-only', '--format='])));
    assert.deepEqual(
      [...new Set(git(clone, ['log', '--all', '--format=%ae%n%ce']).split(/\r?\n/).filter(Boolean))],
      [NOREPLY],
    );
    const check = verify(clone);
    assert.equal(check.ok, true, check.failures.join('\n'));
  },
);
