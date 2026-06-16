# Review — linear-mirror

## Pre-build: iterative-plan-review (Claude evaluator, skeptic persona)

5 rounds, converged to **APPROVED**. Full transcript in `iprev-review-full.md`.

| Round | Verdict | Findings |
|---|---|---|
| 1 | REVISE | 20 (2 critical: truncated-fetch false-tombstone, `parent_id` FK abort; 8 high; 9 medium; 1 low) |
| 2 | REVISE | all 20 fixed; 9 new on the set-3 re-verify-mine mechanism (cross-team churn, over-fetch, query gaps) |
| 3 | REVISE | all r2 fixed; 6 new (viewer-resolve-on-failure wipe, set-3 absence-vs-404, by-id pagination, eviction completeness) |
| 4 | REVISE | all r3 fixed; 5 new from the immediate-`set_key`-wipe (account-swap race, dead backstop, set-3 4th outcome + label-strip, team-selection reset) |
| 5 | **APPROVED** | key-generation epoch closes the swap race structurally; residuals LOW (BEGIN IMMEDIATE note, notify-after-commit) — both folded into tasks.md |

Key design hardening the review forced (all in design.md / specs / tasks):
- Completeness-gated tombstoning (no tombstones on an interrupted fetch).
- `parent_id` as a plain column (no self-FK abort / cascade-wipe).
- Set-3 delta re-verify (un-assignment detection independent of `updatedAt`), chunked+paginated, full-relation, presence-keyed outcomes gated on completeness.
- Age-out eviction distinct from tombstoning, inside the txn.
- Key-generation epoch for account-swap isolation (commit discarded on generation change; `BEGIN IMMEDIATE`).
- `RATELIMITED`-on-400 + exhausted-bucket backoff + `[1s,60s]` clamp + hard-complexity distinct.
- Inline-image gate + `validate_url` on untrusted URL opens.

## Build-phase reviewers

_Single-sequential + security escalation (auth + untrusted-content + same-uid boundary) write findings here during implementation._
