# Implementation — db-lock-io-sweep

## Verified codebase facts (do not re-assume)

- `SharedDb = Arc<Mutex<Database>>`; commands take `db: State<'_, SharedDb>` and `let db = db.lock().map_err(|e| e.to_string())?`. The guard is a `MutexGuard<Database>` that drops at end of its lexical scope unless block-scoped or `drop()`-ed. (`src/commands/git.rs`, `ship_pr.rs` passim.)
- `resolve_session_cwd(&db, id) -> Result<PathBuf, String>`, `resolve_session_branch`, `resolve_session_base` are pure DB reads returning owned values (`src/commands/git.rs`, `ship_pr.rs`). Safe to call under a short block then drop.
- `MocBuilder::build(session_id, cfg, db: &Database) -> Result<Option<PathBuf>>` (`src/obsidian/moc.rs:24`): DB reads at lines 32-42 (`find_session`, `get_visible_tasks`, `get_annotations`), then file I/O at 44-94 (`extract_session_activity/model/files` read the session-log file; `create_dir_all` + `atomic_write` write the MOC). **No git subprocess** — the :230 comment is aspirational. The "nothing to show" early-return is at line 62.
- `BacklinkUpdater::propagate(&moc_path, &cfg)` (`src/obsidian/moc.rs:98+`): pure filesystem, no `&Database`.
- `startup_recover(db: &Database, stale_after_ms)` (`post_session.rs:323`) → `drain_inline(db: &Database)` (:234) → `drain(...)` (:245) → `process_marker(path, db: &Database)` (:289). `lib.rs:293` calls `startup_recover(&g, …)` with a **locked guard** `&g`, so the entire per-marker loop runs under one guard. There is no `&SharedDb` threaded through — Pattern C′ requires changing these signatures to take `&SharedDb` and passing the Arc from `lib.rs:293`. The detached `run()` path (`post_session.rs:220-224`) opens its own `Database::open()` (`&Database`) and calls `process_marker(path, &db)` — the signature flip breaks it, so `run()` must wrap: `let db = Arc::new(Mutex::new(Database::open()?));` and pass `&db` (single-threaded, so the extra Mutex is inert). `run()` + `drain_inline` are the only callers of `process_marker`; `lib.rs:294` + `drain_inline`(:328) are the only callers of `startup_recover`/`drain_inline`.
- `create_worktree(repo_path, slug)` (`worktree.rs:351-354`): **reuses on exists** — `if worktree_path.exists() { return Ok(worktree_path); }`. It does NOT error on a concurrent duplicate, so any "re-check on create_worktree error" logic never fires. The spawn trio must instead, after re-acquire and before `create_session`, check for an existing session bound to `worktree_path` and abort (without removing the shared worktree) if found.
- `create_session` (`workspaces_sessions.rs:247-269`): `is_first = session_count_for_workspace == 0`; worktree (slow work) is created **only** on the `!is_first && is_git_repo` branch. The `is_first`/non-git branch does no slow work — keep it atomic; only apply Pattern B to the worktree branch.
- `finalize_session_obsidian` (`hooks/server.rs:513`): `write_session_log_footer(&guard, …)` runs under the guard in **both** branches; the drop-asymmetry the plan targets is only the MOC block (514-529). Footer must be hoisted too.
- Offender inventory (guard held across I/O), 53 sites / 10 files, from the full-body enumeration:

  **Pattern A — simple-drop (no trailing DB write):**
  - `git.rs`: `init_git_repo`:9, `list_branches`:43, `get_file_diff`:68, `get_session_changed_files`:101, `get_session_git_info`:136 (5 chained git calls), `check_session_has_commits`:230, `get_git_status`:285, `git_stage_file`:331, `git_unstage_file`:342, `git_stage_all`:349, `git_unstage_all`:356, `git_commit`:367, `git_stash_list`:377, `git_stash_create`:388, `git_stash_apply`:399, `git_stash_pop`:410, `git_stash_drop`:421, `git_stash_show`:432, `git_stash_branch`:444, `get_recent_commits`:455, `pull_target_into_session`:493, `git_push`:500, `get_commit_files`:514 (direct `Command::new("git")`).
  - `ship_pr.rs`: `list_prs`:23, `get_pr_diff`:90, `get_pr_checks`:122, `gh_pr_merge`:149, `get_pr_status`:182, `complete_pending_merge`:254, `has_pending_merge`:261, `enable_pr_auto_merge`:272, `get_pr_preview_data`:287, `poll_pr_checks`:298.
  - `conflicts.rs`: `get_conflicted_files`:13, `get_file_conflict_versions`:24, `save_conflict_resolution`:36 (fs::write + stage_file + conflicted_files), `build_conflict_prompt`:54, `enqueue_conflict_context`:103.
  - `openspec.rs`: `list_openspec_changes`:118 (read_dir), `read_openspec_artifact`:201 (read_to_string), `write_openspec_artifact`:238 (create_dir_all + write).
  - `workspaces_sessions.rs`: `merge_session`:429 (squash_merge).

  **Pattern B — read → drop → I/O → re-acquire (trailing DB write / create+rollback):**
  - `git.rs`: `git_rename_branch`:309 (rename_current_branch → `db.update_worktree_branch`).
  - `workspaces_sessions.rs`: `create_session`:240 (create_worktree → `db.create_session`), `cleanup_merged_session`:502 (archive_plans fs::copy + remove_worktree + delete_branch → `db.delete_session`).
  - `clickup/mod.rs`: `clickup_spawn_worktree_with_task`:880 (create_worktree → `guard.create_session`).
  - `linear/mod.rs`: `linear_spawn_worktree_with_issue`:501 (create_worktree → `guard.create_session`).
  - `mcp/worktree_sessions.rs`: `build_worktree_session`:867 (create_worktree → `guard.create_session`; rollback remove_worktree).

  **Pattern C — gather → drop → loop-I/O (MOC/log-footer under lock in a per-N loop):**
  - `workspaces_sessions.rs`: `delete_workspace`:187 (per-session MocBuilder::build + BacklinkUpdater::propagate, then per-session remove_worktree, then `db.delete_workspace`), `delete_session`:345 (write_session_log_footer + MocBuilder::build/propagate, then `db.workspace_repo_path`).
  - `hooks/server.rs`: `finalize_session_obsidian`:499 (one branch already `drop(guard)`s before I/O; the other holds it — unify).
  - `lib.rs`: crash-recovery drain/process_marker:293 (fs::read_to_string + MocBuilder::build/propagate looped per marker under an outer guard), `queue_close_markers`:1026 (write_session_log_footer + MocBuilder::build/propagate looped per workspace/session).

## Execution order

1. **`obsidian/moc.rs` — split `MocBuilder::build`** into `gather(session_id, cfg, &db) -> Result<Option<MocInputs>>` (DB reads only, lines 32-42) + `MocInputs::render_and_write(&cfg) -> Result<Option<PathBuf>>` (log-file reads + `atomic_write`, lines 44-94, moving the "nothing to show" check into the write phase). Keep `build()` as a thin `gather()?.map_or(Ok(None), |i| i.render_and_write(cfg))` wrapper so non-loop callers are untouched. Inspect `write_session_log_footer` — if it writes a file under `&db`, give it the same gather/write split; if pure-DB, leave it.
2. **Pattern C callers** (depend on step 1): `delete_workspace`, `delete_session`, `finalize_session_obsidian` (also hoist the footer write), `queue_close_markers`. Gather all sessions' `MocInputs` (+ footer inputs + repo_path/worktree paths needed for the subsequent `remove_worktree`) under one guard, drop, then loop `render_and_write` + `propagate` + `remove_worktree` guard-free, re-acquiring only for the final `db.delete_*` write (re-validate existence).
2b. **Pattern C′ — `lib.rs:293` marker drain** (per-iteration lock): change `startup_recover`/`drain_inline`/`process_marker` to take `&SharedDb`; `lib.rs:293` passes the Arc `&db` (not `db.lock()`). Per marker: `read_to_string` guard-free → short `lock()` for `find_session` + `gather` → drop → `render_and_write` + `propagate` guard-free. Keep the `drain` test harness (`drain_count_excludes_failed_markers`, post_session.rs:472) green — adjust the injected `process` closure's captured handle accordingly.
3. **Pattern B sites**: block-extract inputs, drop, do I/O, re-acquire for the write; preserve rollback on the create/spawn trio.
4. **Pattern A sites** (bulk, mechanical): block-scope the guard around the `resolve_*` reads, drop before the subprocess/fs call.
5. **Verify**: full check + re-run the offender enumeration grep; the table must be empty.

## Per-section plan

- **moc.rs**: new `pub struct MocInputs { session: Session, tasks_done: usize, decisions: Vec<String> }` (own only what `render_moc` needs). `gather` returns `Ok(None)` when `moc_dir` empty or session missing. `render_and_write` does the log-file extraction + earned-MOC gate + `atomic_write`. Update the 5 callers.
- **git.rs / ship_pr.rs / conflicts.rs / openspec.rs**: Pattern A block-scoping. Watch the multi-value readers (`get_session_git_info` needs only cwd; `poll_pr_checks`/`get_pr_status` need cwd+branch). `save_conflict_resolution` (:36) does fs::write then stage_file then conflicted_files — extract cwd, drop, do all three guard-free.
- **workspaces_sessions.rs**: `create_session` (B — keep the `is_first`/non-git branch atomic; drop→create_worktree→re-acquire only on the worktree branch), `cleanup_merged_session` (B), `merge_session` (A), `delete_workspace` + `delete_session` (C).
- **clickup/mod.rs, linear/mod.rs, mcp/worktree_sessions.rs**: identical B shape (`create_worktree` → `create_session` + rollback).
- **hooks/server.rs, lib.rs**: C loops; `lib.rs` outer guard wraps a per-marker loop — gather marker→inputs first, drop, then loop.

## Per-phase risk

- **Step 1 (moc split)**: shared helper, 5 callers. Mitigation: `build()` wrapper preserves the old signature/behavior; only Pattern-C loops call the two-phase form. Compile-time enforced (write phase has no `&db`).
- **Step 2 (C loops)**: must snapshot repo_path + per-session worktree_path for `remove_worktree` before the drop. A missed field surfaces as a compile error, not a silent bug.
- **Step 3 (B)**: TOCTOU on re-acquire → re-validate + no-op; keep create/spawn rollback guard-free.
- **Step 4 (A)**: volume; a missed site is a perf regression, not incorrectness. Mitigation: reviewer re-runs enumeration.
