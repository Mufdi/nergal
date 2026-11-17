# Evidence: session-resume-by-id

## Diagnosis (2026-11-14, read-only)

- `reconcile_worktrees` (`src-tauri/src/lib.rs:1193`) demoted "Fixes generales", "Post en el foro para app" and "Upgrades de packages" on 2026-11-02 17:44:23Z (same `updated_at`), and "Upgrades" on 2026-11-12 20:16:20Z.
- The worktree directories were removed outside Nergal: `sudo rm -rf …/fixes-generales-1792165881` from the shell history (~2026-11-02 02:10Z); `git worktree remove … upgrades-1794247985 && git branch -D …` by the "General XD001" Claude Code session (2026-11-11T22:02Z).
- "Upgrades de packages" then recorded the main conversation (`-home-felipe-Projects-XD001/f3d85e80….jsonl`) as its transcript: `--continue` in the repo root picked the main conversation.

## Spike (2026-11-17)

`claude -p --resume <id>` launched from directory B restored a conversation created in directory A (answered with the word from turn 1) and appended to A's transcript file; B got no transcript. Throwaway dirs removed afterwards.

## Claude Code transcript retention (2026-11-17)

- `find ~/.claude/projects -maxdepth 2 -name '*.jsonl' -mtime +30` → 0: Claude Code's default `cleanupPeriodDays` (30) removed every transcript older than 30 days.
- Lost for good: "Post en el foro para app" (`b1e317be…`, last write 2026-10-16) and the older "Fixes generales" transcript (`3028a03a…`). No `session_summaries` row and no vault log for the first one.
- User decision: `cleanupPeriodDays: 90` written to `~/.claude/settings.json` (20 keys preserved).

## Group 4: data repair (2026-11-17)

- Nergal GUI not running (only the `nergal mcp` shim of the working Claude Code session, which holds no DB descriptor).
- Backup: `~/.config/nergal/nergal.db.bak-2026-11-17-session-repair` (3,334,144 bytes, SQLite backup API).
- One transaction, `PRAGMA foreign_keys=ON`, each update asserted `rowcount == 1`, each id asserted to exist once as `~/.claude/projects/*/<id>.jsonl`:

| Session | `agent_internal_session_id` set to |
|---|---|
| General XD001 | `10d0368d-bea0-4451-ae7a-823292e9dcae` |
| Burbuja flotante | `56051be2-df2b-4e9f-aa69-e81ad4faa87f` |
| Fixes generales | `b5f133dd-d199-4f1f-a7bb-c732e43a208d` |
| Upgrades de packages | `8278600b-4ad7-4c05-b497-9ea112cd8bac` (+ `session_transcripts` row pointed back to its own file) |
| Upgrades | `30cb1db2-8b3a-4918-9a0c-912e5f060bca` |

- "Post en el foro para app" (`e30eb7-1792167226`) deleted at the user's request; 0 child rows left.
- Orphan directory `XD001/.worktrees/nergal/feat-review-panel-checksum-1793587152` (1.2 GB, root-owned build caches, not a registered worktree, branch already gone) removed with `docker run … alpine find . -mindepth 1 -delete` + `rmdir`.
- The installed build already prefers the stored id (`pty.rs:767-774` → `--resume <id>`), so the repaired sessions resume their own conversation before this change ships.

## Baseline (main @ 440bfb7 + uncommitted docs/shortcuts.md, 2026-11-17)

| Gate | Result |
|---|---|
| `cargo clippy --all-targets -- -D warnings` | Finished, no warnings |
| `cargo test` | 925 passed, 0 failed |
| `cargo fmt --check` | exit 0 |
| `npx tsc --noEmit` | no errors |
| `pnpm test` | 23 files, 260 tests passed |

## iprev

- Round 1 (claude, opus): REVISE, 11 issues (1 critical, 1 high, 4 medium, 5 low). Critical: nested `claude` processes inherit `NERGAL_SESSION_ID` and would overwrite the stored id and the `Stop` transcript marker. All 11 addressed: D5 top-level filter (`agent_parent_pid` vs PTY shell pid), fallback only when no id is stored, `resume_candidates`/`pick_resume_id` split, transcript-grandparent projects dir, `worktree_sessions` query, `SessionStart`-only capture, pull-based notice (`take_reconciled_worktrees`), double-checked delete guard, `finalize` restore-on-error, `cc_resume_arg` seam test, softened sidebar wording.
- Round 2: REVISE, 6 issues (1 high: parent-pid rule marks every event nested behind a non-`exec` wrapper). Addressed: ancestor chain with run counting, legacy repo-root transcript only for the oldest session, `top_level` gate on all `Stop` identity writes, `parse_agent_chain`, D4 wording, per-pid sysinfo refresh.
- Round 3 (cap): REVISE, 5 issues (2 medium). Addressed: complete chain without the shell pid → nested (detached `tmux`/`setsid`), guard in `cleanup_merged_session`, race residue documented, `pstree` walk check, `agent_shell_pid` lock order. `iprev-cap` + `validated` events recorded.

## Red (test-writer, sonnet, 2026-11-17)

- 7 locked test files (`handoff/.red-tests`), hashes re-checked by the orchestrator: all `ok`.
- `cargo test --no-run`: 13 errors, every one a missing locked symbol (`conversation`, `cc_resume_arg`, `is_claude_process`, `AgentChain`, `parse_agent_chain`/`is_top_level_chain`/`record_cc_conversation_id`, `PtyManager::agent_shell_pid`, `Database::is_last_root_session` ×5) or `reconcile_worktrees` still returning `()` (E0308 ×2).
- vitest: `src/lib/sessionDelete.test.ts` fails on the missing `./sessionDelete` import.
- Not unit-covered (need an `AppHandle`/Tauri state): nested `Stop` leaving the transcript marker, `delete_session`/`cleanup_merged_session` refusals, `take_reconciled_worktrees`, `finalize` restore. Covered by the predicate tests plus manual walk 6.4–6.6. `agent_chain()` (live process walk) covered by walk 6.4.
- Adapter tests assert only when `claude` is on PATH (existing pattern); it is on PATH on the dev host.
- UI brief: no new view or component; the frontend work reuses the existing delete button, `toastsAtom` and `setupHookListeners`, so `/frontend-build` was not loaded.

## Green (Builder, sonnet, 2026-11-17)

Deviation: `Database::workspace_resume_peers` + `ResumePeer` added because `Database.conn` is private outside `db/`; no behavior change. Equal `created_at` values both count as oldest.

### Gates (orchestrator run)

| Gate | Result |
|---|---|
| `cargo clippy --all-targets -- -D warnings` | Finished, no warnings |
| `cargo test` | 966 passed, 0 failed, 1 ignored (baseline 925) |
| `cargo fmt --check` | exit 0 |
| `npx tsc --noEmit` | no errors |
| `pnpm test` | 24 files, 263 tests passed (baseline 260) |
| `work-gates.sh main session-resume-by-id` | 2 Red locks PASS (7 unchanged), 5 Deps SKIP, 6a/6b/6c PASS (gitleaks clean) |

Comment audit: 5 added comments, all WHY (why `continue` adds no flag ×2, why nested hooks must not write identity, why one pid per refresh, why the reconcile fetch tolerates failure).

## Reviews (B5, 2026-11-17)

- Adversarial (opus): FAIL, 6 Important (compliance/test gaps, main `cc-adapter` spec still naming `--continue`) + 5 nits. Frontend (sonnet): FAIL, 2 Important (refused delete left the row without tab/active session; row lost when `get_workspaces` also fails) + 5 nits. Consolidated in `handoff/REVIEW.md`.
- Spec fix: `specs/cc-adapter/spec.md` now MODIFIES "Zero regression for CC users post-refactor".

## Group 7: review fixes

- Red (test-writer, sonnet): 6 new locked files + 3 re-locked (13 total). `cargo test --no-run` failed only on `attach_agent_chain`, `cc_resume_args`, `record_stop_marker`, `ReconciledWorktrees::take`; vitest R-T6 failed on assertions (tab ids missing). R-T8 and R-T9 already passed (regression guards).
- Green (Builder, sonnet). Deviation: `reconcile_worktrees(&Database)` became `#[cfg(test)]`; startup uses `reconcile_shared_worktrees(&SharedDb)`, which composes the same `missing_worktrees` + `clear_missing` with the guard released during `stat` (accepted: the locked test covers the shared pieces).

### Gates (orchestrator run)

| Gate | Result |
|---|---|
| `cargo clippy --all-targets -- -D warnings` | no issues |
| `cargo test` | 976 passed, 0 failed, 1 ignored |
| `cargo fmt --check` | exit 0 |
| `npx tsc --noEmit` | no errors |
| `pnpm test` | 25 files, 266 tests passed |
| `work-gates.sh main session-resume-by-id` | Red locks PASS (13 unchanged), Deps SKIP, 6a/6b/6c PASS |

Comment audit: added comments are WHY-only (why `continue` adds no flag, why the marker path is guarded, why nested hooks must not write identity, why reconcile stats outside the guard, why one pid per refresh).

Pending: manual walk 6.3–6.6 on the installed `.deb`.
