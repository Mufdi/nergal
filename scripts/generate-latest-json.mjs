#!/usr/bin/env node

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { buildReleaseBody } from './extract-release-body.mjs';

const REPO_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const CHANGELOG = path.join(REPO_ROOT, 'CHANGELOG.md');
const APPIMAGE_DIR = path.join(REPO_ROOT, 'src-tauri', 'target', 'release', 'bundle', 'appimage');

function findOne(dir, suffix) {
  const entries = fs.readdirSync(dir).filter((f) => f.endsWith(suffix));
  if (entries.length === 0) throw new Error(`No file matching *${suffix} in ${dir}`);
  if (entries.length > 1) throw new Error(`Multiple files matching *${suffix} in ${dir}: ${entries.join(', ')}`);
  return entries[0];
}

function main() {
  const tag = process.argv[2];
  const outPath = process.argv[3];
  if (!tag || !outPath) {
    process.stderr.write('Usage: node scripts/generate-latest-json.mjs <tag> <out-path>\n');
    process.exit(1);
  }
  const version = tag.replace(/^v/, '');

  const appImageName = findOne(APPIMAGE_DIR, '.AppImage');
  const sigName = findOne(APPIMAGE_DIR, '.AppImage.sig');
  const sigContent = fs.readFileSync(path.join(APPIMAGE_DIR, sigName), 'utf8').trim();

  const changelog = fs.readFileSync(CHANGELOG, 'utf8');
  const notes = buildReleaseBody(changelog, version) ?? '';

  const manifest = {
    version: tag,
    notes,
    pub_date: new Date().toISOString(),
    platforms: {
      'linux-x86_64': {
        signature: sigContent,
        url: `https://github.com/Mufdi/nergal/releases/download/${tag}/${appImageName}`,
      },
    },
  };

  fs.writeFileSync(outPath, JSON.stringify(manifest, null, 2) + '\n');
  process.stdout.write(`Wrote ${outPath} (AppImage: ${appImageName}, sig: ${sigName})\n`);
}

const isMain = import.meta.url === `file://${process.argv[1]}`;
if (isMain) {
  try {
    main();
  } catch (e) {
    process.stderr.write(`generate-latest-json failed: ${e.message}\n`);
    process.exit(1);
  }
}
