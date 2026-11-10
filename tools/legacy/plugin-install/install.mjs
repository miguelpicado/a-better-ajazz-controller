#!/usr/bin/env node
/**
 * Native Stream Deck plugin installer for Orca Deck.
 *
 * Installs a .streamDeckPlugin (zip) or .sdPlugin folder without the Elgato app.
 * Marketplace manifests encrypted with the ELGATO header are NOT decrypted: the
 * host manifest is reconstructed from the plugin's own plaintext files and
 * written as an overlay under <data>/manifests/<uuid>.json.
 *
 * Usage:
 *   node install.mjs inspect <pluginDir|package> [--json]
 *   node install.mjs install <package> --plugins <dir> [--data <dir>] [--manifest <json>] [--force]
 *   node install.mjs watch <inboxDir> --plugins <dir> [--data <dir>] [--once]
 *   node install.mjs export --data <dir> --out <dir>
 *   node install.mjs import <dir> --data <dir>
 */
import { execFileSync } from 'node:child_process';
import {
  cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync,
  renameSync, rmSync, statSync, watch as fsWatch, writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { basename, dirname, extname, join, relative, resolve, sep } from 'node:path';
import { pathToFileURL } from 'node:url';

const ELGATO_MAGIC = Buffer.from('ELGATO');
const SKIP_DIRS = new Set(['node_modules', '.git', 'logs', 'archive', 'storecache']);

export function readManifestState(pluginDir) {
  const path = join(pluginDir, 'manifest.json');
  if (!existsSync(path)) return { kind: 'missing' };
  const buf = readFileSync(path);
  if (buf.subarray(0, ELGATO_MAGIC.length).equals(ELGATO_MAGIC)) {
    return { kind: 'encrypted', size: buf.length };
  }
  try {
    return { kind: 'plain', manifest: JSON.parse(stripBom(buf.toString('utf8'))) };
  } catch {
    return { kind: 'invalid', size: buf.length };
  }
}

export function reconstructManifest(pluginDir) {
  const dir = resolve(pluginDir);
  const uuid = basename(dir).replace(/\.sdPlugin$/i, '');
  const state = readManifestState(dir);
  if (state.kind === 'plain') {
    return { uuid, encrypted: false, confidence: 'plain', manifest: state.manifest };
  }
  if (state.kind === 'missing' || state.kind === 'invalid') {
    return { uuid, encrypted: false, confidence: 'none', manifest: null };
  }

  const locale = loadLocale(dir);
  const actionUuids = collectActionUuids(dir, uuid, locale);
  const codePath = findCodePath(dir);
  const node = /\.(c|m)?js$/i.test(codePath || '');

  const actions = [...actionUuids].sort().map((actionUuid) => {
    const meta = isObject(locale[actionUuid]) ? locale[actionUuid] : {};
    const short = actionUuid === uuid
      ? ''
      : actionUuid.startsWith(`${uuid}.`)
        ? actionUuid.slice(uuid.length + 1)
        : actionUuid.split('.').pop();
    const icon = short ? findIcon(dir, short) : findPluginIcon(dir);
    const pi = short
      ? findPropertyInspector(dir, short) || findPropertyInspector(dir, parentShort(short))
      : findPropertyInspector(dir, 'launcher') || findTopLevelPi(dir);
    const piName = pi
      ? humanize(basename(pi, extname(pi)).replace(/[._-]*(inspector|pi)$/i, ''))
      : '';
    const states = Array.isArray(meta.States) && meta.States.length
      ? meta.States.map((s) => (isObject(s) ? { ...s } : {}))
      : [{}];
    return {
      UUID: actionUuid,
      Name: typeof meta.Name === 'string' && meta.Name
        ? meta.Name
        : piName || humanize(short || uuid.split('.').pop()),
      ...(typeof meta.Tooltip === 'string' && meta.Tooltip ? { Tooltip: meta.Tooltip } : {}),
      ...(icon ? { Icon: icon } : {}),
      ...(pi ? { PropertyInspectorPath: pi } : {}),
      States: states,
      Controllers: /(dial|knob|encoder)/i.test(short) ? ['Keypad', 'Knob'] : ['Keypad'],
    };
  });

  const manifest = {
    UUID: uuid,
    Name: text(locale.Name) || humanize(uuid.split('.').pop()),
    ...(text(locale.Description) ? { Description: text(locale.Description) } : {}),
    ...(text(locale.Author) ? { Author: text(locale.Author) } : {}),
    ...(text(locale.Category) ? { Category: text(locale.Category) } : {}),
    Version: packageVersion(dir) || '1.0.0',
    SDKVersion: 2,
    ...(codePath ? { CodePath: codePath } : {}),
    ...(node ? { Nodejs: { Version: '20' } } : {}),
    ...(findPluginIcon(dir) ? { Icon: findPluginIcon(dir) } : {}),
    ...(findTopLevelPi(dir) ? { PropertyInspectorPath: findTopLevelPi(dir) } : {}),
    Actions: actions,
  };

  const confidence = codePath && actions.length ? 'full'
    : codePath || actions.length ? 'partial'
    : 'none';
  return { uuid, encrypted: true, confidence, manifest, codePath, actionCount: actions.length };
}

function extractPackage(pkg, dest) {
  const entries = execFileSync('tar', ['-tf', pkg], { encoding: 'utf8' }).split(/\r?\n/).filter(Boolean);
  for (const entry of entries) {
    if (entry.startsWith('/') || entry.startsWith('\\') || /^[a-z]:/i.test(entry) || entry.split(/[\\/]/).includes('..')) {
      throw new Error(`Unsafe path in package: ${entry}`);
    }
  }
  execFileSync('tar', ['-xf', pkg, '-C', dest]);
}

export function inspectPackage(packagePath) {
  const pkg = resolve(packagePath);
  if (statSync(pkg).isDirectory()) return reconstructManifest(pkg);
  const tempDir = mkdtempSync(join(tmpdir(), 'orca-sd-inspect-'));
  try {
    extractPackage(pkg, tempDir);
    return reconstructManifest(findPluginDir(tempDir));
  } finally {
    rmSync(tempDir, { recursive: true, force: true });
  }
}

export function installPackage(packagePath, { pluginsRoot, dataRoot, force = false, manifestPath }) {
  const pkg = resolve(packagePath);
  const plugins = resolve(pluginsRoot);
  const data = resolve(dataRoot || join(plugins, '..'));
  if (!existsSync(pkg)) throw new Error(`Package not found: ${pkg}`);
  const provided = manifestPath
    ? JSON.parse(stripBom(readFileSync(resolve(manifestPath), 'utf8')))
    : null;

  let sourceDir;
  let tempDir;
  if (statSync(pkg).isDirectory()) {
    sourceDir = pkg;
  } else {
    tempDir = mkdtempSync(join(tmpdir(), 'orca-sd-install-'));
    extractPackage(pkg, tempDir);
    sourceDir = findPluginDir(tempDir);
  }

  try {
    const uuid = basename(sourceDir).replace(/\.sdPlugin$/i, '');
    const dest = join(plugins, `${uuid}.sdPlugin`);
    if (resolve(sourceDir) === resolve(dest)) throw new Error('Source and destination are the same folder');
    mkdirSync(plugins, { recursive: true });
    if (existsSync(dest)) {
      if (!force) throw new Error(`${dest} already exists (use --force to replace)`);
      renameSync(dest, `${dest}.bak-${new Date().toISOString().replace(/[:.]/g, '-')}`);
    }
    cpSync(sourceDir, dest, { recursive: true });

    const result = reconstructManifest(dest);
    let overlay = null;
    if (provided) {
      mkdirSync(join(data, 'manifests'), { recursive: true });
      overlay = join(data, 'manifests', `${uuid}.json`);
      writeFileSync(overlay, JSON.stringify(provided, null, 2) + '\n');
      return { uuid, dest, overlay, encrypted: true, confidence: 'provided', manifest: provided, actionCount: (provided.Actions || []).length };
    }
    if (result.encrypted && result.confidence === 'full') {
      mkdirSync(join(data, 'manifests'), { recursive: true });
      overlay = join(data, 'manifests', `${uuid}.json`);
      writeFileSync(overlay, JSON.stringify(result.manifest, null, 2) + '\n');
    }
    return { uuid, dest, overlay, needsManifest: result.encrypted && result.confidence !== 'full', ...result };
  } finally {
    if (tempDir) rmSync(tempDir, { recursive: true, force: true });
  }
}

export function watchPackages(dir, { once = false, ...options }) {
  const inbox = resolve(dir);
  if (!existsSync(inbox)) throw new Error(`Watch folder not found: ${inbox}`);
  const seen = new Map();

  const install = (file) => {
    try {
      const stat = statSync(file);
      const prev = seen.get(file);
      if (prev && prev.mtimeMs === stat.mtimeMs && prev.size === stat.size) return;
      const result = installPackage(file, { ...options, force: true });
      seen.set(file, { mtimeMs: stat.mtimeMs, size: stat.size });
      console.log(`Installed ${result.uuid} -> ${result.dest}`);
      if (result.overlay) console.log(`  manifest overlay: ${result.overlay} (${result.confidence})`);
      else if (result.needsManifest) console.log('  encrypted manifest needs --manifest <json>');
    } catch (error) {
      console.error(`${file}: ${error.message}`);
    }
  };

  const scan = () => {
    for (const name of readdirSync(inbox)) {
      if (name.toLowerCase().endsWith('.streamdeckplugin')) install(join(inbox, name));
    }
  };

  scan();
  if (once) return;
  console.log(`Watching ${inbox} for .streamDeckPlugin files...`);
  fsWatch(inbox, (_event, filename) => {
    if (!filename || !filename.toLowerCase().endsWith('.streamdeckplugin')) return;
    const file = join(inbox, filename);
    // Wait until the copy/download stops growing before reading the zip.
    let lastSize = -1;
    let stable = 0;
    const timer = setInterval(() => {
      try {
        const size = statSync(file).size;
        if (size === lastSize) stable += 1;
        else {
          lastSize = size;
          stable = 0;
        }
        if (stable >= 2) {
          clearInterval(timer);
          install(file);
        }
      } catch {
        clearInterval(timer);
      }
    }, 500);
  });
}

export function exportOverlays(dataRoot, outDir) {
  const src = join(resolve(dataRoot), 'manifests');
  if (!existsSync(src)) throw new Error(`No overlays found in ${src}`);
  const out = resolve(outDir);
  mkdirSync(out, { recursive: true });
  let count = 0;
  for (const name of readdirSync(src)) {
    if (!name.toLowerCase().endsWith('.json')) continue;
    cpSync(join(src, name), join(out, name));
    count += 1;
  }
  return count;
}

export function importOverlays(inDir, dataRoot) {
  const src = resolve(inDir);
  const dest = join(resolve(dataRoot), 'manifests');
  if (!existsSync(src)) throw new Error(`Overlay folder not found: ${src}`);
  mkdirSync(dest, { recursive: true });
  let count = 0;
  for (const name of readdirSync(src)) {
    if (!name.toLowerCase().endsWith('.json')) continue;
    const manifest = JSON.parse(stripBom(readFileSync(join(src, name), 'utf8')));
    if (!manifest.UUID || !Array.isArray(manifest.Actions)) throw new Error(`Invalid overlay: ${name}`);
    cpSync(join(src, name), join(dest, name));
    count += 1;
  }
  return count;
}

function stripBom(s) {
  return s.charCodeAt(0) === 0xfeff ? s.slice(1) : s;
}

function isObject(v) {
  return v !== null && typeof v === 'object' && !Array.isArray(v);
}

function text(v) {
  return typeof v === 'string' ? v : '';
}

function humanize(s) {
  return s
    .replace(/([a-z0-9])([A-Z])/g, '$1 $2')
    .replace(/[._-]+/g, ' ')
    .replace(/\b\w/g, (c) => c.toUpperCase());
}

function loadLocale(dir) {
  const preferred = ['en.json', 'en-US.json', 'en_GB.json', 'en-GB.json'];
  for (const name of preferred) {
    const path = join(dir, name);
    if (existsSync(path)) return parseJson(path) || {};
  }
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    if (!entry.isFile() || extname(entry.name).toLowerCase() !== '.json') continue;
    if (entry.name === 'manifest.json' || entry.name === 'package.json') continue;
    const parsed = parseJson(join(dir, entry.name));
    if (parsed && Object.keys(parsed).some((k) => k.includes('.'))) return parsed;
  }
  return {};
}

function parseJson(path) {
  try {
    return JSON.parse(stripBom(readFileSync(path, 'utf8')));
  } catch {
    return null;
  }
}

function packageVersion(dir) {
  const pkg = parseJson(join(dir, 'package.json')) || parseJson(join(dir, 'bin', 'package.json'));
  return text(pkg?.version);
}

function collectActionUuids(dir, uuid, locale) {
  const found = new Set();
  const prefix = `${uuid}.`;
  for (const [key, value] of Object.entries(locale)) {
    if (!key.startsWith(prefix) || !isObject(value)) continue;
    if (/\.(png|jpe?g|svg|gif|json|js|html|css)$/i.test(key)) continue;
    if (value.Name || value.Tooltip || value.States) found.add(key);
  }
  for (const value of readRegistryActions(uuid)) found.add(value);
  // Locale and the Stream Dock registry are authoritative when present; code
  // literals are a fallback for plugins neither of them has seen.
  if (found.size) return found;
  const vendor = uuid.split('.').slice(0, 2).join('.');
  const re = new RegExp(`${escapeRegExp(vendor)}\\.[a-z0-9][a-z0-9._-]*`, 'gi');
  for (const text of readCodeText(dir)) {
    for (const match of text.matchAll(re)) {
      const value = match[0].replace(/[._-]+$/, '');
      if (!/\.(png|jpe?g|svg|gif|json|js|html|css|exe|dll|pdb|log|sdplugin)$/i.test(value)) found.add(value);
    }
  }
  return found;
}

function readRegistryActions(uuid) {
  if (process.platform !== 'win32') return [];
  try {
    const out = execFileSync('reg', ['query', `HKCU\\Software\\HotSpot\\StreamDock\\${uuid}.sdPlugin`], {
      encoding: 'utf8',
      windowsHide: true,
      stdio: ['ignore', 'pipe', 'ignore'],
    });
    return out.split(/\r?\n/)
      .map((line) => line.trim().split(/\s{2,}/)[0])
      .filter((name) => /^[a-z0-9]+(\.[a-z0-9-]+){2,}$/i.test(name));
  } catch {
    return [];
  }
}

function readCodeText(dir) {
  const parts = [];
  for (const file of walk(dir)) {
    if (!/\.(c|m)?js$|\.html?$|\.exe$|\.dll$/i.test(file)) continue;
    try {
      if (statSync(file).size > 20 * 1024 * 1024) continue;
      const buf = readFileSync(file);
      if (/\.(exe|dll)$/i.test(file)) {
        parts.push(buf.toString('latin1'));
        parts.push(buf.toString('utf16le'));
      } else {
        parts.push(buf.toString('utf8'));
      }
    } catch {
      // unreadable file: ignore
    }
  }
  return parts;
}

function* walk(dir) {
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    if (entry.isDirectory()) {
      if (SKIP_DIRS.has(entry.name)) continue;
      yield* walk(join(dir, entry.name));
    } else if (entry.isFile()) {
      yield join(dir, entry.name);
    }
  }
}

function findCodePath(dir) {
  const candidates = [
    'bin/plugin.js', 'bin/plugin.mjs', 'bin/plugin.cjs',
    'plugin.js', 'plugin.mjs', 'plugin.cjs',
    'index.html', 'source/main.html', 'main.html',
  ];
  for (const candidate of candidates) {
    if (existsSync(join(dir, candidate))) return candidate;
  }
  const root = readdirSync(dir, { withFileTypes: true });
  const exe = root.find((e) => e.isFile() && /\.exe$/i.test(e.name));
  if (exe) return exe.name;
  for (const entry of root) {
    if (!entry.isDirectory() || SKIP_DIRS.has(entry.name)) continue;
    for (const candidate of ['main.html', 'index.html', 'plugin.js']) {
      if (existsSync(join(dir, entry.name, candidate))) return `${entry.name}/${candidate}`;
    }
  }
  return null;
}

function findIcon(dir, short) {
  const wanted = short.replace(/[._-]/g, '').toLowerCase();
  const last = short.split('.').pop().replace(/[._-]/g, '').toLowerCase();
  const roots = ['imgs/actions', 'images/actions', 'imgs', 'images', 'Images', 'resources/actions', 'resources/assets', 'assets'];
  for (const root of roots) {
    const base = join(dir, root);
    if (!existsSync(base)) continue;
    for (const file of walk(base)) {
      if (!/\.(png|jpe?g|svg|gif)$/i.test(file)) continue;
      const name = basename(file, extname(file)).replace(/[._-]/g, '').toLowerCase();
      if (name === wanted || name === last || name.startsWith(wanted) || name.includes(last)) {
        return relative(dir, file).split(sep).join('/').replace(/\.[^.]+$/, '');
      }
    }
  }
  return null;
}

function findPropertyInspector(dir, short) {
  if (!short) return null;
  const wanted = short.replace(/[._-]/g, '').toLowerCase();
  const last = short.split('.').pop().replace(/[._-]/g, '').toLowerCase();
  const roots = [
    'pi', 'propertyinspector', 'ui',
    'source/actions', 'resources/actions',
    `source/actions/${last}`, `resources/actions/${last}`,
  ];
  let best = null;
  let bestScore = 0;
  for (const root of roots) {
    const base = join(dir, root);
    if (!existsSync(base)) continue;
    for (const file of walk(base)) {
      if (!/\.html?$/i.test(file)) continue;
      const name = basename(file, extname(file)).replace(/[._-]/g, '').toLowerCase();
      const parent = basename(dirname(file)).replace(/[._-]/g, '').toLowerCase();
      let score = 0;
      if (name === wanted || name === last) score = 4;
      else if (name.includes(`action${wanted}`) || name.includes(`action${last}`)) score = 3;
      else if (name.includes(wanted) || name.includes(last)) score = 2;
      if (parent === wanted || parent === last) score = Math.max(score, 3);
      else if (parent.includes(wanted) || parent.includes(last)) score = Math.max(score, 1);
      if (score > bestScore) {
        best = file;
        bestScore = score;
      }
    }
  }
  return best ? relative(dir, best).split(sep).join('/') : null;
}

function parentShort(short) {
  const index = short.lastIndexOf('.');
  return index > 0 ? short.slice(0, index) : null;
}

function findTopLevelPi(dir) {
  for (const candidate of ['pi/index.html', 'propertyinspector/index.html', 'ui/index.html', 'inspector.html']) {
    if (existsSync(join(dir, candidate))) return candidate;
  }
  return null;
}

function findPluginIcon(dir) {
  for (const candidate of ['imgs/plugin', 'images/plugin', 'imgs/pluginIcon', 'images/pluginIcon', 'Images/pluginIcon', 'Images/icon']) {
    const base = join(dir, candidate);
    if (existsSync(base) && statSync(base).isFile()) return candidate;
    if (existsSync(`${base}.png`)) return `${candidate}.png`;
  }
  return null;
}

function findPluginDir(root) {
  const queue = [root];
  while (queue.length) {
    const current = queue.shift();
    for (const entry of readdirSync(current, { withFileTypes: true })) {
      if (!entry.isDirectory()) continue;
      const path = join(current, entry.name);
      if (/\.sdPlugin$/i.test(entry.name) && existsSync(join(path, 'manifest.json'))) return path;
      queue.push(path);
    }
  }
  throw new Error(`No .sdPlugin folder with manifest.json found under ${root}`);
}

function escapeRegExp(s) {
  return s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

function parseArgs(argv) {
  const [command, ...rest] = argv;
  const positional = [];
  const options = { force: false, json: false };
  for (let i = 0; i < rest.length; i++) {
    const arg = rest[i];
    if (arg === '--plugins') options.pluginsRoot = rest[++i];
    else if (arg === '--data') options.dataRoot = rest[++i];
    else if (arg === '--out') options.outDir = rest[++i];
    else if (arg === '--manifest') options.manifestPath = rest[++i];
    else if (arg === '--force') options.force = true;
    else if (arg === '--once') options.once = true;
    else if (arg === '--json') options.json = true;
    else positional.push(arg);
  }
  return { command, positional, options };
}

function main() {
  const { command, positional, options } = parseArgs(process.argv.slice(2));
  if (command === 'inspect' && positional[0]) {
    const result = inspectPackage(positional[0]);
    if (options.json) {
      console.log(JSON.stringify(result, null, 2));
      return;
    }
    console.log(`${result.uuid}: ${result.encrypted ? 'encrypted' : 'plain'} manifest, confidence=${result.confidence}`);
    if (result.manifest) {
      console.log(`  code: ${result.manifest.CodePath || '(none)'}`);
      const actions = result.manifest.Actions || [];
      console.log(`  actions: ${actions.length}`);
      for (const action of actions) console.log(`    - ${action.UUID} (${action.Name})`);
    }
    return;
  }
  if (command === 'install' && positional[0]) {
    if (!options.pluginsRoot) throw new Error('Missing --plugins <dir>');
    const result = installPackage(positional[0], options);
    console.log(`Installed ${result.uuid} -> ${result.dest}`);
    if (result.overlay) console.log(`Manifest overlay -> ${result.overlay} (${result.actionCount} actions, ${result.confidence})`);
    else if (result.needsManifest) console.log('Encrypted manifest needs a manual overlay: re-run with --manifest <json>.');
    return;
  }
  if (command === 'watch' && positional[0]) {
    if (!options.pluginsRoot) throw new Error('Missing --plugins <dir>');
    watchPackages(positional[0], options);
    return;
  }
  if (command === 'export') {
    if (!options.dataRoot || !options.outDir) throw new Error('Missing --data <dir> or --out <dir>');
    const count = exportOverlays(options.dataRoot, options.outDir);
    console.log(`Exported ${count} overlay(s) to ${resolve(options.outDir)}`);
    return;
  }
  if (command === 'import' && positional[0]) {
    if (!options.dataRoot) throw new Error('Missing --data <dir>');
    const count = importOverlays(positional[0], options.dataRoot);
    console.log(`Imported ${count} overlay(s) into ${resolve(options.dataRoot)}`);
    return;
  }
  console.log(readFileSync(new URL(import.meta.url), 'utf8').split('*/')[0].replace(/^#!.*\n\/\*\*?\n?/, '').trim());
  process.exitCode = 1;
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  try {
    main();
  } catch (error) {
    console.error(error.message);
    process.exitCode = 1;
  }
}
