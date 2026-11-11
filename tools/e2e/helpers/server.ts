import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import { spawn, type ChildProcess } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import {
  ensureFixturesExist,
  SYNTHETIC_PROFILE_ID,
  SYNTHETIC_PLUGIN_UUID,
} from '../fixtures/generate-fixtures.js';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const REPO_ROOT = path.resolve(__dirname, '../../..');

export function resolveOrkaDeckBin(): string {
  const envBin = process.env.ORCADECK_BIN;
  if (envBin) {
    const resolved = path.isAbsolute(envBin) ? envBin : path.resolve(REPO_ROOT, envBin);
    if (fs.existsSync(resolved)) return resolved;
    throw new Error(`ORCADECK_BIN set to '${envBin}', but binary does not exist at '${resolved}'`);
  }

  const defaultBin = path.join(REPO_ROOT, 'daemon', 'target', 'release', 'orcadeck.exe');
  if (fs.existsSync(defaultBin)) return defaultBin;

  const fallbackBin = path.join(REPO_ROOT, 'daemon', 'target-e2e', 'release', 'orcadeck.exe');
  if (fs.existsSync(fallbackBin)) return fallbackBin;

  throw new Error(
    `orcadeck binary not found at '${defaultBin}' or '${fallbackBin}'. Please build with 'cargo build --release' or set ORCADECK_BIN.`
  );
}

export interface TempFolders {
  tempDir: string;
  profilesDir: string;
  pluginsDir: string;
  dataDir: string;
  profileId: string;
  manifestPath: string;
  cleanup: () => void;
}

export function createTempFolders(tag: string): TempFolders {
  const { profileDir } = ensureFixturesExist();
  const tempDir = fs.mkdtempSync(path.join(os.tmpdir(), `orcadeck-e2e-${tag}-`));
  const profilesDir = path.join(tempDir, 'profiles');
  const pluginsDir = path.join(tempDir, 'plugins');
  const dataDir = path.join(tempDir, 'data');

  fs.mkdirSync(profilesDir, { recursive: true });
  fs.mkdirSync(pluginsDir, { recursive: true });
  fs.mkdirSync(dataDir, { recursive: true });

  // Copy synthetic profile into profiles/
  const destProfileDir = path.join(profilesDir, `${SYNTHETIC_PROFILE_ID}.sdProfile`);
  fs.cpSync(profileDir, destProfileDir, { recursive: true });

  // Seed state.json
  const stateJson = {
    order: [SYNTHETIC_PROFILE_ID],
    current: SYNTHETIC_PROFILE_ID,
    brightness: 70,
  };
  fs.writeFileSync(path.join(dataDir, 'state.json'), JSON.stringify(stateJson, null, 2), 'utf8');

  const manifestPath = path.join(destProfileDir, 'manifest.json');

  const cleanup = () => {
    try {
      fs.rmSync(tempDir, { recursive: true, force: true });
    } catch {
      // Ignore cleanup error on windows locks
    }
  };

  return {
    tempDir,
    profilesDir,
    pluginsDir,
    dataDir,
    profileId: SYNTHETIC_PROFILE_ID,
    manifestPath,
    cleanup,
  };
}

export interface RunningEditor {
  child: ChildProcess;
  url: string;
  port: number;
  token: string;
  stop: () => Promise<void>;
}

export async function spawnEditor(options: {
  profilesDir: string;
  pluginsDir: string;
  dataDir: string;
  port?: number;
  binPath?: string;
}): Promise<RunningEditor> {
  const bin = options.binPath || resolveOrkaDeckBin();
  const port = options.port ?? 0;

  const extraPath = 'C:\\Tools\\llvm-mingw-20261104-ucrt-x86_64\\bin';
  const currentPath = process.env.PATH || '';
  const newPath = currentPath.includes(extraPath) ? currentPath : `${extraPath};${currentPath}`;

  const env: NodeJS.ProcessEnv = {
    ...process.env,
    PATH: newPath,
    ORCADECK_PROFILES: options.profilesDir,
    ORCADECK_PLUGINS: options.pluginsDir,
    ORCADECK_DATA: options.dataDir,
  };

  const child = spawn(bin, ['edit', '--port', String(port), '--no-open'], {
    env,
    stdio: ['pipe', 'pipe', 'pipe'],
    windowsHide: true,
  });

  return new Promise((resolve, reject) => {
    let output = '';
    let resolved = false;

    const timeout = setTimeout(() => {
      if (!resolved) {
        resolved = true;
        child.kill();
        reject(new Error(`Timed out waiting for editor server to start. Output was:\n${output}`));
      }
    }, 15000);

    const onData = (data: Buffer) => {
      output += data.toString();
      const match = output.match(/OrcaDeck editor:\s+(http:\/\/127\.0\.0\.1:(\d+)\/\?token=([0-9a-f]+))/i);
      if (match && !resolved) {
        resolved = true;
        clearTimeout(timeout);
        const url = match[1];
        const serverPort = parseInt(match[2], 10);
        const token = match[3];

        const stop = async () => {
          return new Promise<void>((res) => {
            if (child.killed || child.exitCode !== null) {
              res();
              return;
            }
            child.once('exit', () => res());
            try {
              if (process.platform === 'win32' && child.pid) {
                spawn('taskkill', ['/F', '/T', '/PID', String(child.pid)]);
              } else {
                child.kill('SIGINT');
              }
            } catch {
              res();
            }
            setTimeout(res, 2000);
          });
        };

        resolve({ child, url, port: serverPort, token, stop });
      }
    };

    child.stdout.on('data', onData);
    child.stderr.on('data', (d) => {
      output += d.toString();
    });

    child.once('error', (err) => {
      if (!resolved) {
        resolved = true;
        clearTimeout(timeout);
        reject(err);
      }
    });

    child.once('exit', (code) => {
      if (!resolved) {
        resolved = true;
        clearTimeout(timeout);
        reject(new Error(`Editor process exited early with code ${code}. Output:\n${output}`));
      }
    });
  });
}

export interface RunningInstall {
  child: ChildProcess;
  url: string;
  port: number;
  token: string;
  stagedToken: string;
  stop: () => Promise<void>;
}

export async function spawnInstall(
  packagePath: string,
  options: {
    profilesDir: string;
    pluginsDir: string;
    dataDir: string;
    binPath?: string;
  }
): Promise<RunningInstall> {
  const bin = options.binPath || resolveOrkaDeckBin();

  const extraPath = 'C:\\Tools\\llvm-mingw-20261104-ucrt-x86_64\\bin';
  const currentPath = process.env.PATH || '';
  const newPath = currentPath.includes(extraPath) ? currentPath : `${extraPath};${currentPath}`;

  const env: NodeJS.ProcessEnv = {
    ...process.env,
    PATH: newPath,
    ORCADECK_PROFILES: options.profilesDir,
    ORCADECK_PLUGINS: options.pluginsDir,
    ORCADECK_DATA: options.dataDir,
  };

  const child = spawn(bin, ['install', packagePath], {
    env,
    stdio: ['pipe', 'pipe', 'pipe'],
    windowsHide: true,
  });

  return new Promise((resolve, reject) => {
    let output = '';
    let resolved = false;

    const timeout = setTimeout(() => {
      if (!resolved) {
        resolved = true;
        child.kill();
        reject(new Error(`Timed out waiting for install command. Output was:\n${output}`));
      }
    }, 15000);

    const onData = (data: Buffer) => {
      output += data.toString();
      const match = output.match(
        /http:\/\/127\.0\.0\.1:(\d+)\/\?token=([0-9a-f]+)&install=([0-9a-f]+)/i
      );
      if (match && !resolved) {
        resolved = true;
        clearTimeout(timeout);
        const port = parseInt(match[1], 10);
        const token = match[2];
        const stagedToken = match[3];
        const url = `http://127.0.0.1:${port}/?token=${token}&install=${stagedToken}`;

        const stop = async () => {
          return new Promise<void>((res) => {
            if (child.killed || child.exitCode !== null) {
              res();
              return;
            }
            child.once('exit', () => res());
            try {
              if (process.platform === 'win32' && child.pid) {
                spawn('taskkill', ['/F', '/T', '/PID', String(child.pid)]);
              } else {
                child.kill('SIGINT');
              }
            } catch {
              res();
            }
            setTimeout(res, 2000);
          });
        };

        resolve({ child, url, port, token, stagedToken, stop });
      }
    };

    child.stdout.on('data', onData);
    child.stderr.on('data', (d) => {
      output += d.toString();
    });

    child.once('error', (err) => {
      if (!resolved) {
        resolved = true;
        clearTimeout(timeout);
        reject(err);
      }
    });

    child.once('exit', (code) => {
      if (!resolved) {
        resolved = true;
        clearTimeout(timeout);
        reject(new Error(`Install process exited early with code ${code}. Output:\n${output}`));
      }
    });
  });
}
