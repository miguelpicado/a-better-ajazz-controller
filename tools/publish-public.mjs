#!/usr/bin/env node
// Republish this private repo to the public mirror with a cleaned history.
// The working repo is never filtered: this clones it to os.tmpdir() first.
// shortcut: the push is never forced and the denylist scans the published tree
// plus all commit messages, not old file versions; upgrade if a secret is ever
// committed and deleted afterwards.
import { execFileSync, spawnSync } from 'node:child_process';
import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const SCRIPT_DIR = dirname(fileURLToPath(import.meta.url));
export const SOURCE_REPO = resolve(SCRIPT_DIR, '..');
export const DEFAULT_REPO = 'miguelpicado/a-better-ajazz-controller';

const BINARY_RE = /\.(bin|exe|dll)$/i;
const GITHUB_NOREPLY_RE = /@users\.noreply\.github\.com$/i;
const ANTHROPIC_NOREPLY = 'noreply@anthropic.com';

// Vendor firmware from the AJAZZ updater: third-party copyright, never public.
export const ALWAYS_DROP = [
  'docs/research/firmware/backup/AKP153E.bin',
  'docs/research/firmware/work/AKP153E_pressrelease_NO_PROBADO.bin',
  'docs/research/firmware/work/AKP153E_pressrelease_v2_NO_PROBADO.bin',
  'docs/research/firmware/work/rwdata.bin',
];

// Deliberate public binaries would go here, by exact path. Empty: drop them all.
export const BINARY_ALLOWLIST = new Set();

export const REQUIRED_FILES = ['LICENSE-MIT', 'LICENSE-APACHE', 'README.md'];

export function binaryPaths(logOutput) {
  const found = new Set();
  for (const line of String(logOutput).split(/\r?\n/)) {
    const path = line.trim();
    if (path && BINARY_RE.test(path)) found.add(path);
  }
  return [...found].sort();
}

export function pathsToDrop(logOutput, allowList = BINARY_ALLOWLIST) {
  const paths = binaryPaths(logOutput).filter((path) => !allowList.has(path));
  return [...new Set([...ALWAYS_DROP, ...paths])].sort();
}

export function isPublishedEmail(email) {
  return GITHUB_NOREPLY_RE.test(email) || email.toLowerCase() === ANTHROPIC_NOREPLY;
}

export function emailsToMap(logOutput) {
  const found = new Set();
  for (const line of String(logOutput).split(/\r?\n/)) {
    const email = line.trim();
    if (email && !isPublishedEmail(email)) found.add(email);
  }
  return [...found].sort();
}

export function loadDenylist(path) {
  if (!existsSync(path)) return [];
  return readFileSync(path, 'utf8')
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter((line) => line && !line.startsWith('#'));
}

export function verify(repoDir, { denylist = [] } = {}) {
  const failures = [];
  const gitIn = (args) => git(['-C', repoDir, ...args]);

  const binaries = binaryPaths(gitIn(['log', '--all', '--name-only', '--format='])).filter(
    (path) => !BINARY_ALLOWLIST.has(path),
  );
  if (binaries.length) {
    failures.push(
      `${binaries.length} binary file(s) remain in history, e.g. ${binaries[0]}`,
    );
  }

  const emails = new Set(gitIn(['log', '--all', '--format=%ae%n%ce']).split(/\r?\n/));
  const foreign = [...emails].filter((email) => email && !isPublishedEmail(email));
  if (foreign.length) {
    failures.push(`${foreign.length} author/committer e-mail(s) are not GitHub noreply`);
  }

  const tracked = gitIn(['ls-files', '-z']).split('\0').filter(Boolean);

  if (denylist.length) {
    const messages = gitIn(['log', '--all', '--format=%B']);
    denylist.forEach((needle, index) => {
      if (messages.includes(needle)) {
        failures.push(`a commit message contains denylist entry #${index + 1}`);
      }
    });
    for (const path of tracked) {
      const bytes = readFileSync(join(repoDir, path));
      if (bytes.includes(0)) continue;
      const text = bytes.toString('utf8');
      denylist.forEach((needle, index) => {
        if (text.includes(needle)) {
          failures.push(`${path} contains denylist entry #${index + 1}`);
        }
      });
    }
  }

  for (const file of REQUIRED_FILES) {
    if (!tracked.includes(file)) failures.push(`required file ${file} is not tracked`);
  }

  return { ok: failures.length === 0, failures };
}

export function publish({
  sourceRepo = SOURCE_REPO,
  repo = DEFAULT_REPO,
  dryRun = false,
  keepTmp = false,
  noreplyEmail,
  denylistPath = join(sourceRepo, 'tools', 'publish-denylist.txt'),
  log = console.log,
} = {}) {
  if (!/^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/.test(repo)) {
    throw new Error(`--repo must look like owner/name, got ${JSON.stringify(repo)}`);
  }
  const dirty = git(['-C', sourceRepo, 'status', '--porcelain']).trim();
  if (dirty) {
    throw new Error(
      `${sourceRepo} has uncommitted changes; commit or stash them first (only committed history is published)`,
    );
  }
  const filterRepo = spawnSync('git', ['filter-repo', '--version'], { encoding: 'utf8' });
  if (filterRepo.status !== 0) {
    throw new Error(
      'git filter-repo is required (https://github.com/newren/git-filter-repo); install it and retry',
    );
  }
  const noreply = noreplyEmail ?? githubNoreply();
  const denylist = loadDenylist(denylistPath);

  const tmpRoot = mkdtempSync(join(tmpdir(), 'orcadeck-publish-'));
  const cloneDir = join(tmpRoot, 'clone');
  log(`source:   ${sourceRepo}`);
  log(`target:   ${repo} (main)`);
  log(`clone:    ${cloneDir}`);
  try {
    git(['clone', '--no-local', '--branch', 'main', sourceRepo, cloneDir]);

    const drop = pathsToDrop(git(['-C', cloneDir, 'log', '--all', '--name-only', '--format=']));
    const emails = emailsToMap(
      git(['-C', cloneDir, 'log', '--all', '--format=%ae%n%ce']),
    );

    log(`drop:     ${drop.length} path(s), vendor firmware and binaries:`);
    for (const path of drop) log(`  ${path}`);

    git([
      '-C',
      cloneDir,
      'filter-repo',
      '--force',
      '--invert-paths',
      ...drop.flatMap((path) => ['--path', path]),
    ]);

    if (emails.length) {
      // The mailmap holds real addresses, so it lives outside the clone.
      const mailmap = join(tmpRoot, 'mailmap');
      writeFileSync(mailmap, emails.map((email) => `<${noreply}> <${email}>\n`).join(''));
      git(['-C', cloneDir, 'filter-repo', '--force', '--mailmap', mailmap]);
      log(`e-mails:  ${emails.length} author/committer e-mail(s) mapped to GitHub noreply`);
    } else {
      log('e-mails:  none to map');
    }

    const check = verify(cloneDir, { denylist });
    if (!check.ok) {
      throw new Error(`verification failed, nothing pushed:\n  - ${check.failures.join('\n  - ')}`);
    }
    log(`checks:   binaries, e-mails, denylist (${denylist.length} string(s)) and required files ok`);

    const commits = Number(git(['-C', cloneDir, 'rev-list', '--count', 'main']).trim());
    log(`commits:  ${commits} on main`);

    if (dryRun) {
      log('dry run:  nothing pushed; re-run without --dry-run to publish');
    } else {
      push(cloneDir, repo);
    }
    return { commits, dropped: drop, emails: emails.length, tmpDir: keepTmp ? tmpRoot : undefined };
  } finally {
    if (!keepTmp) rmSync(tmpRoot, { recursive: true, force: true });
  }
}

function push(cloneDir, repo) {
  const url = `https://github.com/${repo}.git`;
  const result = spawnSync('git', ['-C', cloneDir, 'push', url, 'main:main'], {
    encoding: 'utf8',
    env: { ...process.env, GIT_TERMINAL_PROMPT: '0' },
  });
  if (result.status === 0) {
    console.log(`pushed:   main -> ${url}`);
    return;
  }
  const output = `${result.stdout ?? ''}${result.stderr ?? ''}`.trim();
  if (/non-fast-forward|fetch first|\[rejected\]/i.test(output)) {
    throw new Error(
      `push rejected as non-fast-forward; nothing was forced. Inspect ${repo}, then retry when its main is an ancestor of this history.\n${output}`,
    );
  }
  throw new Error(`push to ${url} failed:\n${output}`);
}

function githubNoreply() {
  const auth = spawnSync('gh', ['auth', 'status'], { encoding: 'utf8' });
  if (auth.status !== 0) throw new Error('gh is not authenticated; run `gh auth login` first');
  const out = execFileSync('gh', ['api', 'user', '--jq', '.id, .login'], { encoding: 'utf8' })
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter(Boolean);
  const [id, login] = out;
  if (!/^\d+$/.test(id ?? '') || !/^[A-Za-z0-9-]+$/.test(login ?? '')) {
    throw new Error('could not read the GitHub id/login from gh');
  }
  return `${id}+${login}@users.noreply.github.com`;
}

function git(args) {
  return execFileSync('git', args, {
    encoding: 'utf8',
    maxBuffer: 256 * 1024 * 1024,
    stdio: ['ignore', 'pipe', 'pipe'],
  });
}

export function parseArgs(argv) {
  const options = { repo: DEFAULT_REPO, dryRun: false, keepTmp: false, help: false };
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === '--dry-run') options.dryRun = true;
    else if (arg === '--keep-tmp') options.keepTmp = true;
    else if (arg === '--repo') options.repo = argv[(index += 1)];
    else if (arg === '--help' || arg === '-h') options.help = true;
    else throw new Error(`unknown argument: ${arg}`);
  }
  if (!options.repo) throw new Error('--repo needs owner/name');
  return options;
}

const HELP = `Usage: node tools/publish-public.mjs [--repo owner/name] [--dry-run] [--keep-tmp]

  --repo owner/name  public mirror (default ${DEFAULT_REPO})
  --dry-run          clone, filter and verify in a temp dir; do not push
  --keep-tmp         keep the temp clone for inspection
`;

function main() {
  const options = parseArgs(process.argv.slice(2));
  if (options.help) {
    console.log(HELP);
    return;
  }
  publish(options);
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  try {
    main();
  } catch (error) {
    console.error(`error: ${error.message}`);
    process.exitCode = 1;
  }
}
