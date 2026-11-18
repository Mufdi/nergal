# Evidence: session-delete-followups

## Origin (2026-11-17)

Manual walk of `session-resume-by-id` + `worktree-root-owned-cleanup` on the installed `.deb`: deleted worktree sessions left `nergal/<slug>` branches; a session kept after a failed reclaim launched Claude Code in a half-deleted worktree (trust prompt); that session started fresh although its transcript `…test-reclaim-no-image-1794953161/831de48c….jsonl` was intact (root cause: `freshSessionsAtom` never consumed).

## iprev (claude, opus)

Round 1 REVISE (14) → R2 REVISE (10; checked-out branch vs `update-ref -d`) → R3 REVISE (8; truncated runtime log as rollback source) → R4 REVISE (6; record written after the CAS) → R5 REVISE (5; record dir missing on fresh installs). Each round confirmed the previous findings closed. R5 findings applied; cap reached → user decision requested.

## Red (test-writer, sonnet)

- 4 locked files: `src-tauri/src/worktree_branch_tests.rs` (20 tests, unix-gated include), `src-tauri/src/commands/workspaces_sessions_notices_tests.rs` (9), `src/lib/launchMode.test.ts` (5), `src/stores/pendingDeletesNotices.test.ts` (4). Hashes re-checked by the orchestrator.
- `cargo test --no-run`: only missing locked symbols (`BranchOutcome`, `KeptReason`, `BranchState`, `branch_state`, `branch_checked_out`, `delete_branch_if_at`, `drop_session_branch`, `worktree_pending_delete`, `CleanupNotice`, `notices_after_removal`, `refuse_launch`). vitest: missing `./launchMode` and `noticeToast`.
- `refuse_launch`, `CleanupNotice` and `notices_after_removal` must live in `src-tauri/src/commands/workspaces_sessions.rs`.

## Green (Builder, sonnet)

- Implemented 1.2, 1.3, 2.2, 2.3, 2.4, 3.2, 3.3. Deviations: the removal `warn!` inside `spawn_blocking` lost its `session_id` field; the pending panel's Delete button repeats the sidebar confirm (`useSessionActions` is local to `Sidebar.tsx`); the pending UI applies to every agent.
- Rulings (orchestrator):
  - Adding `Session.worktree_pending_delete` forced one mechanical line (`worktree_pending_delete: false,`) in 5 locked test files of `session-resume-by-id` (diff verified: exactly that line each, no assertion touched) → re-locked in that change's `.red-tests`.
  - `a_compare_and_delete_refused_by_git_keeps_the_branch_as_moved` failed on this host because the global `core.hooksPath` bypasses repo hooks → test defect; the orchestrator set a repo-local `core.hooksPath` in that fixture and re-locked (`62392e82…`).

### Gates

| Gate | Result |
|---|---|
| `cargo clippy --all-targets -- -D warnings` | no issues |
| `cargo test` | 1049 passed, 0 failed, 1 ignored |
| `cargo fmt --check` | exit 0 |
| `npx tsc --noEmit` | no errors |
| `pnpm test` | 28 files, 279 passed |
| Red locks of the three changes | all match |
| `work-gates.sh a54f05f session-delete-followups` | Red locks PASS, Deps SKIP, 6a/6b/6c PASS |

## Reviews (B5)

- Adversarial + security (opus):
  - Adversarial: FAIL, 2 Important.
    1. A symref `refs/heads/nergal/x -> refs/heads/main` passes every check, and `update-ref -d` follows it and deletes `main`. The reviewer reproduced this on git 2.34.1.
    2. The manual root remedy (`rm -rf` under sudo) is unquoted in the pending panel and in `permission_remedy` (worktree-root-owned-cleanup). A path with spaces removes another path as root.
  - Security: PASS_WITH_NOTES. No lower-trust input reaches git as an option or a revision expression, and no shell is involved.
- Frontend (sonnet): PASS_WITH_NOTES, 3 Important.
  1. The confirm logic is duplicated between the sidebar and the panel.
  2. The panel shows the same unquoted remedy.
  3. The Delete button is not the project `Button` and gets no keyboard focus.
- Regression tests (written by the orchestrator, Red, locked):
  - `src-tauri/src/worktree_review_tests.rs`: the symref is kept and its target stays intact, the CAS uses `--no-deref`, `shell_quote`, and a quoted `permission_remedy`.
  - `src/lib/shellQuote.test.ts`.
  - Red result: `shell_quote` is missing and `./shellQuote` cannot be found.
- Fix round (Builder, sonnet):
  - Symref → `Unknown`/`Kept { Unchecked }`, and the CAS uses `--no-deref`.
  - A second in-use check runs right before the CAS. `common_dir` is inspected.
  - `shell_quote` is used in `permission_remedy`, and `posixQuote` in the panel.
  - Both call sites share `confirmAndDeleteSession`.
  - The panel uses the destructive `Button` and gets focus in the terminal zone.
  - The amber color became the `yellow-500` warning token. The row keeps the GitBranch icon and gains an accessible "Worktree partly deleted" name.
  - `CleanupNotice.kind` is a union type.
  - `session_id` and `repo` are restored in the logs.
- Gates after the fix round:
  - clippy: clean.
  - `cargo test`: 1053 passed, 0 failed.
  - fmt: exit 0.
  - tsc: clean.
  - `pnpm test`: 281 passed.
  - All 22 Red locks of the three changes match.
- Walk 2026-11-18 (installed `.deb` 20:12):
  - C1, C2, C4 and C5 passed.
  - C3 failed. A failed `git worktree remove --force` deletes `<repo>/.git/worktrees/<name>` but keeps the worktree's `.git` file. This was reproduced in a temp repo with a non-writable subdirectory. The `.git exists` rule missed that state, the agent launched, and Claude Code asked for trust because the worktree no longer mapped to XD001. History resumed correctly.
  - Fix (D3 revised): the rule now follows `gitdir:` to a directory with `HEAD`. Three locked "healthy" fixtures used `gitdir: /x`, which is exactly the broken state. The orchestrator moved them to a real admin dir and added the dropped-registration cases, then re-locked.
- Session "test" (`e30eb7-1794952210`) lost its conversation link (D6):
  - The user removed its worktree by hand at about 21:55 on 2026-11-17. Its own transcript shows that. The startup reconcile then demoted it to the repo root, as designed.
  - At 02:59:11 Claude Code wrote `continued-in` from `e3c759bf` to `18e18c76`, with no `SessionStart`.
  - At 03:02:41 a fresh `b2be2d83` started. `session-env` shows it, and `history.jsonl` has no prompt for it. That id was recorded, and its transcript never existed.
  - The real conversation is `~/.claude/projects/-home-felipe-Projects-XD001/18e18c76-0e4e-4831-9830-f4e83cbe2040.jsonl`, ending 03:00:19.
  - Experiment: `claude -p --resume` across directories keeps the same id (haiku, temp dirs), so it does not explain the interactive continuation.
  - Fix: `pick_resume_id` follows `continued-in`. Red tests are in `conversation_continuation_tests.rs`, where 2 of 5 fail as expected.
- Data repair 2026-11-18, with Nergal closed:
  - Backup: `nergal.db.bak-2026-11-18-test-relink`.
  - Session "test": the link moved from `c9932172` to `18e18c76`. `c9932172` was another empty fresh start, created at 03:06 because the stored `b2be2d83` had no transcript.
  - An audit of every Claude Code session found no other missing transcript and no pending continuation.
- Walk after the fixes (`.deb` 00:41): D1 ("test" resumes its conversation), D2 (C3 repeated: pending marker, panel and no trust prompt) and D3 (delete from the panel) passed, as the user reported.
- Accepted residue:
  - A detached repo-root `HEAD` counts as a holder. This is design D1, and a locked test covers it.
  - `merge_target` is not validated as a revision expression. Its setter has no callers.
  - The record is TSV without escaping. The same user chooses the repo path.
  - Test fixtures inherit the global git config. The only test that broke because of it is fixed.
