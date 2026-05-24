#!/usr/bin/env node

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { extractChangelogSection } from './release.mjs';

const REPO_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const CHANGELOG = path.join(REPO_ROOT, 'CHANGELOG.md');

// GH release chrome already shows the tag + date.
export function stripVersionHeader(section) {
  const lines = section.split('\n');
  if (lines.length === 0) return section;
  if (/^## v\d/.test(lines[0])) {
    let start = 1;
    while (start < lines.length && lines[start].trim() === '') start++;
    return lines.slice(start).join('\n');
  }
  return section;
}

export function buildReleaseBody(changelog, version) {
  const section = extractChangelogSection(changelog, version);
  if (section === null) return null;
  return stripVersionHeader(section);
}

function main() {
  const version = process.argv[2];
  if (!version) {
    process.stderr.write('Usage: node scripts/extract-release-body.mjs <version>\n');
    process.exit(1);
  }
  const normalized = version.replace(/^v/, '');
  const content = fs.readFileSync(CHANGELOG, 'utf8');
  const body = buildReleaseBody(content, normalized);
  if (body === null) {
    process.stderr.write(`CHANGELOG.md has no section for v${normalized}.\n`);
    process.exit(1);
  }
  process.stdout.write(body);
  if (!body.endsWith('\n')) process.stdout.write('\n');
}

const isMain = import.meta.url === `file://${process.argv[1]}`;
if (isMain) {
  main();
}
