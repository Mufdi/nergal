# REVIEW — release-db-lock-during-io

## Reviewer: code-quality + concurrency + spec lens (single, sonnet) · 2026-07-03

**Verdict: PASS** — zero blocking findings.

Verified (read, not just trusted the lint):
- Guard scope ends before slow work in all three: `delete_session` (scoped
  `worktree_cleanup` block drops before `remove_worktree`; fresh guard for finalize),
  `create_pr` (guard block drops before `list_branches`/`create_pr`), `git_ship` (guard
  block drops before `current_branch`/`list_branches`/`ship`).
- **No MutexGuard across `.await`**: the only await (`stop_event_pump`) precedes every
  `db.lock()`. Ran clippy with `-W clippy::await_holding_lock` FORCED on (not in the
  default set) — clean.
- TOCTOU finalize correct: fresh guard → `find_session` re-validate → `is_none() → Ok`
  BEFORE `forget_session`/`delete_session`. Double-delete race = the documented+accepted
  D4 risk (idempotent SQL, logged remove_worktree failure).
- `git_ship` inline branch/base logic compared line-by-line vs the shared
  `resolve_session_branch`/`resolve_session_base` helpers — semantically identical, one
  fewer DB lookup. Not a behavior change.
- `base`-after-guard-drop divergence from tasks.md wording is correct (base derives from
  `list_branches`, which is git — can't be "extracted under guard").
- All spec scenarios hold.

## Informational findings (non-blocking, actioned)

1. **MocBuilder::build does NO git I/O** (reviewer read `obsidian/moc.rs`): it parses the
   session log file (`extract_session_files`), a deliberate choice OVER a git diff
   (moc.rs comment). So design.md's D2/task 1.2 premise ("MOC does a worktree git diff"
   is the slow piece to hoist) is stale — the optional D2b split is **moot**: keeping the
   Obsidian block under the guard is file+DB I/O only, no lock-during-git violation.
   `delete_session` is already fully clean (its only git op, `remove_worktree`, is
   hoisted). → task 1.2 dropped as unnecessary (annotated, not silently ticked).
2. **`delete_workspace` is the worst remaining offender** (MocBuilder + remove_worktree
   in a LOOP over every session in the workspace, guard held throughout) — worse blast
   radius than the three fixed here. → named as highest-priority follow-up. Full sweep
   list (task 4.1): delete_workspace, git_push, get_pr_preview_data, poll_pr_checks
   (polled → repeat offender), enable_pr_auto_merge, list_prs, get_pr_diff, get_pr_checks,
   gh_pr_merge, merge_session.

## Gates

- Gate 1-3: PASS (clippy -D warnings clean incl. forced await_holding_lock, 760 tests,
  fmt clean).
- Gate 6 (scope): 1 file = files_estimate 1.
