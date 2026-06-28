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

export const prereleasePlaceholder = (version) =>
  `Pre-release \`v${version}\` — CI pipeline smoke-test, not a user-facing release.`;

// CLI/CI resolution. A pre-release tag (normalized version contains `-`, e.g.
// `0.0.0-test1`) is a pipeline smoke-test that intentionally ships without a
// CHANGELOG section, so fall back to a placeholder body instead of failing the
// publish job. Stable tags still hard-require their section (a missing one is a
// real release mistake). Returns `{ body }` or `{ error }`.
export function resolveReleaseBody(changelog, normalizedVersion) {
  const body = buildReleaseBody(changelog, normalizedVersion);
  if (body !== null) return { body };
  if (normalizedVersion.includes('-')) {
    return { body: prereleasePlaceholder(normalizedVersion) };
  }
  return { error: `CHANGELOG.md has no section for v${normalizedVersion}.` };
}

function main() {
  const version = process.argv[2];
  if (!version) {
    process.stderr.write('Usage: node scripts/extract-release-body.mjs <version>\n');
    process.exit(1);
  }
  const normalized = version.replace(/^v/, '');
  const content = fs.readFileSync(CHANGELOG, 'utf8');
  const { body, error } = resolveReleaseBody(content, normalized);
  if (error) {
    process.stderr.write(`${error}\n`);
    process.exit(1);
  }
  process.stdout.write(body);
  if (!body.endsWith('\n')) process.stdout.write('\n');
}

const isMain =
  !!process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url);
if (isMain) {
  main();
}
