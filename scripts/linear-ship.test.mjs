import { test } from 'node:test';
import assert from 'node:assert/strict';
import { extractIssueIds, planTransitions, shippedComment } from './linear-ship.mjs';

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

test('shippedComment links the release', () => {
  assert.equal(
    shippedComment('v0.6.0', 'https://github.com/Mufdi/nergal/releases/tag/v0.6.0'),
    'Shipped in [v0.6.0](https://github.com/Mufdi/nergal/releases/tag/v0.6.0).',
  );
});
