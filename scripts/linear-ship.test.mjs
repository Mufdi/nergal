import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  extractIssueIds,
  extractTrackerIds,
  mergeIds,
  planTransitions,
  shippedComment,
} from './linear-ship.mjs';

test('extractIssueIds finds ids in subjects, bodies and branch names, deduped and sorted', () => {
  const log = [
    'fix(linear): Fold hidden states (FRI-491)',
    'feat: Setup dialog (FRI-498)',
    "Merge branch 'mufdidev/fri-485-evaluar-linear-releases'",
    'Refs FRI-491 again',
  ].join('\n');
  assert.deepEqual(extractIssueIds(log, 'FRI'), ['FRI-485', 'FRI-491', 'FRI-498']);
});

test('extractIssueIds ignores other team keys and partial words', () => {
  assert.deepEqual(extractIssueIds('ENG-12 AFRI-3 FRI-7x FRI-8', 'FRI'), ['FRI-8']);
});

test('extractIssueIds returns nothing for a log without references', () => {
  assert.deepEqual(extractIssueIds('docs: Update README', 'FRI'), []);
});

test('planTransitions ships only Done issues', () => {
  const issues = [
    { identifier: 'FRI-1', state: 'Done' },
    { identifier: 'FRI-2', state: 'In Review' },
    { identifier: 'FRI-3', state: 'Shipped' },
  ];
  const { ship, skip } = planTransitions(issues);
  assert.deepEqual(ship.map((i) => i.identifier), ['FRI-1']);
  assert.deepEqual(skip.map((i) => i.identifier), ['FRI-2', 'FRI-3']);
});

test('planTransitions sets aside issues Linear no longer has instead of failing', () => {
  const issues = [
    { identifier: 'FRI-1', state: 'Done' },
    { id: null, identifier: 'FRI-2', state: null },
  ];
  const { ship, skip, missing } = planTransitions(issues);
  assert.deepEqual(ship.map((i) => i.identifier), ['FRI-1']);
  assert.deepEqual(skip, []);
  assert.deepEqual(missing.map((i) => i.identifier), ['FRI-2']);
});

test('shippedComment links the release', () => {
  assert.equal(
    shippedComment('v0.6.0', 'https://github.com/Mufdi/nergal/releases/tag/v0.6.0'),
    'Shipped in [v0.6.0](https://github.com/Mufdi/nergal/releases/tag/v0.6.0).',
  );
});

test('extractTrackerIds reads tracker.id out of archived handoff files', () => {
  const files = [
    JSON.stringify({ tier: 'M', tracker: { system: 'linear', id: 'FRI-368' } }),
    JSON.stringify({ tier: 'S', tracker: { system: 'linear', id: 'FRI-12' } }),
  ];
  assert.deepEqual(extractTrackerIds(files, 'FRI'), ['FRI-12', 'FRI-368']);
});

test('extractTrackerIds skips files without a tracker and other team keys', () => {
  const files = [
    JSON.stringify({ tier: 'M' }),
    JSON.stringify({ tracker: { system: 'linear', id: 'ENG-4' } }),
    JSON.stringify({ tracker: { system: 'linear', pending: [] } }),
  ];
  assert.deepEqual(extractTrackerIds(files, 'FRI'), []);
});

test('extractTrackerIds survives a malformed handoff file', () => {
  // A half-written JSON must not cost the rest of the release its transitions.
  const files = ['{ not json', JSON.stringify({ tracker: { id: 'FRI-9' } })];
  assert.deepEqual(extractTrackerIds(files, 'FRI'), ['FRI-9']);
});

test('mergeIds unions both sources without duplicating the overlap', () => {
  assert.deepEqual(mergeIds(['FRI-8', 'FRI-100'], ['FRI-100', 'FRI-2']), [
    'FRI-2',
    'FRI-8',
    'FRI-100',
  ]);
});
