# Implementation Plan: session-resume-by-id

> Grounded in the current codebase, symbols verified 2026-11-17. Behaviour (not just symbol existence) verified for the load-bearing claims below. Revised after iprev round 1.

## Verified codebase facts (do not re-assume)

- `src-tauri/src/pty.rs:660` `start_claude_session(app, state, agents, db, session_id, cwd, cols, rows, resume: Option<String>)`. The frontend passes `resume = "continue"` for every reopen (`src/components/terminal/terminalService.ts:241`: `mode === "new" ? null : mode`; `src/components/terminal/TerminalManager.tsx:45-52`).
- `src-tauri/src/pty.rs:767-774`: for `Some("continue")` it reads `agent_internal_session_id` and falls back to the literal `"continue"`. The agent id is resolved at `pty.rs:733-749` (`agent_id: AgentId`); `crate::agents::AgentId::claude_code()` exists.
- `src-tauri/src/pty.rs:457-459`: `NERGAL_SESSION_ID` is set on the PTY shell env, so every descendant inherits it. `pty.rs:462` keeps the shell pid as `child_pid`; `PtyInstance.child_pid: Option<u32>` (`pty.rs:55`); `PtyManager.session_ptys: Mutex<HashMap<session_id, pty_id>>` (`pty.rs:121`) and `instances: Mutex<HashMap<pty_id, PtyInstance>>` (`pty.rs:117`).
- `src-tauri/src/agents/claude_code/adapter.rs:167-176`: `None` → no flag; `"continue"` → `--continue`; `"resume_pick"` → `--resume`; other → `--resume <id>`. Test `adapter.rs:417-419` asserts `--continue`.
- The CC adapter keeps the trait default `agent_internal_session_id() -> None` (`src-tauri/src/agents/mod.rs:637`); the poller at `pty.rs:865-895` never writes for CC.
- `src-tauri/src/db/sessions.rs:116` `update_agent_internal_session_id(id, internal_id)`; `sessions.rs:152` `sessions_with_worktrees() -> Vec<(id, PathBuf)>` (no name); `sessions.rs:180` `clear_session_worktree` clears `worktree_path`, `worktree_branch`, `status`, `updated_at` only.
- `src-tauri/src/db/transcripts.rs:52` `get_session_transcript(sid) -> Option<SessionTranscript { transcript_path, last_stop_at }>`; written on every `Stop` at `src-tauri/src/hooks/server.rs:806-842`.
- `src-tauri/src/hooks/cli.rs:26` `send_hook_event` reads stdin JSON, inserts `nergal_session_id` from env, writes one line to the socket, exits. The payload carries `hook_event_name`.
- `src-tauri/src/hooks/server.rs:391-410`: the socket loop parses the line, extracts `nergal_session_id` from the raw JSON, then calls `process_event(&app, &event, &db, &plan_state, &agent_state, nergal_sid)` (`server.rs:783`). `process_event` drops events without `nergal_session_id` (`server.rs:794-800`).
- `src-tauri/src/platform_proc.rs:447` `ancestor_env` walks ancestors with `sysinfo` (`System::refresh_processes_specifics`, `process.parent()`); the module already compiles on all targets.
- `src-tauri/src/lib.rs:1193` `reconcile_worktrees(db: &Database)`, called at `lib.rs:325-327` before `tauri::Builder`. Startup values reach the frontend through managed state or delayed emits (`lib.rs:1056-1065`).
- `src-tauri/src/commands/workspaces_sessions.rs:441` `delete_session`: `stop_event_pump` first, then one lock scope that reads the session, then `remove_worktree` unlocked, then a final lock scope that re-validates and calls `db.delete_session` (`workspaces_sessions.rs:530-540`).
- `src/stores/pendingDeletes.ts:130-140` `finalize`: destroys the terminal and prunes state, then `invoke("delete_session").catch(() => undefined)` — no restore on error.
- `src/stores/hooks.ts:79` `setupHookListeners(store)` registers the app-level listeners.
- `src/components/session/SessionRow.tsx:195` and `src/components/layout/Sidebar.tsx:300` gate delete on `worktree_path !== null`; `SessionRow` receives `workspace` (`Sidebar.tsx:887-895`).
- `Database::open_in_memory()` (`src-tauri/src/db/mod.rs:197`, `cfg(test)`) gives a migrated DB for tests outside `db/`.
- Claude Code transcripts live at `<config>/projects/<cwd with non-alphanumerics → '-'>/<conversation id>.jsonl`. `claude --resume <id>` from another cwd restores the conversation (experiment, 2026-11-17). The live process tree shows the top-level `claude` as a direct child of the terminal shell.
- Frontend tests: vitest, colocated `*.test.ts`, `pnpm test`.

## Locked interfaces (tests are written against these)

`src-tauri/src/agents/claude_code/conversation.rs` (new, registered in `agents/claude_code/mod.rs`):

- `pub fn is_valid_conversation_id(id: &str) -> bool`: non-empty, only `[A-Za-z0-9-]`.
- `pub fn project_dir_name(cwd: &Path) -> String`: Claude Code's project directory name (every char outside `[A-Za-z0-9]` becomes `-`).
- `pub struct ResumeCandidates { pub stored_id: Option<String>, pub transcript_path: Option<PathBuf>, pub claimed_by_older: bool, pub is_oldest_in_workspace: bool, pub repo_root: PathBuf }`.
- `pub fn resume_candidates(db: &Database, session_id: &str) -> Option<ResumeCandidates>`: DB reads only. `claimed_by_older` is true when an older session (smaller `created_at`) of the same workspace has the same `transcript_path`, or an `agent_internal_session_id` equal to the transcript stem. `repo_root` is the workspace `repo_path`. `None` for an unknown session.
- `pub fn pick_resume_id(c: &ResumeCandidates, projects_dirs: &[PathBuf]) -> Option<String>`: no DB. Stored id set → that id if valid and `<dir>/*/<id>.jsonl` exists for some dir, else `None`. Stored id unset → transcript stem if valid, the file exists, not `claimed_by_older`, and not (the transcript's parent dir name equals `project_dir_name(repo_root)` while `!is_oldest_in_workspace`).
- `pub fn projects_dirs(transcript_path: Option<&Path>) -> Vec<PathBuf>`: `$CLAUDE_CONFIG_DIR/projects` when set and non-empty, else `<home>/.claude/projects`; plus the grandparent of `transcript_path` when given and different.

`src-tauri/src/platform_proc.rs`:

- `pub fn is_claude_process(name: &str, exe: Option<&Path>, cmd: &[String]) -> bool`: `name == "claude"`, or `exe` has a path component equal to `claude`, or `cmd[0]`/`cmd[1]` ends with `/claude` or contains `claude-code`.
- `pub struct AgentChain { pub entries: Vec<(u32, bool)>, pub complete: bool }`.
- `pub fn agent_chain() -> AgentChain`: the current process's ancestors from its parent upward (max 16), each `(pid, is_claude_process(..))`. `complete` is true when the walk reached pid 1 or a process without a parent; false when it hit the depth cap or a pid it could not read. Refreshes one pid per step with `ProcessesToUpdate::Some`, never all processes.

`src-tauri/src/hooks/`:

- `cli.rs` `send_hook_event`: for `hook_event_name` in {`SessionStart`, `Stop`} only, insert `agent_chain: [{"pid": n, "claude": b}]` and `agent_chain_complete: bool` from `agent_chain()`.
- `server.rs` `pub(crate) fn parse_agent_chain(raw: &serde_json::Value) -> Option<AgentChain>`: `None` unless `agent_chain` is an array whose every item has an integer `pid` within `u32` and a boolean `claude`; `complete` = `agent_chain_complete` when it is a boolean, else `false`.
- `server.rs` `pub(crate) fn is_top_level_chain(chain: Option<&AgentChain>, shell_pid: Option<u32>) -> bool`: chain `None` or shell pid `None` → `true`. Shell pid in the chain → `true` iff the entries before it hold fewer than two runs of consecutive `claude == true`. Shell pid absent → `!chain.complete`.
- `server.rs` `pub(crate) fn record_cc_conversation_id(db: &Database, nergal_session_id: &str, event: &HookEvent)`: only `SessionStart`; only when the row's `agent_id == "claude-code"` and the id differs; validates with `is_valid_conversation_id`.
- `process_event` gains a `top_level: bool` parameter. `record_cc_conversation_id`, the `Stop` pull-marker, `set_session_background` and `set_session_last_message` (`server.rs:815-842`) run only when `top_level`.

`src-tauri/src/pty.rs`:

- `impl PtyManager { pub fn agent_shell_pid(&self, session_id: &str) -> Option<u32> }` via `session_ptys` → `instances[..].child_pid`. Lock order: clone the pty id out of `session_ptys` and release it before locking `instances` (existing code takes them in that order, `pty.rs:1193-1194`; the hook server calls this on every `SessionStart`/`Stop`).
- `pub(crate) fn cc_resume_arg(resume: Option<&str>, picked: Option<String>) -> Option<String>`: `Some("continue")` → `picked`; any other `resume` passes through unchanged. Used only for Claude Code sessions.

`src-tauri/src/lib.rs` + `src-tauri/src/db/sessions.rs`:

- `Database::worktree_sessions() -> Result<Vec<(String /*id*/, String /*name*/, PathBuf)>>` ordered by `created_at` (replaces the use of `sessions_with_worktrees` in reconcile).
- `fn reconcile_worktrees(db: &Database) -> Vec<String>`: names of sessions whose `clear_session_worktree` returned `Ok`.
- `pub struct ReconciledWorktrees(Mutex<Vec<String>>)` managed state + `#[tauri::command] fn take_reconciled_worktrees(state) -> Vec<String>` (returns and empties).
- `Database::is_last_root_session(&self, session_id: &str) -> Result<bool>`: true when the row has `worktree_path IS NULL` and no other session of its workspace does. False for an unknown id.

Frontend:

- `src/lib/sessionDelete.ts`: `export function canDeleteSession(session: Session, workspace: Workspace): boolean`.

## Execution order

1. `conversation.rs`, `platform_proc` helpers, `PtyManager::agent_shell_pid`, `cc_resume_arg`.
2. Spawn path (`pty.rs:767`) + adapter `"continue"` arm.
3. Hook CLI `agent_chain`; server `parse_agent_chain`, `is_top_level_chain`, `record_cc_conversation_id`, `process_event(top_level)`, guarded `Stop` identity writes.
4. `worktree_sessions`, `reconcile_worktrees`, `ReconciledWorktrees` + command (register in the `invoke_handler`).
5. `is_last_root_session` + `delete_session` guard (early + final scope).
6. Frontend: `canDeleteSession`, `SessionRow`, collapsed popover, `finalize` restore-on-error, `take_reconciled_worktrees` call + toast in `setupHookListeners`.

## Plan

### 1–2. Resume

`pty.rs:767`: when `agent_id == AgentId::claude_code()`, compute `candidates = db.lock()…resume_candidates(...)` (lock released at scope end), then `picked = candidates.and_then(|c| pick_resume_id(&c, &projects_dirs(c.transcript_path.as_deref())))` unlocked, then `resume_owned = cc_resume_arg(resume.as_deref(), picked)`. Other agents keep the existing branch. `tracing::warn!` once when every projects dir is missing. `adapter.rs:169`: `"continue"` → no flag.

### 3. Hook identity

CLI: read `hook_event_name` from the parsed JSON before inserting the field; skip the walk for other events (hooks fire on every tool call). Server socket loop (`server.rs:391-410`): `parse_agent_chain(&raw)`, look up `app.state::<PtyManager>().agent_shell_pid(sid)`, compute `is_top_level_chain`, `warn` when a chain is present but lacks the shell pid, pass `top_level` to `process_event`. Other `process_event` call sites (`server.rs:480`, adapter event pumps) pass `true`.

### 4. Lost worktree notice

`run()`: `let demoted = reconcile_worktrees(&db_guard);` then `.manage(ReconciledWorktrees(Mutex::new(demoted)))`. Frontend, end of `setupHookListeners`: `invoke<string[]>("take_reconciled_worktrees")`; non-empty → one `toastsAtom` toast, message `Worktree no longer exists`, description `<names joined by ", "> now run in the repo root with their own conversation.`

### 5. Delete guard

Top of `delete_session`, own short lock scope: `is_last_root_session` → `Err("The last repo-root session of a workspace cannot be deleted")`. Repeat it at the start of the first lock scope (before `claim_finalization`) and inside the final scope before `db.delete_session`. A refusal at the final check leaves the event pump stopped and the footer written early: accepted residue (design, Risks).

`cleanup_merged_session` (`workspaces_sessions.rs:~641`): the same check in its first lock scope, before plan archiving, worktree removal and branch deletion; return the same error. The frontend callers (`GitPanel.tsx:242-260`, `PrViewer.tsx:528-546`) already toast "Cleanup failed" with the message.

### 6. Frontend

`finalize`: replace `.catch(() => undefined)` with a catch that reloads workspaces (the terminal is already destroyed; it relaunches in `continue` mode on selection) through the existing workspaces refresh path (grep `get_workspaces` in `src/stores/`) and sets an error toast with the backend message; keep `reportCleanupErrors` for the success path. `SessionRow.tsx:195` and the collapsed popover (`Sidebar.tsx:300`, carry the workspace in `sessionsWithWs`) use `canDeleteSession`. The icon at `SessionRow.tsx:213` stays keyed on `worktree_path`.

## Per-phase risk

- [Detection misses the Claude Code process (unusual install)] → zero runs below the shell → top-level: same exposure as today, never a lost resume.
- [A wrapper or launcher sits between the PTY shell and `claude`] → non-claude wrappers break no run and `npx`→`node` forms one run → top-level. Unit scenarios cover both.
- [sysinfo cost in the hook CLI; `Stop` runs every turn] → `SessionStart`/`Stop` hooks are registered `is_async: true` (`src-tauri/src/setup.rs:12, 86`), so they do not block Claude Code; the walk still refreshes only one pid per step (≤16) to keep each hook process cheap. The walk measures the hook latency once with `time`.
- [Orphaned nested hook] → an async hook whose `claude -p` already exited walks a chain that reaches pid 1 without the shell pid, so it reads as nested (adversarial review, verified).
- [Backend refusal during the grace window] → `finalize` reloads workspaces and toasts; the sidebar never offers the action outside races.
- [CC writes the new transcript lazily after `/clear`] → the stored id has no file and the session starts fresh; it never falls back to the cleared conversation.

## Verification

- `cd src-tauri && cargo clippy --all-targets -- -D warnings && cargo test && cargo fmt --check`
- `npx tsc --noEmit && pnpm test`
- Manual walk on the installed `.deb` (`pnpm tauri build --bundles deb` + `dpkg -i`): repaired XD001 sessions reopen their own history; running `claude -p "hi"` inside a session does not change its stored id; a new XD001 session gets a worktree; "General XD001" has no delete action, a demoted session has one; a deleted worktree directory produces the toast after restart.
