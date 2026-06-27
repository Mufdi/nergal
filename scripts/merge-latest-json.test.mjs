import { test } from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const SCRIPT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), 'merge-latest-json.mjs');

function writeFrag(dir, name, data) {
  const p = path.join(dir, name);
  fs.writeFileSync(p, JSON.stringify(data, null, 2) + '\n');
  return p;
}

test('merges two fragments into a valid latest.json', () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'merge-test-'));
  const out = path.join(dir, 'latest.json');

  const linuxFrag = writeFrag(dir, 'linux.json', {
    version: 'v0.4.1',
    notes: 'Release notes',
    platform: 'linux-x86_64',
    signature: 'sig-linux',
    url: 'https://github.com/Mufdi/nergal/releases/download/v0.4.1/Nergal_0.4.1_amd64.AppImage',
  });
  const macosFrag = writeFrag(dir, 'macos.json', {
    version: 'v0.4.1',
    notes: 'Release notes',
    platform: 'darwin-aarch64',
    signature: 'sig-macos',
    url: 'https://github.com/Mufdi/nergal/releases/download/v0.4.1/Nergal.app.tar.gz',
  });

  const result = spawnSync('node', [SCRIPT, linuxFrag, macosFrag, out]);
  assert.equal(result.status, 0, `Exit non-zero: ${result.stderr.toString()}`);

  const manifest = JSON.parse(fs.readFileSync(out, 'utf8'));
  assert.equal(manifest.version, 'v0.4.1');
  assert.ok(manifest.pub_date, 'pub_date must be present');
  assert.ok(new Date(manifest.pub_date).getTime() > 0, 'pub_date must be a valid ISO date');
  assert.ok(manifest.platforms['linux-x86_64'], 'linux-x86_64 platform entry must exist');
  assert.ok(manifest.platforms['darwin-aarch64'], 'darwin-aarch64 platform entry must exist');
  assert.equal(manifest.platforms['linux-x86_64'].signature, 'sig-linux');
  assert.equal(manifest.platforms['darwin-aarch64'].signature, 'sig-macos');
});

test('darwin-aarch64 url ends with .app.tar.gz, not .dmg', () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'merge-test-'));
  const out = path.join(dir, 'latest.json');

  const macosFrag = writeFrag(dir, 'macos.json', {
    version: 'v0.4.1',
    notes: 'notes',
    platform: 'darwin-aarch64',
    signature: 'sig',
    url: 'https://github.com/Mufdi/nergal/releases/download/v0.4.1/Nergal.app.tar.gz',
  });

  spawnSync('node', [SCRIPT, macosFrag, out]);
  const manifest = JSON.parse(fs.readFileSync(out, 'utf8'));
  const darwinUrl = manifest.platforms['darwin-aarch64'].url;
  assert.ok(darwinUrl.endsWith('.app.tar.gz'), `Expected .app.tar.gz, got: ${darwinUrl}`);
  assert.ok(!darwinUrl.endsWith('.dmg'), '.dmg must not appear in latest.json darwin url');
});

test('aborts with non-zero exit when version fields differ across fragments', () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'merge-test-'));
  const out = path.join(dir, 'latest.json');

  const linuxFrag = writeFrag(dir, 'linux.json', {
    version: 'v0.4.1',
    notes: 'notes',
    platform: 'linux-x86_64',
    signature: 'sig-linux',
    url: 'https://github.com/example/r/download/v0.4.1/app.AppImage',
  });
  const macosFrag = writeFrag(dir, 'macos.json', {
    version: 'v0.4.2',
    notes: 'notes',
    platform: 'darwin-aarch64',
    signature: 'sig-macos',
    url: 'https://github.com/example/r/download/v0.4.2/app.tar.gz',
  });

  const result = spawnSync('node', [SCRIPT, linuxFrag, macosFrag, out]);
  assert.notEqual(result.status, 0, 'Should exit non-zero on version mismatch');
  assert.ok(!fs.existsSync(out), 'Output file must not be written on version mismatch');
});
