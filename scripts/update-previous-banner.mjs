#!/usr/bin/env node
// Idempotent: strips any existing banner before re-rendering so re-runs
// never stack.

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';

const REPO_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const TEMPLATE_PATH = path.join(REPO_ROOT, 'scripts', 'release-banner.tpl.md');
const REPO = 'Mufdi/nergal';

const BANNER_RE =
  /^> \*\*Latest release →\*\*[^\n]*\n\n---\n\n/;

function gh(args) {
  const r = spawnSync('gh', args, { encoding: 'utf8' });
  if (r.status !== 0) {
    throw new Error(`[gh ${args.join(' ')}] ${r.stderr || r.stdout}`);
  }
  return r.stdout;
}

export function stripExistingBanner(body) {
  return body.replace(BANNER_RE, '');
}

export function renderBanner(template, newTag, originalBody) {
  // Template supplies the final `\n` — trim to keep re-runs idempotent.
  const trimmed = originalBody.replace(/\n+$/, '');
  return template
    .replaceAll('{{NEW_TAG}}', newTag)
    .replace('{{ORIGINAL_BODY}}', trimmed);
}

function main() {
  const newTag = process.argv[2];
  if (!newTag) {
    process.stderr.write('Usage: node scripts/update-previous-banner.mjs <new-tag>\n');
    process.exit(1);
  }

  const releasesJson = gh([
    'api',
    `repos/${REPO}/releases`,
    '--paginate',
    '--jq',
    '[.[] | {id, tag_name, created_at, body}]',
  ]);
  const releases = JSON.parse(releasesJson);
  const sorted = releases
    .filter((r) => r.tag_name !== newTag)
    .sort((a, b) => (a.created_at < b.created_at ? 1 : -1));
  const prev = sorted[0];
  if (!prev) {
    process.stdout.write('No previous release found; nothing to update.\n');
    return;
  }

  const template = fs.readFileSync(TEMPLATE_PATH, 'utf8');
  const stripped = stripExistingBanner(prev.body || '');
  const rendered = renderBanner(template, newTag, stripped);

  const tmpFile = path.join('/tmp', `prev-banner-${prev.id}.md`);
  fs.writeFileSync(tmpFile, rendered);

  gh(['release', 'edit', prev.tag_name, '--notes-file', tmpFile, '--repo', REPO]);
  process.stdout.write(`Updated banner on ${prev.tag_name} pointing to ${newTag}.\n`);
}

const isMain =
  !!process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url);
if (isMain) {
  try {
    main();
  } catch (e) {
    process.stderr.write(`update-previous-banner failed: ${e.message}\n`);
    process.exit(1);
  }
}
