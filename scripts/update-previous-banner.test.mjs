import { test } from 'node:test';
import assert from 'node:assert/strict';
import { stripExistingBanner, renderBanner } from './update-previous-banner.mjs';

test('stripExistingBanner removes the banner block', () => {
  const body = '> **Latest release →** [v0.1.5](https://github.com/Mufdi/nergal/releases/tag/v0.1.5)\n\n---\n\noriginal body';
  assert.equal(stripExistingBanner(body), 'original body');
});

test('stripExistingBanner is a no-op for bodies without a banner', () => {
  const body = 'just the body\n\n## section\n- thing\n';
  assert.equal(stripExistingBanner(body), body);
});

test('stripExistingBanner only strips at the start (later "Latest release →" text is preserved)', () => {
  const body = 'header\n\n> **Latest release →** [v1](url)\n\n---\n\nrest';
  assert.equal(stripExistingBanner(body), body);
});

test('renderBanner substitutes NEW_TAG and ORIGINAL_BODY', () => {
  const tpl =
    '> **Latest release →** [{{NEW_TAG}}](https://example/{{NEW_TAG}})\n\n---\n\n{{ORIGINAL_BODY}}\n';
  const rendered = renderBanner(tpl, 'v0.1.5', 'body content');
  assert.equal(
    rendered,
    '> **Latest release →** [v0.1.5](https://example/v0.1.5)\n\n---\n\nbody content\n',
  );
});

test('renderBanner is idempotent when re-run on the stripped output', () => {
  const tpl =
    '> **Latest release →** [{{NEW_TAG}}](https://example/{{NEW_TAG}})\n\n---\n\n{{ORIGINAL_BODY}}\n';
  const first = renderBanner(tpl, 'v0.1.5', 'body');
  const second = renderBanner(tpl, 'v0.1.5', stripExistingBanner(first));
  assert.equal(first, second);
});
