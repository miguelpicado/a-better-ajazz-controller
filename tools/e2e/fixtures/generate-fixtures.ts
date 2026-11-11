import fs from 'node:fs';
import path from 'node:path';
import zlib from 'node:zlib';
import { fileURLToPath } from 'node:url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

// 67-byte valid 1x1 RGBA PNG generated purely in code (no vendor asset)
export const SYNTHETIC_PNG_BYTES = Buffer.from(
  '89504e470d0a1a0a0000000d49484452000000010000000108060000001f15c4890000000a49444154789c63600000000200014e6c0c0000000049454e44ae426082',
  'hex'
);

export interface ZipEntry {
  name: string;
  data: Buffer | string;
}

/**
 * Creates a standard uncompressed (STORE) ZIP archive without external dependencies.
 */
export function createZipBuffer(entries: ZipEntry[]): Buffer {
  const localHeaders: Buffer[] = [];
  const centralHeaders: Buffer[] = [];
  let offset = 0;

  for (const entry of entries) {
    const nameBuf = Buffer.from(entry.name.replace(/\\/g, '/'), 'utf8');
    const dataBuf = Buffer.isBuffer(entry.data) ? entry.data : Buffer.from(entry.data, 'utf8');
    const crc = zlib.crc32(dataBuf);

    // Local file header (30 bytes + name + data)
    const local = Buffer.alloc(30 + nameBuf.length + dataBuf.length);
    local.writeUInt32LE(0x04034b50, 0); // signature
    local.writeUInt16LE(20, 4); // min version 2.0
    local.writeUInt16LE(0, 6); // general flags
    local.writeUInt16LE(0, 8); // compression: 0 = STORE
    local.writeUInt16LE(0, 10); // mod time
    local.writeUInt16LE(0, 12); // mod date
    local.writeUInt32LE(crc, 14); // CRC-32
    local.writeUInt32LE(dataBuf.length, 18); // compressed size
    local.writeUInt32LE(dataBuf.length, 22); // uncompressed size
    local.writeUInt16LE(nameBuf.length, 26);
    local.writeUInt16LE(0, 28); // extra field length
    nameBuf.copy(local, 30);
    dataBuf.copy(local, 30 + nameBuf.length);
    localHeaders.push(local);

    // Central directory header (46 bytes + name)
    const central = Buffer.alloc(46 + nameBuf.length);
    central.writeUInt32LE(0x02014b50, 0); // signature
    central.writeUInt16LE(20, 4); // version made by
    central.writeUInt16LE(20, 6); // version needed
    central.writeUInt16LE(0, 8); // flags
    central.writeUInt16LE(0, 10); // compression 0
    central.writeUInt16LE(0, 12); // mod time
    central.writeUInt16LE(0, 14); // mod date
    central.writeUInt32LE(crc, 16); // CRC-32
    central.writeUInt32LE(dataBuf.length, 20); // comp size
    central.writeUInt32LE(dataBuf.length, 24); // uncomp size
    central.writeUInt16LE(nameBuf.length, 28);
    central.writeUInt16LE(0, 30); // extra len
    central.writeUInt16LE(0, 32); // comment len
    central.writeUInt16LE(0, 34); // disk num
    central.writeUInt16LE(0, 36); // internal attr
    central.writeUInt32LE(0, 38); // external attr
    central.writeUInt32LE(offset, 42); // relative local header offset
    nameBuf.copy(central, 46);
    centralHeaders.push(central);

    offset += local.length;
  }

  const centralOffset = offset;
  const centralSize = centralHeaders.reduce((sum, h) => sum + h.length, 0);

  // End of central directory record (22 bytes)
  const eocd = Buffer.alloc(22);
  eocd.writeUInt32LE(0x06054b50, 0); // signature
  eocd.writeUInt16LE(0, 4); // disk num
  eocd.writeUInt16LE(0, 6); // start disk
  eocd.writeUInt16LE(entries.length, 8); // total entries on disk
  eocd.writeUInt16LE(entries.length, 10); // total entries
  eocd.writeUInt32LE(centralSize, 12); // central directory size
  eocd.writeUInt32LE(centralOffset, 16); // offset of central directory
  eocd.writeUInt16LE(0, 20); // zip comment length

  return Buffer.concat([...localHeaders, ...centralHeaders, eocd]);
}

export function createZipFromDirectory(sourceDir: string, destZipPath: string): void {
  const entries: ZipEntry[] = [];
  const baseName = path.basename(sourceDir);

  function walk(currentDir: string, relativePath: string) {
    const files = fs.readdirSync(currentDir, { withFileTypes: true });
    for (const f of files) {
      const fullPath = path.join(currentDir, f.name);
      const entryRelPath = relativePath ? `${relativePath}/${f.name}` : f.name;
      if (f.isDirectory()) {
        walk(fullPath, entryRelPath);
      } else {
        entries.push({
          name: `${baseName}/${entryRelPath}`,
          data: fs.readFileSync(fullPath),
        });
      }
    }
  }

  walk(sourceDir, '');
  const zipBuffer = createZipBuffer(entries);
  fs.mkdirSync(path.dirname(destZipPath), { recursive: true });
  fs.writeFileSync(destZipPath, zipBuffer);
}

export const SYNTHETIC_PROFILE_ID = 'SYNTHETIC01-0000-0000-0000-000000000001';
export const SYNTHETIC_PLUGIN_UUID = 'com.synthetic.pack';

export function ensureFixturesExist(): {
  fixturesDir: string;
  profileDir: string;
  pluginDir: string;
  pluginZipPath: string;
} {
  const fixturesDir = path.resolve(__dirname);
  const profileDir = path.join(fixturesDir, 'profiles', `${SYNTHETIC_PROFILE_ID}.sdProfile`);
  const profileImagesDir = path.join(profileDir, 'Images');
  const pluginDir = path.join(fixturesDir, 'plugins', `${SYNTHETIC_PLUGIN_UUID}.sdPlugin`);
  const pluginImgsDir = path.join(pluginDir, 'imgs');
  const pluginZipPath = path.join(fixturesDir, `${SYNTHETIC_PLUGIN_UUID}.streamDeckPlugin`);

  // 1. Synthetic Profile
  fs.mkdirSync(profileImagesDir, { recursive: true });
  fs.writeFileSync(path.join(profileImagesDir, 'alpha.png'), SYNTHETIC_PNG_BYTES);
  fs.writeFileSync(path.join(profileImagesDir, 'beta.png'), SYNTHETIC_PNG_BYTES);
  fs.writeFileSync(path.join(profileImagesDir, 'updated-alpha.png'), SYNTHETIC_PNG_BYTES);

  const profileManifest = {
    Name: 'Synthetic Profile',
    Version: '1.0',
    DeviceUUID: 'AKP153E',
    DeviceModel: '20GBA9901',
    DeviceSerialNumber: '1234567890',
    AppIdentifier0: '*',
    Pages: {
      Current: `${SYNTHETIC_PROFILE_ID}.sdProfile`,
      Pages: [`${SYNTHETIC_PROFILE_ID}.sdProfile`],
    },
    Actions: {
      '0,0': {
        ActionID: 'action-0-0',
        Controller: '',
        Name: 'Synthetic Action 1',
        Settings: { volume: 50 },
        State: 0,
        States: [
          {
            Image: 'alpha.png',
            Title: 'Original Title 1',
          },
        ],
        UUID: 'com.synthetic.fixture.action1',
      },
      '1,0': {
        ActionID: 'action-1-0',
        Controller: '',
        Name: 'Synthetic Action 2',
        Settings: { mode: 'on' },
        State: 0,
        States: [
          {
            Image: 'beta.png',
            Title: 'Original Title 2',
          },
        ],
        UUID: 'com.synthetic.fixture.action2',
      },
    },
  };
  fs.writeFileSync(
    path.join(profileDir, 'manifest.json'),
    JSON.stringify(profileManifest, null, 2),
    'utf8'
  );

  // 2. Synthetic Plugin (.sdPlugin)
  fs.mkdirSync(pluginImgsDir, { recursive: true });
  fs.writeFileSync(path.join(pluginImgsDir, 'icon.png'), SYNTHETIC_PNG_BYTES);
  fs.writeFileSync(path.join(pluginImgsDir, 'action.png'), SYNTHETIC_PNG_BYTES);

  const pluginManifest = {
    Name: 'Synthetic Pack',
    Version: '1.2.3',
    Author: 'Fixture Author',
    Description: 'Synthetic plugin package for e2e testing',
    CodePath: 'plugin.js',
    Icon: 'imgs/icon',
    PropertyInspectorPath: 'property_inspector.html',
    Actions: [
      {
        UUID: `${SYNTHETIC_PLUGIN_UUID}.action`,
        Name: 'Synthetic Action',
        Icon: 'imgs/action',
        PropertyInspectorPath: 'property_inspector.html',
        States: [
          {
            Image: 'imgs/action',
            Title: 'Pack Action',
          },
        ],
      },
    ],
  };
  fs.writeFileSync(
    path.join(pluginDir, 'manifest.json'),
    JSON.stringify(pluginManifest, null, 2),
    'utf8'
  );
  fs.writeFileSync(
    path.join(pluginDir, 'plugin.js'),
    '// Synthetic plugin script\nconsole.log("synthetic plugin ready");\n',
    'utf8'
  );
  fs.writeFileSync(
    path.join(pluginDir, 'property_inspector.html'),
    '<!doctype html><html><head><meta charset="utf-8"><title>Synthetic PI</title></head><body><h1>Synthetic PI</h1></body></html>',
    'utf8'
  );

  // 3. Synthetic Zip (.streamDeckPlugin)
  createZipFromDirectory(pluginDir, pluginZipPath);

  return { fixturesDir, profileDir, pluginDir, pluginZipPath };
}

// Generate if executed directly
if (process.argv[1] === __filename) {
  const result = ensureFixturesExist();
  console.log('Fixtures generated:', result);
}
