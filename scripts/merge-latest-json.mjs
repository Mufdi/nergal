#!/usr/bin/env node
// Merges per-platform CI fragments into a single Tauri updater latest.json.
//
// Usage: node scripts/merge-latest-json.mjs <frag1.json> [frag2.json ...] <out.json>
//
// Each fragment must contain: version, notes, platform, signature, url.
// version must be identical across all fragments (mismatch = abort).
// pub_date is generated here so per-runner clock skew never diverges.

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

function mergeFragments(fragmentPaths, outPath) {
  if (fragmentPaths.length < 1) {
    process.stderr.write('merge-latest-json: at least one fragment path required\n');
    process.exit(1);
  }

  const fragments = fragmentPaths.map((p) => {
    let raw;
    try {
      raw = fs.readFileSync(p, 'utf8');
    } catch (e) {
      process.stderr.write(`merge-latest-json: cannot read ${p}: ${e.message}\n`);
      process.exit(1);
    }
    let frag;
    try {
      frag = JSON.parse(raw);
    } catch (e) {
      process.stderr.write(`merge-latest-json: invalid JSON in ${p}: ${e.message}\n`);
      process.exit(1);
    }
    for (const key of ['version', 'platform', 'signature', 'url']) {
      if (!frag[key]) {
        process.stderr.write(`merge-latest-json: fragment ${p} missing required field "${key}"\n`);
        process.exit(1);
      }
    }
    return frag;
  });

  const first = fragments[0];

  // version mismatch across runners = hard abort; must not ship.
  for (let i = 1; i < fragments.length; i++) {
    if (fragments[i].version !== first.version) {
      process.stderr.write(
        `merge-latest-json: version mismatch — fragment[0]="${first.version}" but fragment[${i}]="${fragments[i].version}". Aborting.\n`,
      );
      process.exit(1);
    }
    if (fragments[i].notes !== first.notes) {
      process.stderr.write(
        `merge-latest-json: warning — notes differ between fragment[0] and fragment[${i}] (proceeding with fragment[0] value)\n`,
      );
    }
  }

  const platforms = {};
  for (const frag of fragments) {
    platforms[frag.platform] = { signature: frag.signature, url: frag.url };
  }

  const manifest = {
    version: first.version,
    notes: first.notes ?? '',
    pub_date: new Date().toISOString(),
    platforms,
  };

  fs.writeFileSync(outPath, JSON.stringify(manifest, null, 2) + '\n');
  process.stdout.write(
    `Wrote ${outPath} (platforms: ${Object.keys(platforms).join(', ')})\n`,
  );
  return manifest;
}

// Real-path comparison so this also works if ever run on Windows (argv[1] is a
// backslash drive path there, never equal to the file:///D:/… import URL).
const isMain =
  !!process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url);
if (isMain) {
  // Last positional arg is the output path; all prior args are fragment paths.
  const args = process.argv.slice(2);
  if (args.length < 2) {
    process.stderr.write(
      'Usage: node scripts/merge-latest-json.mjs <frag1.json> [frag2.json ...] <out.json>\n',
    );
    process.exit(1);
  }
  const outPath = args[args.length - 1];
  const fragmentPaths = args.slice(0, args.length - 1);
  try {
    mergeFragments(fragmentPaths, outPath);
  } catch (e) {
    process.stderr.write(`merge-latest-json failed: ${e.message}\n`);
    process.exit(1);
  }
}

export { mergeFragments };
