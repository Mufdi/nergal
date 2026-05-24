import { test } from 'node:test';
import assert from 'node:assert/strict';
import { stripVersionHeader, buildReleaseBody } from './extract-release-body.mjs';

test('stripVersionHeader removes the `## vX.Y.Z — DATE` first line', () => {
  const section = '## v0.1.4 — 2026-05-23\n\n### Highlights\n- thing\n';
  assert.equal(stripVersionHeader(section), '### Highlights\n- thing\n');
});

test('stripVersionHeader keeps non-version-header content untouched', () => {
  const section = '### Highlights\n- thing\n';
  assert.equal(stripVersionHeader(section), '### Highlights\n- thing\n');
});

test('stripVersionHeader collapses blank lines after the header', () => {
  const section = '## v0.1.4\n\n\n\nbody\n';
  assert.equal(stripVersionHeader(section), 'body\n');
});

test('buildReleaseBody returns null when section missing', () => {
  const changelog = '## v0.1.3 — 2026-04-01\n\nstuff\n';
  assert.equal(buildReleaseBody(changelog, '0.1.4'), null);
});

test('buildReleaseBody extracts + strips header for real-shape CHANGELOG', () => {
  const changelog = [
    '# Changelog',
    '',
    '## v0.1.4 — 2026-05-23',
    '',
    '### Added',
    '- foo',
    '',
    '### Fixed',
    '- bar',
    '',
    '## v0.1.3 — 2026-04-01',
    '',
    '- previous',
    '',
  ].join('\n');
  const body = buildReleaseBody(changelog, '0.1.4');
  assert.equal(body, '### Added\n- foo\n\n### Fixed\n- bar');
});

test('buildReleaseBody works for the last section in the file', () => {
  const changelog = '# Changelog\n\n## v0.1.4 — 2026-05-23\n\nbody\n';
  assert.equal(buildReleaseBody(changelog, '0.1.4'), 'body');
});
