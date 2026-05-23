import { test } from 'node:test';
import assert from 'node:assert/strict';
import { bumpVersion, extractChangelogSection, isWorkingTreeAllowedDirty } from './release.mjs';

// ---- bumpVersion ----

test('bumpVersion: patch increments patch', () => {
  assert.equal(bumpVersion('0.1.3', 'patch'), '0.1.4');
  assert.equal(bumpVersion('0.9.9', 'patch'), '0.9.10');
  assert.equal(bumpVersion('1.0.0', 'patch'), '1.0.1');
});

test('bumpVersion: minor increments minor and resets patch', () => {
  assert.equal(bumpVersion('0.1.3', 'minor'), '0.2.0');
  assert.equal(bumpVersion('1.5.9', 'minor'), '1.6.0');
});

test('bumpVersion: major increments major and resets minor/patch', () => {
  assert.equal(bumpVersion('0.1.3', 'major'), '1.0.0');
  assert.equal(bumpVersion('1.5.9', 'major'), '2.0.0');
});

test('bumpVersion: explicit version used verbatim', () => {
  assert.equal(bumpVersion('0.1.3', '0.1.10'), '0.1.10');
  assert.equal(bumpVersion('0.1.3', 'v2.0.0'), '2.0.0');
  assert.equal(bumpVersion('0.1.3', '999.99.99'), '999.99.99');
});

test('bumpVersion: invalid bump throws', () => {
  assert.throws(() => bumpVersion('0.1.3', 'foo'), /Invalid bump/);
  assert.throws(() => bumpVersion('not-a-version', 'patch'), /Invalid current version/);
});

// ---- extractChangelogSection ----

test('extractChangelogSection: finds section between version headers', () => {
  const content = `# Changelog

## v0.1.4 — 2026-05-22

* Added X
* Fixed Y

## v0.1.3 — 2026-05-20

* Added Z
`;
  const result = extractChangelogSection(content, '0.1.4');
  assert.equal(
    result,
    `## v0.1.4 — 2026-05-22

* Added X
* Fixed Y`,
  );
});

test('extractChangelogSection: returns null when missing', () => {
  const content = `# Changelog

## v0.1.3 — 2026-05-20

* Added Z
`;
  assert.equal(extractChangelogSection(content, '0.1.4'), null);
});

test('extractChangelogSection: section at EOF', () => {
  const content = `# Changelog

## v0.1.4 — 2026-05-22

* Added X
`;
  assert.equal(
    extractChangelogSection(content, '0.1.4'),
    `## v0.1.4 — 2026-05-22

* Added X`,
  );
});

test('extractChangelogSection: word boundary prevents prefix match', () => {
  const content = `# Changelog

## v0.1.40 — 2026-12-01

* Added P
`;
  assert.equal(extractChangelogSection(content, '0.1.4'), null);
});

test('extractChangelogSection: handles real-repo CHANGELOG shape', () => {
  // Modeled after the actual repo CHANGELOG.md format
  const content = `# Changelog

## v0.1.3 — 2026-05-20

* Added per-agent theme sync
* Fixed the "Open on GitHub" icon
* Changed the sidebar focus model

## v0.1.2 — 2026-05-11

* Added X
`;
  const result = extractChangelogSection(content, '0.1.3');
  assert.ok(result.startsWith('## v0.1.3 — 2026-05-20'));
  assert.ok(result.includes('Added per-agent theme sync'));
  assert.ok(!result.includes('## v0.1.2'));
});

// ---- isWorkingTreeAllowedDirty ----

test('isWorkingTreeAllowedDirty: empty tree is allowed', () => {
  assert.equal(isWorkingTreeAllowedDirty(''), true);
  assert.equal(isWorkingTreeAllowedDirty('   '), true);
});

test('isWorkingTreeAllowedDirty: only CHANGELOG.md modified is allowed', () => {
  assert.equal(isWorkingTreeAllowedDirty(' M CHANGELOG.md'), true);
  assert.equal(isWorkingTreeAllowedDirty('M  CHANGELOG.md'), true);
  assert.equal(isWorkingTreeAllowedDirty('MM CHANGELOG.md'), true);
});

test('isWorkingTreeAllowedDirty: any other file blocks', () => {
  assert.equal(isWorkingTreeAllowedDirty(' M src/foo.tsx'), false);
  assert.equal(isWorkingTreeAllowedDirty(' M package.json'), false);
  assert.equal(isWorkingTreeAllowedDirty(' M CHANGELOG.md\n M src/foo.tsx'), false);
});

test('isWorkingTreeAllowedDirty: untracked file blocks', () => {
  assert.equal(isWorkingTreeAllowedDirty('?? newfile.md'), false);
});
