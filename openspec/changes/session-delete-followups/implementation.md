# Implementation Plan: session-delete-followups

> Grounded in the working tree at `a54f05f`, symbols verified 2026-11-17. Revised after iprev rounds 1–4.

## Verified codebase facts (do not re-assume)

- `src-tauri/src/worktree.rs:257-270` `delete_branch(repo, branch)` runs `git branch -D`. Callers: `delete_workspace` (`commands/workspaces_sessions.rs:~303`), `cleanup_merged_session` (`~:789`). `delete_session` never deletes a branch.
- `delete_session`: removal inside `spawn_blocking` (`~:590-597`); reclaim failure → `Err`, row kept (`~:599-607`); other failure → warning, continue (`~:608-614`); final lock scope re-validates and deletes the row (`~:618+`). Today it returns `Vec<String>`.
- `src/stores/pendingDeletes.ts:65-68` `reportCleanupErrors` shows each string as an error toast "Worktree left on disk"; the session `finalize` calls it on success.
- `Session` has `worktree_branch` and `merge_target` (`src-tauri/src/models.rs`).
- `get_workspaces` (`commands/workspaces_sessions.rs:157-171`) loads under the DB guard; `Workspace.is_git` is computed under the guard (`db/workspaces.rs:69`).
- `start_claude_session` (`src-tauri/src/pty.rs:665`) is the single backend entry for agent launches.
- `TerminalManager.tsx:45-52` picks the mode; `:54-55` calls `terminalService.show`; `:58-60` `spawnEnvShells`. `terminalService.show` deduplicates concurrent calls (`terminalService.ts:307`).
- `freshSessionsAtom` writers: `worktreeGate.ts:87`, `linear.ts:570`, `clickup.ts:585`, `deepLinkRouter.ts:277`, `deepLinkRouter.ts:417`, `Sidebar.tsx:664`. Readers: `TerminalManager.tsx:48`, `Sidebar.tsx:614`, `shortcuts.ts:137` (which triggers `triggerResumeSessionAtom`).

## Locked interfaces (tests are written against these)

`src-tauri/src/worktree.rs` (all targets; plain git argument vectors):

- `pub fn branch_is_safe_to_drop(repo_path: &Path, sha: &str, target: Option<&str>) -> bool` — `git rev-list <sha> --not --remotes <refs/heads/target | HEAD> --` exits 0 with empty stdout → `true`; anything else (non-empty, non-zero exit, git missing) → `false`.
- `#[derive(Debug, PartialEq, Eq)] pub enum BranchOutcome { Deleted { sha: String }, Kept { reason: KeptReason }, Missing }` and `pub enum KeptReason { InUse, Unshared, Unchecked, RecordFailed { path: PathBuf }, Moved }` (derives `Debug, PartialEq, Eq`).
- `pub fn branch_checked_out(repo_path: &Path, branch: &str) -> bool` — true when `git worktree list --porcelain` has a line exactly equal to `branch refs/heads/<branch>`, or when any worktree git dir (repo root: `git rev-parse --git-dir`; linked: `<git-common-dir>/worktrees/*`) has `rebase-merge/head-name` or `rebase-apply/head-name` equal to `refs/heads/<branch>`, or `BISECT_START` equal to `<branch>`; true on git failure (fail closed).
- `pub enum BranchState { Exists { sha: String }, Missing, Unknown }` and `pub fn branch_state(repo_path: &Path, branch: &str) -> BranchState` — `show-ref --verify --quiet refs/heads/<branch>`: exit 0 → `rev-parse` SHA (`Exists`; a failing `rev-parse` → `Unknown`), exit 1 → `Missing`, else `Unknown`.
- `pub fn record_deleted_branch(record: &Path, repo_path: &Path, branch: &str, sha: &str) -> std::io::Result<()>` — `create_dir_all(record.parent())`, then appends `<RFC 3339>\t<repo>\t<branch>\t<sha>\n` (create + append, never truncate). Production record: `dirs::data_dir()/nergal/deleted-branches.log`.
- `pub fn delete_branch_if_at(repo_path: &Path, branch: &str, sha: &str) -> bool` — `git update-ref -d refs/heads/<branch> <sha>` exit status.
- `pub fn drop_session_branch(repo_path: &Path, branch: &str, target: Option<&str>, record: &Path) -> BranchOutcome` — exactly design D1: `branch_state` (`Missing` → `Missing`, `Unknown` → `Kept`); `branch_checked_out` → `Kept`; `branch_is_safe_to_drop` false → `Kept`; `record_deleted_branch` error → `Kept` (write-ahead) and the `branch_kept` message says `could not write the restore record at <path>` instead of "has commits only on this machine" (so `BranchOutcome::Kept` carries a reason: `Kept { reason: KeptReason }` with `InUse`, `Unshared`, `Unchecked`, `RecordFailed { path }`, `Moved`); `delete_branch_if_at` false → `Kept`; else `tracing::info!` and `Deleted { sha }`.
- `pub fn worktree_pending_delete(worktree_path: &Path) -> bool` — `!join(".git").exists()` (true also when the directory is gone).

`src-tauri/src/commands/workspaces_sessions.rs`:

- `#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)] pub struct CleanupNotice { pub kind: &'static str, pub message: String }` with kinds `"worktree_left"`, `"branch_kept"` and `"branch_deleted"` (message contains `git branch <name> <sha>`); `delete_session` returns `Result<Vec<CleanupNotice>, String>`.
- `pub(crate) fn notices_after_removal(removal: anyhow::Result<()>, repo_path: &Path, branch: Option<&str>, target: Option<&str>, record: &Path) -> Result<Vec<CleanupNotice>, String>` — `Err(e)` with `is_reclaim_failure` → `Err(message)` (row kept, branch untouched); other `Err(e)` → `Ok([worktree_left: "<e>; branch <name> kept"])` without touching the branch; `Ok(())` → `drop_session_branch` when `branch` is `Some`, `Kept` → `branch_kept` notice, `Deleted` → `branch_deleted` notice, `Missing` → no notice. Runs inside the existing `spawn_blocking`.
- `pub(crate) fn refuse_launch(worktree_path: Option<&Path>, cwd: Option<&Path>) -> Result<(), String>` — `Err("This worktree was partly deleted; delete the session to finish")` when `worktree_path` is `Some` and pending; `Err("Working directory <cwd> does not exist")` when `cwd` is `Some` and not an existing directory; else `Ok(())`. `start_claude_session` looks the row up in its own short guard scope (missing row → `Err`), releases it, and calls this before `reserve_session_pty` (`pty.rs:685`). `spawn_aux_shell` (`pty.rs:1029`, gains `State<SharedDb>`) calls it with its session's worktree and `None`; a missing session row means `refuse_launch(None, None)` (allowed: an aux shell without a worktree has nothing pending, iprev R5 #5).

`src-tauri/src/models.rs`: `Session.worktree_pending_delete: bool` (`#[serde(default)]`, never written).

Frontend:

- `src/lib/launchMode.ts`: `export function launchAndConsume(id: string, launchModes: Record<string, "new" | "continue">, fresh: ReadonlySet<string>): { mode: "new" | "continue"; nextFresh: Set<string> }` — precedence: explicit `launchModes[id]`, else `new` when fresh, else `continue`; `nextFresh` is a new set without `id` only when the mode is `new`, else a copy; the input set is never mutated.
- `terminalService.showTerminal`: declare `container` before the `try`; in the `catch`, remove the container (its textarea listeners go with it; the entry is not yet in `entries`), and raise an error toast with the backend message at most once per session key until a later successful `show` clears the mark.
- `src/stores/pendingDeletes.ts`: `export function noticeToast(notice: { kind: string; message: string }): { message: string; description: string; type: "error" | "info" }` — `worktree_left` → "Worktree left on disk"/error; `branch_kept` → "Branch kept"/info; `branch_deleted` → "Branch deleted"/info. The session `finalize` routes `delete_session` notices through it; the workspace path keeps `reportCleanupErrors` with `string[]`.

## Execution order

1. `branch_state`, `branch_checked_out`, `branch_is_safe_to_drop`, `record_deleted_branch`, `delete_branch_if_at`, `BranchOutcome`, `drop_session_branch`, `worktree_pending_delete`.
2. `CleanupNotice`, `notices_after_removal`; `delete_session` carries `worktree_branch` + `merge_target` into the blocking step and uses it.
3. `get_workspaces`: release the guard, then set `worktree_pending_delete`.
4. `refuse_launch` in `start_claude_session` (before `reserve_session_pty`) and `spawn_aux_shell`.
5. Frontend: `Session` type field; `launchAndConsume` in `TerminalManager`; pending panel (no `show`, no `spawnEnvShells`); `SessionRow` marker; `terminalService` catch cleanup + toast; session `finalize` uses `noticeToast`.

## Per-phase risk

- [Typed notices change a command's return type] → only the session-delete `finalize` consumes it; grep `invoke<string[]>("delete_session"` to confirm a single caller.
- [Pending panel focus] → reuse the existing empty-state container of `TerminalManager`.

## Verification

- `cd src-tauri && cargo clippy --all-targets -- -D warnings && cargo test && cargo fmt --check`
- `npx tsc --noEmit && pnpm test`
- Manual walks in `tasks.md` group 4.
