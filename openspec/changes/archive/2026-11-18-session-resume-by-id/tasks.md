## 1. Resume by conversation id (backend)

- [x] 1.1 Test (Red): `conversation.rs` — `is_valid_conversation_id` (uuid ok; empty, `../x`, `a/b` rejected); `resume_candidates` (`claimed_by_older` for a shared transcript and for an older session whose id equals the stem; unknown session → `None`); `pick_resume_id` (stored id with file; stored id without file → `None` even when the transcript exists; legacy stem with file; `claimed_by_older` → `None`; legacy repo-root transcript for a non-oldest session → `None`, for the oldest → the stem; invalid id → `None`); `project_dir_name("/home/u/00Custom Mods/X.y")` → `-home-u-00Custom-Mods-X-y`; `projects_dirs` adds the transcript grandparent — scenarios of `session-continuity` "Claude Code sessions resume their own conversation"
- [x] 1.2 Test (Red): `pty.rs` `cc_resume_arg` (`"continue"` + picked → id; `"continue"` + none → `None`; `"resume_pick"` and explicit ids pass through) and `PtyManager::agent_shell_pid` for an unknown session → `None`
- [x] 1.3 Test (Red): `adapter.rs` — `"continue"` yields neither `--continue` nor `--resume`; `"resume_pick"` yields `--resume` alone; an id yields `--resume <id>` (replaces the assertion at `adapter.rs:417-419`)
- [x] 1.4 Create `src-tauri/src/agents/claude_code/conversation.rs` with `is_valid_conversation_id`, `ResumeCandidates`, `resume_candidates`, `pick_resume_id`, `projects_dirs`; register it in `src-tauri/src/agents/claude_code/mod.rs`
- [x] 1.5 Add `cc_resume_arg` in `src-tauri/src/pty.rs`; use it with `resume_candidates`/`pick_resume_id` at `pty.rs:767` for Claude Code sessions (DB reads under the lock, file checks after it)
- [x] 1.6 `src-tauri/src/agents/claude_code/adapter.rs:169`: map `"continue"` to no flag

## 2. Top-level hook identity

- [x] 2.1 Test (Red): `platform_proc::is_claude_process` (name `claude`; exe under a `claude` dir; node with `cmd[1]` ending in `/claude`; `zsh` and `node` running another script rejected)
- [x] 2.2 Test (Red): `parse_agent_chain` (valid; string pid, negative pid, pid above `u32::MAX`, non-boolean `claude`, non-array → `None`) and `is_top_level_chain` (direct child, bash wrapper, npx→node, nested via tool shell, complete chain without the shell pid → nested, truncated chain without the shell pid → top-level, chain absent, shell pid unknown) and `record_cc_conversation_id` (SessionStart records; new id overwrites; Stop does not write; Pi session untouched; invalid id ignored) — scenarios of "Only the top-level agent process writes session identity" and "Claude Code conversation id is recorded per session"
- [x] 2.3 Add `is_claude_process`, `AgentChain` and `agent_chain` in `src-tauri/src/platform_proc.rs` (per-pid refresh, `complete` flag)
- [x] 2.4 `src-tauri/src/hooks/cli.rs` `send_hook_event`: add `agent_chain` and `agent_chain_complete` for `SessionStart` and `Stop` only
- [x] 2.6 Add `PtyManager::agent_shell_pid` (release `session_ptys` before locking `instances`)
- [x] 2.5 `src-tauri/src/hooks/server.rs`: `parse_agent_chain`, `is_top_level_chain`, `record_cc_conversation_id`, `top_level` parameter on `process_event` (other call sites pass `true`); the `Stop` pull-marker, `set_session_background` and `set_session_last_message` run only when `top_level`

## 3. Lost worktree notice

- [x] 3.1 Test (Red): `reconcile_worktrees` returns the names of demoted sessions in `created_at` order, clears `worktree_path`/`worktree_branch`, keeps `agent_internal_session_id`; returns `[]` and changes nothing when every worktree exists
- [x] 3.2 Add `Database::worktree_sessions` in `src-tauri/src/db/sessions.rs`; make `reconcile_worktrees` (`src-tauri/src/lib.rs`) return `Vec<String>` of the rows it cleared
- [x] 3.3 Add `ReconciledWorktrees` managed state and the `take_reconciled_worktrees` command; register it in the invoke handler
- [x] 3.4 `src/stores/hooks.ts` `setupHookListeners`: call `take_reconciled_worktrees` and show one toast when the list is non-empty

## 4. Delete guard for the last repo-root session

- [x] 4.1 Test (Red): `Database::is_last_root_session` — only root with a worktree sibling → true; one of two roots → false; worktree session → false; unknown id → false
- [x] 4.2 Test (Red): `canDeleteSession` in `src/lib/sessionDelete.test.ts` — sidebar scenarios of "The last repo-root session of a workspace cannot be deleted"
- [x] 4.3 Add `Database::is_last_root_session` in `src-tauri/src/db/sessions.rs`
- [x] 4.4 `src-tauri/src/commands/workspaces_sessions.rs` `delete_session`: refuse before `stop_event_pump`, at the start of the first lock scope, and in the final lock scope
- [x] 4.7 `cleanup_merged_session`: refuse the last repo-root session before any side effect
- [x] 4.5 Add `src/lib/sessionDelete.ts`; use it in `src/components/session/SessionRow.tsx:195` and the collapsed popover in `src/components/layout/Sidebar.tsx:300`
- [x] 4.6 `src/stores/pendingDeletes.ts` `finalize`: on a refused delete, reload workspaces and show an error toast (the terminal stays destroyed until the session is selected again)

## 7. Review fixes (`handoff/REVIEW.md`)

- [x] 7.1 Test (Red): R-T1 `cc_resume_args(resume_from: Option<&str>) -> Vec<String>` in `adapter.rs` (`None` → `[]`; `"continue"` → `[]`; `"resume_pick"` → `["--resume"]`; `"aaaa-1111"` → `["--resume", "aaaa-1111"]`), no `which`
- [x] 7.2 Test (Red): R-T2 `record_stop_marker(db: &Database, session_id: &str, transcript_path: Option<&str>, top_level: bool)` in `hooks/server.rs` (top-level moves the marker; nested does not; a non-`.jsonl` path is ignored)
- [x] 7.3 Test (Red): R-T3 `attach_agent_chain(payload: &mut serde_json::Value, chain: &AgentChain)` in `hooks/cli.rs` (adds `agent_chain` + `agent_chain_complete` for `SessionStart` and `Stop`; leaves `PreToolUse` untouched); R-T4 `agent_chain()` from the test process is `complete` and its first entry is the parent pid
- [x] 7.4 Test (Red): R-T5 `ReconciledWorktrees::take()` returns the names once, then `[]`; R-T9 `refuse_last_root_session(db: &Database, session_id: &str) -> Result<(), String>` refuses the only repo-root session and leaves the row, accepts one of two
- [x] 7.5 Test (Red): R-T7 `is_valid_conversation_id` rejects `-x` and `--resume`; R-T8 `canDeleteSession` is false for a workspace with one repo-root session and nothing else; R-T6 refused `delete_session` in `pendingDeletes.ts` restores the row, the tab and the active session, and shows one error toast
- [x] 7.6 R-F1 `finalize` restore path; R-F2 toast copy; R-F3 remove the restating comment (`src/stores/`)
- [x] 7.7 R-F4 final-scope recheck in `cleanup_merged_session`; R-F5 gate `drain_idle`/`drain_worktree_outcomes` on `top_level`; R-F6 reject a leading `-` in conversation ids; R-F7 `reconcile_worktrees` stats paths outside the DB guard; R-F8 fix the stale comments
- [x] 7.8 Extract and use the helpers of 7.1–7.4 (`cc_resume_args`, `record_stop_marker`, `attach_agent_chain`, `ReconciledWorktrees::take`, `pub(crate) refuse_last_root_session`)

## 5. Data repair (one-off, outside the codebase)

- [x] 5.1 Back up `~/.config/nergal/nergal.db` before any write
- [x] 5.2 Set `agent_internal_session_id` of the XD001 sessions to their own transcript ids; point the `session_transcripts` row of "Upgrades de packages" back to its own transcript
- [x] 5.3 Delete "Post en el foro para app" (its transcript was removed by Claude Code's 30-day retention)
- [x] 5.4 Remove the orphan directory `XD001/.worktrees/nergal/feat-review-panel-checksum-1793587152` (branch already gone)
- [x] 5.5 Record the before/after rows in `handoff/evidence.md`

## 6. Verification

- [x] 6.1 `cd src-tauri && cargo clippy --all-targets -- -D warnings && cargo test && cargo fmt --check`
- [x] 6.2 `npx tsc --noEmit && pnpm test`
- [x] 6.3 Manual walk on the installed `.deb`: "Fixes generales", "Upgrades de packages" and "Upgrades" each reopen their own conversation
- [x] 6.4 Manual walk: `claude -p "hi"` run inside a session does not change that session's stored id, transcript marker or last message; `time` the hook on a `Stop`; record a `pstree -p` with subagents and background tasks running and confirm no `claude` is a direct child of another `claude`
- [x] 6.5 Manual walk: a new XD001 session gets a worktree; "General XD001" shows no delete action; a demoted session shows one and deletes
- [x] 6.6 Manual walk: deleting a worktree directory and restarting Nergal shows the `Worktree no longer exists` toast and the session keeps its conversation
