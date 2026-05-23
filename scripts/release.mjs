#!/usr/bin/env node
// Release script for Nergal/cluihud. See openspec/changes/release-script/ for the contract.
// 2-step ship flow: Claude writes CHANGELOG section in session → this script verifies + bumps + commits + tags + pushes.

import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const REPO_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const PACKAGE_JSON = path.join(REPO_ROOT, 'package.json');
const CARGO_TOML = path.join(REPO_ROOT, 'src-tauri', 'Cargo.toml');
const TAURI_CONF = path.join(REPO_ROOT, 'src-tauri', 'tauri.conf.json');
const CARGO_LOCK = path.join(REPO_ROOT, 'src-tauri', 'Cargo.lock');
const CHANGELOG = path.join(REPO_ROOT, 'CHANGELOG.md');
const TRACKED_FILES = [
  'package.json',
  'src-tauri/Cargo.toml',
  'src-tauri/tauri.conf.json',
  'src-tauri/Cargo.lock',
  'CHANGELOG.md',
];

// ---- Pure helpers (exported for tests) ----

export function bumpVersion(current, bump) {
  if (/^v?\d+\.\d+\.\d+$/.test(bump)) {
    return bump.replace(/^v/, '');
  }
  const m = current.match(/^(\d+)\.(\d+)\.(\d+)$/);
  if (!m) throw new Error(`Invalid current version: ${current}`);
  const [major, minor, patch] = [Number(m[1]), Number(m[2]), Number(m[3])];
  switch (bump) {
    case 'patch':
      return `${major}.${minor}.${patch + 1}`;
    case 'minor':
      return `${major}.${minor + 1}.0`;
    case 'major':
      return `${major + 1}.0.0`;
    default:
      throw new Error(`Invalid bump: ${bump}. Use patch | minor | major | <X.Y.Z>`);
  }
}

export function extractChangelogSection(content, version) {
  const lines = content.split('\n');
  const escaped = version.replace(/\./g, '\\.');
  const headerRe = new RegExp(`^## v${escaped}\\b`);
  const start = lines.findIndex((l) => headerRe.test(l));
  if (start === -1) return null;
  let end = lines.length;
  for (let i = start + 1; i < lines.length; i++) {
    if (lines[i].startsWith('## ')) {
      end = i;
      break;
    }
  }
  let lastNonBlank = end - 1;
  while (lastNonBlank > start && lines[lastNonBlank].trim() === '') {
    lastNonBlank--;
  }
  return lines.slice(start, lastNonBlank + 1).join('\n');
}

export function isWorkingTreeAllowedDirty(porcelainOutput) {
  const trimmed = porcelainOutput.trim();
  if (trimmed === '') return true;
  const lines = trimmed.split('\n');
  if (lines.length !== 1) return false;
  return /^[\sM]M?\s+CHANGELOG\.md$/.test(lines[0]);
}

// ---- Subprocess helpers ----

function git(args, opts = {}) {
  return spawnSync('git', args, { encoding: 'utf8', cwd: REPO_ROOT, ...opts });
}

function gitOrDie(args, opts = {}) {
  const r = git(args, opts);
  if (r.status !== 0) {
    process.stderr.write(`[git ${args.join(' ')}] ${r.stderr || ''}\n`);
    process.exit(1);
  }
  return r.stdout.trim();
}

// ---- CLI ----

function usage() {
  process.stdout.write(`Usage: pnpm release <bump> [--dry-run] [--no-push]

Bump:
  patch         Increment patch (0.1.3 → 0.1.4)
  minor         Increment minor (0.1.3 → 0.2.0)
  major         Increment major (0.1.3 → 1.0.0)
  <X.Y.Z>       Explicit version (0.1.10 or v0.1.10)

Flags:
  --dry-run     All reads + computation, no mutations
  --no-push     Local commit + tag, skip push (testing); also relaxes the
                "must be on main" guard so you can run on a test branch
  --help        Show this message

2-step ship flow:
  1. In a Claude session: say "cortemos v0.1.X" → Claude reads commits +
     writes a contextual CHANGELOG section into CHANGELOG.md.
  2. Run \`pnpm release <bump>\` → script verifies section presence, bumps
     versions, refreshes Cargo.lock, commits, tags, pushes.

Build + GitHub release remain manual until OpenSpec change \`release-ci-signed\`.
`);
}

function parseArgs(argv) {
  const args = argv.slice(2);
  if (args.length === 0 || args.includes('--help') || args.includes('-h')) {
    usage();
    process.exit(args.length === 0 ? 1 : 0);
  }
  const flags = { dryRun: false, noPush: false };
  let bump = null;
  for (const a of args) {
    if (a === '--dry-run') flags.dryRun = true;
    else if (a === '--no-push') flags.noPush = true;
    else if (bump === null) bump = a;
    else {
      process.stderr.write(`Unexpected argument: ${a}\n`);
      usage();
      process.exit(1);
    }
  }
  if (!bump) {
    process.stderr.write('Bump argument required.\n');
    usage();
    process.exit(1);
  }
  return { bump, ...flags };
}

// ---- Main flow ----

async function main() {
  const { bump, dryRun, noPush } = parseArgs(process.argv);

  const pkg = JSON.parse(fs.readFileSync(PACKAGE_JSON, 'utf8'));
  const current = pkg.version;

  let next;
  try {
    next = bumpVersion(current, bump);
  } catch (e) {
    process.stderr.write(`${e.message}\n`);
    process.exit(1);
  }
  const newTag = `v${next}`;
  const mode = dryRun ? 'DRY-RUN' : noPush ? 'NO-PUSH' : 'LIVE';

  process.stdout.write(`Current: ${current}\nNew:     ${next}\nTag:     ${newTag}\nMode:    ${mode}\n\n`);

  // ---- Pre-flight guards ----
  process.stdout.write('[1/5] Pre-flight guards...\n');

  const porcelain = gitOrDie(['status', '--porcelain']);
  if (!isWorkingTreeAllowedDirty(porcelain)) {
    process.stderr.write('Working tree has unrelated uncommitted changes:\n');
    process.stderr.write(porcelain + '\n');
    process.stderr.write('Commit or stash before release. (CHANGELOG.md may be dirty; nothing else.)\n');
    process.exit(1);
  }

  const branch = gitOrDie(['rev-parse', '--abbrev-ref', 'HEAD']);
  if (branch !== 'main' && !noPush) {
    process.stderr.write(`Release must run from main, currently on ${branch}.\n`);
    process.stderr.write(`(--no-push relaxes this guard for integration testing.)\n`);
    process.exit(1);
  }

  const prevTagR = git(['describe', '--tags', '--abbrev=0']);
  if (prevTagR.status !== 0) {
    process.stderr.write('No previous tag found — tag the previous release manually first.\n');
    process.exit(1);
  }
  const prevTag = prevTagR.stdout.trim();

  const localTag = gitOrDie(['tag', '--list', newTag]);
  if (localTag) {
    process.stderr.write(`Tag ${newTag} already exists locally.\n`);
    process.exit(1);
  }

  const remoteTag = gitOrDie(['ls-remote', '--tags', 'origin', `refs/tags/${newTag}`]);
  if (remoteTag) {
    process.stderr.write(`Tag ${newTag} already exists on origin.\n`);
    process.exit(1);
  }

  const changelogContent = fs.readFileSync(CHANGELOG, 'utf8');
  const section = extractChangelogSection(changelogContent, next);
  if (!section) {
    process.stderr.write(
      `CHANGELOG.md is missing the ${newTag} section — generate it first in a Claude session ` +
        `(the bug-workflow ship ritual recommends this step as part of "cortemos ${newTag}"). ` +
        `Aborting before any file is touched.\n`,
    );
    process.exit(1);
  }

  process.stdout.write('  ✓ working tree clean (CHANGELOG.md allowed dirty)\n');
  process.stdout.write(`  ✓ branch: ${branch}${branch !== 'main' ? ' (--no-push)' : ''}\n`);
  process.stdout.write(`  ✓ prev tag: ${prevTag}\n`);
  process.stdout.write(`  ✓ new tag ${newTag} not in local or origin\n`);
  process.stdout.write(`  ✓ CHANGELOG.md has ${newTag} section\n\n`);

  // ---- Version bumps ----
  process.stdout.write('[2/5] Version bumps...\n');

  if (dryRun) {
    process.stdout.write(`  WOULD UPDATE package.json: ${current} → ${next}\n`);
    process.stdout.write(`  WOULD UPDATE src-tauri/Cargo.toml: ${current} → ${next}\n`);
    process.stdout.write(`  WOULD UPDATE src-tauri/tauri.conf.json: ${current} → ${next}\n`);
    process.stdout.write(`  WOULD REFRESH src-tauri/Cargo.lock\n`);
  } else {
    const pkgRaw = fs.readFileSync(PACKAGE_JSON, 'utf8');
    const newPkgRaw = pkgRaw.replace(/("version":\s*")\d+\.\d+\.\d+(")/, `$1${next}$2`);
    if (newPkgRaw === pkgRaw) {
      process.stderr.write('Failed to update package.json version field.\n');
      process.exit(1);
    }
    fs.writeFileSync(PACKAGE_JSON, newPkgRaw);
    process.stdout.write(`  ✓ package.json: ${current} → ${next}\n`);

    const cargoRaw = fs.readFileSync(CARGO_TOML, 'utf8');
    const newCargoRaw = cargoRaw.replace(
      /(\[package\][\s\S]*?\nversion\s*=\s*")\d+\.\d+\.\d+(")/,
      `$1${next}$2`,
    );
    if (newCargoRaw === cargoRaw) {
      process.stderr.write('Failed to update src-tauri/Cargo.toml version field.\n');
      process.exit(1);
    }
    fs.writeFileSync(CARGO_TOML, newCargoRaw);
    process.stdout.write(`  ✓ src-tauri/Cargo.toml: ${current} → ${next}\n`);

    const tauriConf = JSON.parse(fs.readFileSync(TAURI_CONF, 'utf8'));
    tauriConf.version = next;
    fs.writeFileSync(TAURI_CONF, JSON.stringify(tauriConf, null, 2) + '\n');
    process.stdout.write(`  ✓ src-tauri/tauri.conf.json: ${current} → ${next}\n`);

    const lockBefore = fs.existsSync(CARGO_LOCK) ? fs.statSync(CARGO_LOCK).mtimeMs : 0;
    const cargoR = spawnSync('cargo', ['check', '--offline'], {
      cwd: path.join(REPO_ROOT, 'src-tauri'),
      stdio: 'pipe',
      encoding: 'utf8',
    });
    if (cargoR.status !== 0) {
      const retry = spawnSync('cargo', ['check'], {
        cwd: path.join(REPO_ROOT, 'src-tauri'),
        stdio: 'pipe',
        encoding: 'utf8',
      });
      if (retry.status !== 0) {
        process.stderr.write('[cargo check] failed:\n');
        process.stderr.write((retry.stderr || cargoR.stderr) + '\n');
        process.exit(1);
      }
    }
    const lockAfter = fs.statSync(CARGO_LOCK).mtimeMs;
    if (lockAfter <= lockBefore) {
      process.stdout.write('  ⚠ Cargo.lock mtime unchanged (probably already in sync with new version)\n');
    } else {
      process.stdout.write('  ✓ src-tauri/Cargo.lock refreshed\n');
    }
  }
  process.stdout.write('\n');

  // ---- CHANGELOG echo ----
  process.stdout.write('[3/5] CHANGELOG section echo...\n');
  process.stdout.write('--- BEGIN CHANGELOG ENTRY ---\n');
  process.stdout.write(section + '\n');
  process.stdout.write('--- END CHANGELOG ENTRY ---\n\n');

  if (dryRun) {
    process.stdout.write('[4/5] WOULD COMMIT, TAG, PUSH (dry-run)\n');
    process.stdout.write(`  WOULD STAGE: ${TRACKED_FILES.join(', ')}\n`);
    process.stdout.write(`  WOULD COMMIT: chore(release): ${newTag}\n`);
    process.stdout.write(`  WOULD TAG: ${newTag}\n`);
    process.stdout.write(`  WOULD PUSH origin main\n`);
    process.stdout.write(`  WOULD PUSH origin ${newTag}\n\n`);
    process.stdout.write('[5/5] Dry-run complete. No changes made.\n');
    return;
  }

  // ---- Stage + commit + tag ----
  process.stdout.write('[4/5] Commit + tag...\n');
  gitOrDie(['add', ...TRACKED_FILES]);
  process.stdout.write('  ✓ staged 5 files\n');

  const commitR = git(['commit', '-m', `chore(release): ${newTag}`]);
  if (commitR.status !== 0) {
    process.stderr.write('[git commit] failed:\n');
    process.stderr.write((commitR.stderr || commitR.stdout) + '\n');
    process.stderr.write(
      `\nFiles modified but step 'commit' failed. Inspect with \`git status\` and \`git diff\`.\n` +
        `To reset non-changelog files: git checkout -- package.json src-tauri/Cargo.toml src-tauri/tauri.conf.json src-tauri/Cargo.lock\n` +
        `CHANGELOG.md edits were written earlier by Claude — keep them or revert manually.\n`,
    );
    process.exit(1);
  }
  const commitSha = gitOrDie(['rev-parse', 'HEAD']);
  process.stdout.write(`  ✓ commit ${commitSha.slice(0, 8)}: chore(release): ${newTag}\n`);

  const tagR = git(['tag', newTag]);
  if (tagR.status !== 0) {
    process.stderr.write('[git tag] failed:\n');
    process.stderr.write(tagR.stderr + '\n');
    process.exit(1);
  }
  process.stdout.write(`  ✓ tag ${newTag}\n\n`);

  // ---- Push ----
  process.stdout.write('[5/5] Push...\n');
  if (noPush) {
    process.stdout.write(`  WOULD PUSH origin main\n`);
    process.stdout.write(`  WOULD PUSH origin ${newTag}\n\n`);
    process.stdout.write(`Released ${newTag} locally (--no-push).\n`);
    process.stdout.write(`To push manually: git push origin main && git push origin ${newTag}\n`);
    return;
  }

  const pushMainR = git(['push', 'origin', 'main']);
  if (pushMainR.status !== 0) {
    process.stderr.write('[git push origin main] failed:\n');
    process.stderr.write(pushMainR.stderr + '\n');
    process.stderr.write(
      `\nLocal commit and tag created but push of main failed.\n` +
        `To retry: git push origin main && git push origin ${newTag}\n` +
        `To undo: git reset --hard HEAD~1 && git tag -d ${newTag}\n`,
    );
    process.exit(1);
  }
  process.stdout.write('  ✓ pushed main\n');

  const pushTagR = git(['push', 'origin', newTag]);
  if (pushTagR.status !== 0) {
    process.stderr.write(`[git push origin ${newTag}] failed:\n`);
    process.stderr.write(pushTagR.stderr + '\n');
    process.stderr.write(`\nMain pushed but tag push failed. To retry: git push origin ${newTag}\n`);
    process.exit(1);
  }
  process.stdout.write(`  ✓ pushed ${newTag}\n\n`);

  process.stdout.write(`Released ${newTag} (commit ${commitSha.slice(0, 8)}).\n\n`);
  process.stdout.write(`Build artifacts and GitHub release remain manual until Change B (release-ci-signed).\n`);
  process.stdout.write(`Next manual steps:\n`);
  process.stdout.write(`  GSTREAMER_PLUGINS_DIR=/usr/lib/x86_64-linux-gnu/gstreamer-1.0 pnpm tauri build\n`);
  process.stdout.write(`  gh release create ${newTag} --title "Nergal ${newTag}" --notes-file <body.md> <artifact paths>\n`);
}

// Only run main when invoked directly (not when imported by tests)
const isMain = import.meta.url === `file://${process.argv[1]}`;
if (isMain) {
  main().catch((e) => {
    process.stderr.write(`Unexpected error: ${e.message}\n`);
    process.exit(1);
  });
}
