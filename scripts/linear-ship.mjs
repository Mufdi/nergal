#!/usr/bin/env node
// Moves the Linear issues referenced by a release's commits from Done to
// Shipped and comments the release on each one. Done means merged and
// verified; Shipped means it reached users, so only CI may set it, at the
// moment the release is published.

import { spawnSync } from 'node:child_process';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const API = 'https://api.linear.app/graphql';
const FROM_STATE = 'Done';
const TO_STATE = 'Shipped';
const ARCHIVE_DIR = 'openspec/changes/archive';
const TRACKER_FILE = /\/handoff\/\.work-modules\.json$/;

function byNumber(ids) {
  return [...ids].sort((a, b) => Number(a.split('-')[1]) - Number(b.split('-')[1]));
}

export function extractIssueIds(text, teamKey) {
  const re = new RegExp(`\\b${teamKey}-(\\d+)\\b`, 'gi');
  const ids = new Set();
  for (const m of text.matchAll(re)) ids.add(`${teamKey.toUpperCase()}-${m[1]}`);
  return byNumber(ids);
}

/**
 * The second id source, for repos where the commits do not carry the id: in
 * XD001 only 8 of the last 200 did, so reading the log alone would have left
 * most of a release's issues sitting in Done. What does carry the id reliably
 * is the openspec change — `handoff/.work-modules.json` records `tracker.id` —
 * and archiving that change is the last thing a piece of work does before it
 * ships, so an archive that appears inside the release range is a shipped
 * issue.
 *
 * Returns nothing when the repo has no such directory, which is why both
 * sources are always read instead of configured per project.
 */
export function extractTrackerIds(contents, teamKey) {
  const re = new RegExp(`^${teamKey}-\\d+$`, 'i');
  const ids = new Set();
  for (const raw of contents) {
    let id;
    try {
      id = JSON.parse(raw)?.tracker?.id;
    } catch {
      // A malformed or half-written handoff file is not a reason to abandon the
      // release: the other changes in the range still have to be moved.
      continue;
    }
    if (typeof id === 'string' && re.test(id)) ids.add(id.toUpperCase());
  }
  return byNumber(ids);
}

export function mergeIds(...lists) {
  return byNumber(new Set(lists.flat()));
}

export function planTransitions(issues) {
  const ship = [];
  const skip = [];
  for (const issue of issues) {
    if (issue.state === FROM_STATE) ship.push(issue);
    else skip.push(issue);
  }
  return { ship, skip };
}

export function shippedComment(tag, url) {
  return `Shipped in [${tag}](${url}).`;
}

function git(args) {
  const r = spawnSync('git', args, { encoding: 'utf8' });
  if (r.status !== 0) throw new Error(`[git ${args.join(' ')}] ${r.stderr || r.stdout}`);
  return r.stdout.trim();
}

/** The `.work-modules.json` of every change archived inside the range, as it
 *  stood at the release ref. */
function archivedTrackerFiles(range, ref) {
  const added = git(['log', '--diff-filter=A', '--name-only', '--format=', range, '--', ARCHIVE_DIR]);
  const paths = [...new Set(added.split('\n').map((l) => l.trim()).filter((l) => TRACKER_FILE.test(l)))];
  return paths.map((file) => {
    const r = spawnSync('git', ['show', `${ref}:${file}`], { encoding: 'utf8' });
    return r.status === 0 ? r.stdout : '';
  });
}

function previousTag(tag) {
  const r = spawnSync('git', ['describe', '--tags', '--abbrev=0', `${tag}^`], { encoding: 'utf8' });
  return r.status === 0 ? r.stdout.trim() : null;
}

async function linear(apiKey, query, variables) {
  const res = await fetch(API, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json', Authorization: apiKey },
    body: JSON.stringify({ query, variables }),
  });
  const body = await res.json();
  if (!res.ok || body.errors) {
    throw new Error(`Linear API ${res.status}: ${JSON.stringify(body.errors ?? body)}`);
  }
  return body.data;
}

async function fetchIssue(apiKey, identifier) {
  try {
    const data = await linear(
      apiKey,
      'query($id: String!) { issue(id: $id) { id identifier state { name } } }',
      { id: identifier },
    );
    const i = data.issue;
    return i ? { id: i.id, identifier: i.identifier, state: i.state.name } : null;
  } catch (e) {
    // A commit can cite an issue that was deleted or belongs to another workspace.
    if (/not found|Entity not found/i.test(e.message)) return null;
    throw e;
  }
}

async function shippedStateId(apiKey, teamKey) {
  const data = await linear(
    apiKey,
    'query($key: String!, $name: String!) { workflowStates(filter: { team: { key: { eq: $key } }, name: { eq: $name } }) { nodes { id } } }',
    { key: teamKey, name: TO_STATE },
  );
  const id = data.workflowStates.nodes[0]?.id;
  if (!id) throw new Error(`Team ${teamKey} has no "${TO_STATE}" state.`);
  return id;
}

function parseArgs(argv) {
  const args = { tag: null, from: null, dryRun: false };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a === '--dry-run') args.dryRun = true;
    else if (a === '--from') args.from = argv[++i];
    else if (!args.tag) args.tag = a;
  }
  return args;
}

async function main() {
  const { tag, from, dryRun } = parseArgs(process.argv.slice(2));
  if (!tag) {
    process.stderr.write('Usage: node scripts/linear-ship.mjs <tag> [--from <ref>] [--dry-run]\n');
    process.exit(1);
  }
  const teamKey = process.env.LINEAR_TEAM_KEY || 'FRI';
  const apiKey = process.env.LINEAR_API_KEY;
  const repo = process.env.GITHUB_REPOSITORY || 'Mufdi/nergal';
  const url = process.env.RELEASE_URL || `https://github.com/${repo}/releases/tag/${tag}`;

  const base = from ?? previousTag(tag);
  const range = base ? `${base}..${tag}` : tag;
  const fromCommits = extractIssueIds(git(['log', '--format=%B', range]), teamKey);
  const handoffs = archivedTrackerFiles(range, tag);
  const fromArchive = extractTrackerIds(handoffs, teamKey);
  const ids = mergeIds(fromCommits, fromArchive);
  // The two counts are not decoration: "0 ids out of 4 handoff files" is a
  // missing `tracker` field, and "0 out of 0" is a release that archived
  // nothing. Without them a quiet run cannot be told from a broken one.
  process.stdout.write(
    `${range}: ${ids.length ? ids.join(', ') : 'no issue references'}` +
      ` (${fromCommits.length} from commits, ${fromArchive.length} from ${handoffs.length} archived changes)\n`,
  );
  if (ids.length === 0) return;

  if (!apiKey) {
    if (dryRun) {
      process.stdout.write('Dry run without LINEAR_API_KEY: states not checked.\n');
      return;
    }
    throw new Error('LINEAR_API_KEY is not set.');
  }

  const issues = [];
  for (const id of ids) {
    const issue = await fetchIssue(apiKey, id);
    if (issue) issues.push(issue);
    else process.stdout.write(`${id}: not found, skipped\n`);
  }
  const { ship, skip } = planTransitions(issues);
  for (const i of skip) process.stdout.write(`${i.identifier}: ${i.state}, left as is\n`);
  if (ship.length === 0) return;

  if (dryRun) {
    for (const i of ship) process.stdout.write(`${i.identifier}: would move ${FROM_STATE} → ${TO_STATE}\n`);
    return;
  }

  const stateId = await shippedStateId(apiKey, teamKey);
  const body = shippedComment(tag, url);
  for (const i of ship) {
    await linear(
      apiKey,
      'mutation($id: String!, $stateId: String!) { issueUpdate(id: $id, input: { stateId: $stateId }) { success } }',
      { id: i.id, stateId },
    );
    await linear(
      apiKey,
      'mutation($issueId: String!, $body: String!) { commentCreate(input: { issueId: $issueId, body: $body }) { success } }',
      { issueId: i.id, body },
    );
    process.stdout.write(`${i.identifier}: ${FROM_STATE} → ${TO_STATE}\n`);
  }
}

const isMain =
  !!process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url);
if (isMain) {
  main().catch((e) => {
    process.stderr.write(`linear-ship failed: ${e.message}\n`);
    process.exit(1);
  });
}
