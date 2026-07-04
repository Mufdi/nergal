## 1. MocBuilder split (enabler)

- [ ] 1.1 Add `MocInputs` struct + `MocBuilder::gather(session_id, cfg, &db) -> Result<Option<MocInputs>>` (DB-read phase, moc.rs lines 32-42) and `MocInputs::render_and_write(&cfg) -> Result<Option<PathBuf>>` (file phase, lines 44-94, including the earned-MOC gate)
- [ ] 1.2 Keep `MocBuilder::build` as a thin wrapper delegating to gather + render_and_write; confirm non-loop callers compile unchanged
- [ ] 1.3 Inspect `write_session_log_footer`; split into gather/write if it writes a file under `&db`, else leave

## 2. Pattern C — gather → drop → loop-I/O

- [ ] 2.1 `delete_workspace` (workspaces_sessions.rs:187): gather MocInputs + repo_path + per-session worktree paths under one guard, drop, loop render_and_write + propagate + remove_worktree, re-acquire for `db.delete_workspace`
- [ ] 2.2 `delete_session` (workspaces_sessions.rs:345): same shape; re-acquire only for the trailing DB read/write, re-validate existence
- [ ] 2.3 `finalize_session_obsidian` (hooks/server.rs:499): unify both branches onto the drop-before-I/O path AND hoist the `write_session_log_footer` (:513) write out of the guard (gather footer inputs first)
- [ ] 2.4 `lib.rs` `queue_close_markers` (:1026): same per-workspace/session gather-then-loop

## 2b. Pattern C′ — per-iteration lock (marker drain)

- [ ] 2b.1 Change `startup_recover`/`drain_inline`/`process_marker` (post_session.rs) to take `&SharedDb`; `lib.rs:293` passes the Arc `&db`, not `db.lock()`
- [ ] 2b.2 In `process_marker`: read marker guard-free, short-lock for `find_session` + `MocBuilder::gather`, drop, then `render_and_write` + `propagate` guard-free
- [ ] 2b.3 Confirm `drain_count_excludes_failed_markers` (post_session.rs:472) stays green (drain signature unchanged; test uses a DB-free closure — expected no-op)
- [ ] 2b.4 Update the detached `run()` path (post_session.rs:220-224): wrap its connection as `Arc::new(Mutex::new(Database::open().context("opening database for post-session runner")?))` (preserve the existing `.context`) and pass `&db` (SharedDb) to `drain`. Also at lib.rs:294 pass the Arc `&db` and REMOVE the `match db.lock() { Ok(g) => …(&g) }` wrapper — keeping the locked guard would deadlock (std Mutex is non-reentrant; process_marker re-locks per marker)

## 3. Pattern B — read → drop → I/O → re-acquire

- [ ] 3.1 `git_rename_branch` (git.rs:309): extract, drop, rename, re-acquire for `update_worktree_branch` (no-op if row gone)
- [ ] 3.2 `create_session` (workspaces_sessions.rs:240): keep the `is_first`/non-git branch fully atomic (no slow work); only on the worktree branch drop → create_worktree → re-acquire for create_session
- [ ] 3.3 `cleanup_merged_session` (workspaces_sessions.rs:502): extract, drop, archive_plans + remove_worktree + delete_branch, re-acquire for delete_session
- [ ] 3.4 `clickup_spawn_worktree_with_task` (clickup/mod.rs:880): extract, drop, create_worktree, re-acquire; before create_session re-check no existing session targets this worktree_path (create_worktree reuses on-exists, so it won't error on a dup — abort without removing the shared worktree if found); guard-free rollback on create failure
- [ ] 3.5 `linear_spawn_worktree_with_issue` (linear/mod.rs:501): same shape incl. the pre-create dup-worktree re-check
- [ ] 3.6 `build_worktree_session` (mcp/worktree_sessions.rs:867): same shape incl. rollback remove_worktree + the pre-create dup-worktree re-check

## 4. Pattern A — simple-drop (bulk)

- [ ] 4.1 git.rs: block-scope all 23 sites (init_git_repo, list_branches, get_file_diff, get_session_changed_files, get_session_git_info, check_session_has_commits, get_git_status, git_stage/unstage_file/all, git_commit, git_stash_* ×8, get_recent_commits, pull_target_into_session, git_push, get_commit_files)
- [ ] 4.2 ship_pr.rs: block-scope all 10 sites (list_prs, get_pr_diff, get_pr_checks, gh_pr_merge, get_pr_status, complete_pending_merge, has_pending_merge, enable_pr_auto_merge, get_pr_preview_data, poll_pr_checks)
- [ ] 4.3 conflicts.rs: block-scope 5 sites (get_conflicted_files, get_file_conflict_versions, save_conflict_resolution, build_conflict_prompt, enqueue_conflict_context)
- [ ] 4.4 openspec.rs: block-scope 3 sites (list_openspec_changes, read_openspec_artifact, write_openspec_artifact)
- [ ] 4.5 workspaces_sessions.rs: block-scope merge_session:429

## 5. Verify

- [ ] 5.1 `cargo clippy --all-targets -- -D warnings` + `cargo test` + `cargo fmt --check` green
- [ ] 5.2 `tsc --noEmit` green (no frontend change expected)
- [ ] 5.3 Re-run the offender enumeration (grep each `db.lock()` site's fn for a `worktree::`/`Command::new`/`atomic_write`/`fs::` call under a live guard) — table must be empty
