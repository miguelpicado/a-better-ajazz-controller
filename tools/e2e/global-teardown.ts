import fs from 'node:fs';
import path from 'node:path';
import { spawn } from 'node:child_process';
import { SESSION_FILE } from './global-setup.js';

export default async function globalTeardown() {
  if (!fs.existsSync(SESSION_FILE)) return;

  try {
    const session = JSON.parse(fs.readFileSync(SESSION_FILE, 'utf8'));
    if (session.pid) {
      if (process.platform === 'win32') {
        await new Promise<void>((resolve) => {
          const k = spawn('taskkill', ['/F', '/T', '/PID', String(session.pid)]);
          k.on('close', () => resolve());
          setTimeout(resolve, 2000);
        });
      } else {
        process.kill(session.pid, 'SIGKILL');
      }
    }

    if (session.tempDir && fs.existsSync(session.tempDir)) {
      fs.rmSync(session.tempDir, { recursive: true, force: true });
    }
  } catch {
    // Ignore teardown errors
  } finally {
    try {
      fs.unlinkSync(SESSION_FILE);
    } catch {
      // Ignore
    }
  }
}
