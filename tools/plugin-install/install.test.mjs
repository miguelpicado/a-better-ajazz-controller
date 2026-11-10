import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { installPackage, reconstructManifest } from './install.mjs';

const UUID = 'com.example.demo';

function fixture() {
  const root = mkdtempSync(join(tmpdir(), 'orca-sd-test-'));
  const dir = join(root, `${UUID}.sdPlugin`);
  mkdirSync(join(dir, 'bin'), { recursive: true });
  writeFileSync(
    join(dir, 'manifest.json'),
    Buffer.concat([Buffer.from('ELGATO'), Buffer.from([1, 0]), Buffer.alloc(64)]),
  );
  writeFileSync(join(dir, 'en.json'), JSON.stringify({
    Name: 'Demo',
    'com.example.demo.launch': { Name: 'Launch', Tooltip: 'Start it', States: [{ Name: 'Idle' }] },
    'com.example.demo.stop': { Name: 'Stop', States: [{ Name: 'Idle' }] },
  }));
  writeFileSync(join(dir, 'bin', 'plugin.js'), 'const actions = ["com.example.demo.launch", "com.example.demo.stop"];');
  return { root, dir };
}

test('reconstructs an encrypted manifest from locale and code path', () => {
  const { root, dir } = fixture();
  try {
    const result = reconstructManifest(dir);
    assert.equal(result.encrypted, true);
    assert.equal(result.confidence, 'full');
    assert.equal(result.manifest.CodePath, 'bin/plugin.js');
    assert.deepEqual(result.manifest.Actions.map((a) => a.UUID), [
      'com.example.demo.launch',
      'com.example.demo.stop',
    ]);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('falls back to code literals when there is no locale or registry', () => {
  const { root, dir } = fixture();
  try {
    writeFileSync(join(dir, 'en.json'), '{}');
    writeFileSync(
      join(dir, 'bin', 'plugin.js'),
      'const a = "com.example.demo.launch"; const b = "com.example.other.action";',
    );
    const result = reconstructManifest(dir);
    assert.equal(result.confidence, 'full');
    assert.ok(result.manifest.Actions.some((a) => a.UUID === 'com.example.demo.launch'));
    assert.ok(result.manifest.Actions.some((a) => a.UUID === 'com.example.other.action'));
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('leaves a plain manifest untouched', () => {
  const { root, dir } = fixture();
  try {
    const plain = { UUID, CodePath: 'index.html', Actions: [{ UUID: 'com.example.demo.launch', Name: 'Launch' }] };
    writeFileSync(join(dir, 'manifest.json'), JSON.stringify(plain));
    const result = reconstructManifest(dir);
    assert.equal(result.encrypted, false);
    assert.equal(result.confidence, 'plain');
    assert.deepEqual(result.manifest, plain);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('installs a .streamDeckPlugin zip and writes the overlay', () => {
  const { root, dir } = fixture();
  try {
    const zip = join(root, `${UUID}.streamDeckPlugin`);
    execFileSync('tar', ['-a', '-c', '-f', zip, '-C', root, `${UUID}.sdPlugin`]);
    const pluginsRoot = join(root, 'plugins');
    const dataRoot = join(root, 'data');
    const result = installPackage(zip, { pluginsRoot, dataRoot });
    assert.equal(result.uuid, UUID);
    assert.ok(existsSync(join(pluginsRoot, `${UUID}.sdPlugin`, 'bin', 'plugin.js')));
    const overlay = join(dataRoot, 'manifests', `${UUID}.json`);
    assert.equal(result.overlay, overlay);
    assert.equal(JSON.parse(readFileSync(overlay, 'utf8')).Actions.length, 2);
    assert.throws(() => installPackage(zip, { pluginsRoot, dataRoot }), /already exists/);
    installPackage(zip, { pluginsRoot, dataRoot, force: true });
    assert.ok(readdirSync(pluginsRoot).some((name) => name.startsWith(`${UUID}.sdPlugin.bak-`)));
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
