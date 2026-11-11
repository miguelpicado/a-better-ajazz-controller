import { test, expect } from '@playwright/test';
import fs from 'node:fs';
import path from 'node:path';
import { SESSION_FILE } from '../global-setup.js';
import {
  spawnInstall,
  type RunningInstall,
} from '../helpers/server.js';
import {
  ensureFixturesExist,
  SYNTHETIC_PLUGIN_UUID,
} from '../fixtures/generate-fixtures.js';

function getSession() {
  if (!fs.existsSync(SESSION_FILE)) {
    throw new Error(`Session file not found at ${SESSION_FILE}. Did global setup run?`);
  }
  return JSON.parse(fs.readFileSync(SESSION_FILE, 'utf8'));
}

test.describe('OrcaDeck Web Editor E2E Suite', () => {
  test('scenario 1: editor loads with valid token, displays profile grid, rejects unauthenticated API request', async ({
    page,
    playwright,
  }) => {
    const session = getSession();

    // 1. Visit editor with valid token URL (redirects to / with session cookie)
    await page.goto(session.url);

    // Assert top branding and title
    await expect(page.locator('.brand span')).toHaveText('OrcaDeck');

    // Assert scene tabs show the synthetic scene
    const sceneTab = page.locator('#scene-tabs li button');
    await expect(sceneTab).toContainText('Synthetic Profile');

    // Assert keys grid contains fixture actions
    const cell00 = page.locator('.cell[data-key="0,0"]');
    await expect(cell00).toBeVisible();
    await expect(cell00).toContainText('Original Title 1');

    const cell10 = page.locator('.cell[data-key="1,0"]');
    await expect(cell10).toBeVisible();
    await expect(cell10).toContainText('Original Title 2');

    // 2. Assert unauthenticated request is rejected with HTTP 401
    const unauthContext = await playwright.request.newContext();
    const res = await unauthContext.get(`http://127.0.0.1:${session.port}/api/scenes`);
    expect(res.status()).toBe(401);
    await unauthContext.dispose();
  });

  test('scenario 2: select cell, change title and image, save, and verify disk manifest changed only that cell', async ({
    page,
  }) => {
    const session = getSession();

    // Snapshot manifest before mutation
    const manifestPath = session.manifestPath;
    const beforeRaw = fs.readFileSync(manifestPath, 'utf8').replace(/^\uFEFF/, '');
    const before = JSON.parse(beforeRaw);

    await page.goto(session.url);

    // Select cell 0,0
    const cell00 = page.locator('.cell[data-key="0,0"]');
    await cell00.click();
    await expect(cell00).toHaveClass(/sel/);

    // Edit Title and Image in Inspector
    const titleInput = page.locator('#inspector textarea.title-in');
    await expect(titleInput).toBeVisible();
    await titleInput.fill('Updated Title 1');

    const imageInput = page.locator('#inspector label:has-text("Imagen") input');
    await expect(imageInput).toBeVisible();
    await imageInput.fill('updated-alpha.png');

    // Save changes
    const saveBtn = page.locator('#inspector button.primary:has-text("Guardar cambios")');
    await expect(saveBtn).toBeEnabled();
    await saveBtn.click();

    // Wait for save confirmation status
    await expect(page.locator('#status')).toContainText('Cambios guardados');

    // Read updated manifest from disk
    const afterRaw = fs.readFileSync(manifestPath, 'utf8').replace(/^\uFEFF/, '');
    const after = JSON.parse(afterRaw);

    // Verify cell 0,0 was modified as expected
    expect(after.Actions['0,0'].States[0].Title).toBe('Updated Title 1');
    expect(after.Actions['0,0'].States[0].Image).toBe('updated-alpha.png');

    // Verify other cells (e.g. 1,0) remained strictly untouched
    expect(after.Actions['1,0']).toEqual(before.Actions['1,0']);

    // Verify root fields remained identical
    expect(after.Name).toBe(before.Name);
    expect(after.DeviceUUID).toBe(before.DeviceUUID);
    expect(after.Pages).toEqual(before.Pages);
  });

  test('scenario 3: install-plugin page via staged token: shows metadata, requires confirmation, installs to temp dir, token cannot be reused', async ({
    page,
  }) => {
    const session = getSession();
    const { pluginZipPath } = ensureFixturesExist();

    // Stage package via CLI `orcadeck install <path>`
    const installer: RunningInstall = await spawnInstall(pluginZipPath, {
      profilesDir: session.profilesDir,
      pluginsDir: session.pluginsDir,
      dataDir: session.dataDir,
      binPath: session.binPath,
    });

    try {
      // Navigate to staged install URL
      await page.goto(installer.url);

      // Verify metadata displayed on the install page
      const detail = page.locator('#pdetail');
      await expect(detail.locator('h2')).toHaveText('Instalar plugin');
      await expect(detail.locator('h3')).toHaveText('Synthetic Pack');
      await expect(detail).toContainText(SYNTHETIC_PLUGIN_UUID);
      await expect(detail).toContainText('1.2.3');

      // Verify explicit confirmation dialog
      let dialogConfirmed = false;
      page.once('dialog', async (dialog) => {
        dialogConfirmed = true;
        expect(dialog.message()).toContain('Synthetic Pack');
        await dialog.accept();
      });

      // Click install
      const installBtn = detail.locator('button.primary:has-text("Instalar plugin")');
      await expect(installBtn).toBeVisible();
      await installBtn.click();

      // Assert confirmation dialog was prompted and accepted
      expect(dialogConfirmed).toBe(true);

      // Assert status shows installation success
      await expect(page.locator('#pstatus')).toContainText('instalado');

      // Assert plugin files exist in temp plugins folder
      const installedPluginDir = path.join(
        session.pluginsDir,
        `${SYNTHETIC_PLUGIN_UUID}.sdPlugin`
      );
      const installedManifest = path.join(installedPluginDir, 'manifest.json');
      expect(fs.existsSync(installedManifest)).toBe(true);

      const installedManifestData = JSON.parse(fs.readFileSync(installedManifest, 'utf8'));
      expect(installedManifestData.Name).toBe('Synthetic Pack');

      // Assert second use of same staged token fails (consumed on first successful install)
      const secondTry = await page.request.get(
        `http://127.0.0.1:${installer.port}/api/plugins/staged?token=${installer.stagedToken}`
      );
      expect(secondTry.ok()).toBe(false);
      expect([400, 404]).toContain(secondTry.status());
    } finally {
      await installer.stop();
    }
  });

  test('scenario 4: remove plugin from UI and assert it is deleted from the plugins folder', async ({
    page,
  }) => {
    const session = getSession();
    const installedPluginDir = path.join(
      session.pluginsDir,
      `${SYNTHETIC_PLUGIN_UUID}.sdPlugin`
    );

    // Ensure plugin exists and is tracked in state.json as installed by OrcaDeck
    if (!fs.existsSync(installedPluginDir)) {
      const { pluginDir } = ensureFixturesExist();
      fs.cpSync(pluginDir, installedPluginDir, { recursive: true });
    }
    const stateFile = path.join(session.dataDir, 'state.json');
    const state = JSON.parse(fs.readFileSync(stateFile, 'utf8'));
    state.installed = state.installed || [];
    if (!state.installed.includes(SYNTHETIC_PLUGIN_UUID)) {
      state.installed.push(SYNTHETIC_PLUGIN_UUID);
      fs.writeFileSync(stateFile, JSON.stringify(state, null, 2), 'utf8');
    }

    await page.goto(session.url);

    // Switch to Plugins view
    const pluginsTab = page.locator('button[data-view="plugins"]');
    await pluginsTab.click();

    // Select the synthetic plugin from the list
    const pluginItem = page.locator('#plist button', { hasText: 'Synthetic Pack' });
    await expect(pluginItem).toBeVisible();
    await pluginItem.click();

    // Locate "Quitar plugin" danger button
    const removeBtn = page.locator('#pdetail button.danger:has-text("Quitar plugin")');
    await expect(removeBtn).toBeVisible();

    // Verify explicit confirmation dialog
    let removeConfirmed = false;
    page.once('dialog', async (dialog) => {
      removeConfirmed = true;
      expect(dialog.message()).toContain('Quitar');
      await dialog.accept();
    });

    await removeBtn.click();
    expect(removeConfirmed).toBe(true);

    // Assert status message
    await expect(page.locator('#pstatus')).toContainText('quitado');

    // Verify plugin directory is deleted from disk
    expect(fs.existsSync(installedPluginDir)).toBe(false);
  });

  test('scenario 5: property inspector persists setting through SDK socket', async () => {
    // Explicitly justified skip per prompt requirements:
    // Standalone `orcadeck edit` leaves FsStore::inspector at None (responding with HTTP 501
    // "el daemon no está en marcha: no hay inspector en vivo"). The live Property Inspector
    // host bridge is hosted by `orcadeck run` on port 18618. However, `orcadeck run` cannot
    // run without the physical AKP153E USB HID hardware device (Dock::open fails immediately).
    test.skip(
      true,
      'Property Inspector requires a running `orcadeck run` daemon to host the SDK socket on port 18618. Standalone `orcadeck edit` leaves FsStore::inspector at None (HTTP 501), and `orcadeck run` requires physical AKP153E HID hardware (Dock::open fails without device).'
    );
  });

  test('scenario 6: basic accessibility smoke: title, main landmark, zero console errors', async ({
    page,
  }) => {
    const session = getSession();
    const consoleErrors: string[] = [];

    page.on('console', (msg) => {
      if (msg.type() === 'error') {
        consoleErrors.push(msg.text());
      }
    });

    page.on('pageerror', (err) => {
      consoleErrors.push(err.message);
    });

    await page.goto(session.url);

    // Check page title
    await expect(page).toHaveTitle('OrcaDeck');

    // Check main landmark
    const mainKeys = page.locator('main#keys-view');
    await expect(mainKeys).toBeVisible();

    // Check navigation to plugins view landmark
    await page.locator('button[data-view="plugins"]').click();
    const mainPlugins = page.locator('main#plugins-view');
    await expect(mainPlugins).toBeVisible();

    // Check navigation back
    await page.locator('button[data-view="keys"]').click();
    await expect(mainKeys).toBeVisible();

    // Assert no console errors occurred during the happy path
    expect(consoleErrors).toEqual([]);
  });
});
