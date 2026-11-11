import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import { createTempFolders, spawnEditor, resolveOrkaDeckBin } from './helpers/server.js';
import { ensureFixturesExist } from './fixtures/generate-fixtures.js';

export const SESSION_FILE = path.join(os.tmpdir(), 'orcadeck-e2e-session.json');

export default async function globalSetup() {
  // Ensure fixtures exist
  ensureFixturesExist();

  // Create isolated temp directories
  const tempFolders = createTempFolders('suite');

  // Spawn editor server
  const binPath = resolveOrkaDeckBin();
  const editor = await spawnEditor({
    profilesDir: tempFolders.profilesDir,
    pluginsDir: tempFolders.pluginsDir,
    dataDir: tempFolders.dataDir,
    binPath,
  });

  const sessionData = {
    pid: editor.child.pid,
    url: editor.url,
    port: editor.port,
    token: editor.token,
    tempDir: tempFolders.tempDir,
    profilesDir: tempFolders.profilesDir,
    pluginsDir: tempFolders.pluginsDir,
    dataDir: tempFolders.dataDir,
    profileId: tempFolders.profileId,
    manifestPath: tempFolders.manifestPath,
    binPath,
  };

  fs.writeFileSync(SESSION_FILE, JSON.stringify(sessionData, null, 2), 'utf8');
  process.env.PLAYWRIGHT_ORCADECK_SESSION = SESSION_FILE;
}
