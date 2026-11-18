# Implementation Plan: local-branches-chip

> Grounded in `main` at `0079b75`, symbols verified 2026-11-18. Revised after iprev round 1. Git behavior verified on git 2.34.1 in scratch repos: `for-each-ref` output; `diff-tree` (root and merge); `worktree add <path> nergal/x` attaches the branch even when a tag `nergal/x` exists, while `refs/heads/nergal/x` detaches; `rm -rf` leaves a `prunable` entry; `git worktree remove --force <missing path>` drops only that entry.

## Verified codebase facts (do not re-assume)

Backend:

- `src-tauri/src/worktree.rs:12` `git()` builds the `git` `Command` (no window). Use it for every new git call.
- `worktree.rs:272-482` branch primitives:
  - `BranchState` (:273), `KeptReason { InUse, Unshared, Unchecked, RecordFailed { path }, Moved }` (:280), `BranchOutcome { Deleted { sha }, Kept { reason }, Missing }` (:289).
  - `branch_state` (:295): a symref gives `Unknown`.
  - `branch_checked_out` (:334): true for any porcelain line `branch refs/heads/<b>`, including `prunable` entries. It also reads rebase/bisect files in every git dir, and fails closed.
  - `branch_is_safe_to_drop` (:376).
  - `record_deleted_branch` (:408): `create_dir_all`, append, `sync_all`.
  - `delete_branch_if_at` (:435): `update-ref --no-deref -d <ref> <sha>`.
  - `drop_session_branch` (:450): state → in use → safe → record → in use again → CAS.
- `worktree.rs:581` `create_worktree` returns early when the directory exists (:586) and tries `git worktree add -b` first (:591). Not reusable (design D4).
- `worktree.rs:623` `remove_worktree(repo, path)` (non-reclaiming): `git worktree remove --force`, then falls back to `remove_dir_all` + `prune`.
- `worktree.rs:1073` `CommitEntry { hash, message }` (`Clone, Serialize`).
- `worktree.rs:757` `file_diff` runs `git diff HEAD -- <rel>`. This is the working-tree diff that History wrongly shows.
- `worktree.rs:712` `mod reclaim;` is the precedent for a child module in `src-tauri/src/worktree/`. Test files attach at the end of `worktree.rs` with `#[cfg(test)] #[path = "worktree_<x>_tests.rs"] mod <x>_tests;` (:2147-2156).
- `src-tauri/src/lib.rs:1256` `reconcile_worktrees` clears DB rows for missing worktrees but never runs `git worktree prune`.
- `src-tauri/src/commands/git.rs:43` `list_branches(db, workspace_id)` reads `db.workspace_repo_path` (`db/workspaces.rs:101`) under the guard and drops it before git. It is the template for workspace-scoped commands.
- `commands/git.rs:57` `DiffResponse { file_path, diff_text, is_new }` (`Clone, Serialize`).
- `commands/git.rs:572` `get_commit_files(session_id, hash, db)`:
  - its only caller is `HistoryChip.tsx:57`, and it is registered at `lib.rs:559`;
  - it does not validate `hash`.
- `commands/shared.rs:46` `resolve_session_cwd(db, session_id)` returns the worktree path, else the workspace repo path.
- `commands/workspaces_sessions.rs:343` `build_new_session(session_id, name, workspace_id, worktree_path, worktree_branch, ts, &agent_id, agent_capabilities, launch_options, env_shells, default_pinned_notes)`. It sets `merge_target: None`.
- `workspaces_sessions.rs:395-500` `create_session`:
  - the session id is `format!("{}-{ts}", &workspace_id[..6.min(len)])` (:436);
  - the default pinned notes come from `get_obsidian_config(..).default_pinned_note_paths` (:420-425);
  - the agent is `AgentId::claude_code()`, with `capabilities_for_agent_id` (:183, private);
  - after the insert it calls `agents.register_session(&session.id, agent_id.clone())` and `extend_plan_watcher_for_session(&agents, &plan_watcher, &session, &repo_path)` (:52).
- `workspaces_sessions.rs:516` `CleanupNotice`; `:523` `deleted_branches_record()` (private). With no data dir it returns an empty path, the record write fails and the branch is kept. `:529` `kept_message(branch, &KeptReason)` (private). `:553` `notices_after_removal` calls `drop_session_branch`. It runs inside `delete_session`'s `spawn_blocking` (:702), without the DB guard.
- `workspaces_sessions.rs:612-704` `delete_session` is `async`, takes `State<'_, SharedDb>`, and runs git in `tauri::async_runtime::spawn_blocking`. It is the pattern for the new async commands.
- `src-tauri/src/db/mod.rs:103` `SharedDb = Arc<Mutex<Database>>`.
- `db/mod.rs:204` `Database::open_in_memory()` is `pub(crate)`. Test fixtures: `commands/workspaces_sessions_guard_tests.rs:1-34`.
- `db/sessions.rs` has no "sessions of a workspace with their branch" query.
- `db/sessions.rs:249` `clear_session_worktree` sets `worktree_path` and `worktree_branch` to NULL.
- `src-tauri/src/commands/mod.rs:1-24` declares private modules and glob re-exports them.
- `src-tauri/src/lib.rs:419` opens `generate_handler![`. The git commands are at :501, :511, :541 and :559.

Frontend:

- `src/stores/git.ts:84` `ChipMode` union; `:86` `CHIP_ORDER`; `:94` `gitChipModeAtom`; `:131` `prsCacheMapAtom` (plain atom keyed by workspace id).
- `src/components/git/chips/ChipStrip.tsx:12-18` props; `:20-26` `CHIP_META: Record<ChipMode, …>`; `:58` count if-chain.
- `src/components/git/GitPanel.tsx:97` `workspaceId = sessionToWorkspace[sessionId] ?? null`; `:463-477` strip and body switch.
- `src/components/git/chips/HistoryChip.tsx`: `:38-40` `get_recent_commits`; `:50-61` `toggleExpand` → `get_commit_files`; `:63-66` `openCommitFileZen(_hash, filePath)` flattens every expanded commit's files and drops the hash; `:69-100` keyboard model.
- `src/stores/zenMode.ts:35-61` `ZenModeState { open, filePath, sessionId, files, currentIndex }` and `openZenModeAtom`; `closeZenModeAtom` (:63).
- `src/components/zen/ZenMode.tsx:192` returns null without `filePath`/`sessionId`; `:253-260` renders `<DiffView key={state.filePath} filePath sessionId sideBySide inZen onNavFile/>`.
- `src/components/plan/DiffView.tsx:395-412` props; `:423` `fileTimestamp` from `activeSessionFilesAtom`; `:425-439` `fetchDiff` invokes `get_file_diff`.
- `src/components/layout/Sidebar.tsx:617-666` `spawnSession`. The post-create steps (:644-665) use `workspacesAtom`, the local `setExpandedIds` wrapper over `expandedWorkspaceIdsAtom` (:358, where `null` means not hydrated), `freshSessionsAtom`, `activeSessionIdAtom`, `focusZoneAtom` and `terminalService.focusActive()`.
- `src/stores/workspace.ts:82` `workspacesAtom`, `:83` `activeSessionIdAtom`, `:155` `expandedWorkspaceIdsAtom`, `:163` `freshSessionsAtom`.
- `src/stores/shortcuts.ts:63` `focusZoneAtom`. `shortcuts.ts:5-6` imports `./workspace` and `terminalService`, so `workspace.ts` must not import `shortcuts.ts`.
- `src/lib/confirm.ts:60` `confirm(opts)`; `src/lib/escapeHtml.ts:5` `escapeHtml`; `src/lib/shellQuote.ts` `posixQuote`; `src/stores/toast.ts:18` `toastsAtom`.
- Vitest runs only `src/**/*.test.ts` in Node (no DOM). Frontend tests target pure helpers and jotai stores (`createStore`).

## Locked interfaces (tests are written against these)

`src-tauri/src/worktree/local_branches.rs` (`pub mod local_branches;` in `worktree.rs`; tests in `src-tauri/src/worktree_local_branches_tests.rs`):

- `pub struct BranchCandidate { pub name: String, pub sha: String, pub last_commit_unix: i64, pub last_subject: String }` (`Debug, Clone, PartialEq, Eq`).
- `pub fn nergal_branch_candidates(repo: &Path) -> anyhow::Result<Vec<BranchCandidate>>`:
  - runs `git for-each-ref refs/heads/nergal/ --format=%(refname)%09%(objectname)%09%(symref)%09%(committerdate:unix)%09%(subject)` and splits each line into at most 5 fields;
  - strips `refs/heads/` from the refname;
  - skips rows with a non-empty symref field;
  - sorts by `last_commit_unix` descending;
  - returns `Err` when git fails.
- `pub fn pending_commit_count(repo: &Path, sha: &str, target: Option<&str>) -> Option<u32>`: runs `git rev-list --count <sha> --not --remotes <refs/heads/target | HEAD> --`. Returns `None` on a non-zero exit or unparsable output.
- `pub fn pending_commits(repo: &Path, sha: &str, target: Option<&str>, limit: u32) -> anyhow::Result<Vec<CommitEntry>>`: runs `git log --format=%h%x09%s -<limit> <sha> --not --remotes <target-ref> --`. Returns `Err` on a non-zero exit.
- `pub fn is_valid_commit_id(hash: &str) -> bool`: 4–64 chars, all ASCII hex digits.
- `pub fn resolve_commit(base: &Path, hash: &str) -> anyhow::Result<String>`:
  - an invalid id gives `Err` without git;
  - otherwise runs `git rev-parse --verify --quiet <hash>^{commit}`;
  - returns `Err` unless the full id starts with `hash` (lowercased).
- `pub fn nergal_slug(branch: &str) -> Option<&str>`: returns `Some(slug)` when `branch` is `nergal/<slug>` and the slug matches `^[A-Za-z0-9_-][A-Za-z0-9._-]*$`.
- `pub fn commit_files(base: &Path, hash: &str) -> anyhow::Result<Vec<String>>`: runs `resolve_commit`, then `git diff-tree --no-commit-id -r --root -m --first-parent --name-only <full>`. Returns the lines joined to `base`.
- `pub fn commit_file_diff(base: &Path, hash: &str, file_path: &str) -> anyhow::Result<String>`:
  - strips `base` from `file_path` (or takes it as relative);
  - refuses an absolute remainder or any `..` component without running git;
  - runs `resolve_commit`, then `git --literal-pathspecs diff-tree -p --no-color --no-commit-id --root -m --first-parent <full> -- <rel>`;
  - returns `Err` on a non-zero exit.
- `pub struct WorktreeEntry { pub path: PathBuf, pub branch: Option<String>, pub locked: bool, pub prunable: bool }` (`Debug, Clone, PartialEq, Eq`).
- `pub fn worktree_entries(repo: &Path) -> anyhow::Result<Vec<WorktreeEntry>>`: parses `git worktree list --porcelain` into blocks:
  - `worktree <path>` gives the path;
  - `branch refs/heads/<b>` gives `branch: Some(b)`;
  - a `locked` or `locked <reason>` line sets `locked`;
  - a `prunable` or `prunable <reason>` line sets `prunable`.
- `#[derive(Debug, Clone, PartialEq, Eq)] pub enum BranchUsage { Free, InUse, MissingWorktree { paths: Vec<PathBuf> } }`.
- `pub fn branch_usage(repo: &Path, branch: &str) -> BranchUsage`:
  - `InUse` when an entry holds the branch and is `locked`, or when `symlink_metadata(path)` succeeds;
  - `InUse` when the branch is being rebased or bisected (the rebase/bisect half of `branch_checked_out`, factored into `branch_mid_operation(repo, branch) -> bool` with the same fail-closed rule);
  - `InUse` on a git failure;
  - otherwise `MissingWorktree` with the paths of the remaining holders, or `Free`.
- `pub fn clear_missing_worktrees(repo: &Path, branch: &str) -> anyhow::Result<Vec<PathBuf>>`: for each entry that holds the branch, is not locked and whose path fails `symlink_metadata`, runs `git worktree remove --force <path>`. Returns the removed paths and leaves every other entry untouched.
- `pub fn add_worktree_for_branch(repo: &Path, slug: &str) -> anyhow::Result<PathBuf>`:
  - the path is `<repo>/.worktrees/nergal/<slug>`;
  - returns `Err` naming the path when `symlink_metadata(path)` succeeds;
  - otherwise runs `git worktree add <path> nergal/<slug>` (short name, never `-b`, never `refs/heads/`);
  - then requires `git -C <path> symbolic-ref -q HEAD` to print `refs/heads/nergal/<slug>`. Otherwise it runs `remove_worktree(repo, path)` and returns `Err`;
  - returns `Err` with git's stderr when `add` fails.

`src-tauri/src/worktree.rs` (refactor, existing tests unchanged):

- `branch_checked_out` keeps its behavior. Its rebase/bisect half moves into `pub(crate) fn branch_mid_operation(repo_path: &Path, branch: &str) -> bool`.
- `pub enum DropPolicy<'a> { RequireSafe { target: Option<&'a str> }, Unconditional }`.
- `pub fn drop_branch(repo_path: &Path, branch: &str, expected_sha: Option<&str>, policy: DropPolicy<'_>, record: &Path) -> BranchOutcome`:
  - runs the `drop_session_branch` sequence;
  - after `branch_state`, an `expected_sha` that differs from the current SHA gives `Kept { reason: Moved }`;
  - the safe check runs only for `RequireSafe`.
- `drop_session_branch(repo, branch, target, record)` becomes `drop_branch(repo, branch, None, DropPolicy::RequireSafe { target }, record)`.

`src-tauri/src/models.rs` + `src-tauri/src/db/sessions.rs`:

- `pub struct BranchOwner { pub session_id: String, pub session_name: String, pub branch: String, pub merge_target: Option<String> }` (`Debug, Clone, PartialEq, Eq`).
- `pub fn branch_owners(&self, workspace_id: &str) -> anyhow::Result<Vec<BranchOwner>>`: the sessions of the workspace with a non-null `worktree_branch`, ordered by `created_at`.

`src-tauri/src/commands/branches.rs` (tests in `src-tauri/src/commands/branches_tests.rs`):

- `#[derive(Debug, Clone, PartialEq, Eq, Serialize)] pub struct BranchSession { pub id: String, pub name: String }`.
- `#[derive(Debug, Clone, PartialEq, Eq, Serialize)] pub struct LocalBranchRow { pub name: String, pub sha: String, pub pending: Option<u32>, pub last_commit_unix: i64, pub last_subject: String, pub session: Option<BranchSession>, pub in_use: bool, pub missing_worktree: Option<String> }`.
- `pub(crate) fn local_branch_rows(repo: &Path, owners: &[BranchOwner]) -> Result<Vec<LocalBranchRow>, String>`:
  - returns `Ok(vec![])` when `!is_git_repo(repo)`;
  - keeps the candidates in order;
  - the owner is the first `BranchOwner` with `branch == name`;
  - `pending = pending_commit_count(repo, sha, owner.merge_target)`, and rows with `pending == Some(0)` are dropped;
  - `branch_usage` gives `InUse` → `in_use: true`, or `MissingWorktree { paths }` → `missing_worktree: Some(first path)`.
- `#[derive(Debug, Clone, PartialEq, Eq, Serialize)] #[serde(tag = "kind", rename_all = "snake_case")] pub enum LocalBranchDelete { Deleted { sha: String, repo: String }, Missing, Kept { message: String } }`.
- `pub(crate) fn branch_repo_lock(repo: &Path) -> Arc<Mutex<()>>`: one mutex per canonical repo path (the raw path when canonicalize fails); poisoned guards are recovered with `into_inner`.
- `pub(crate) fn delete_local_branch_core(db: &SharedDb, workspace_id: &str, branch: &str, expected_sha: &str, record: &Path) -> Result<LocalBranchDelete, String>`:
  1. invalid name → `Kept` "Branch <b> kept: not a Nergal branch name", without git;
  2. guard → repo path (missing → `Err("workspace not found")`) → drop the guard;
  3. take the repo lock;
  4. guard → `branch_owners`; an owner → `Kept` "Branch <b> kept: session <name> uses it"; drop the guard;
  5. `clear_missing_worktrees`;
  6. `drop_branch(repo, branch, Some(expected_sha), Unconditional, record)`, mapped with `kept_message`; `Deleted` carries `repo.display()`.
- `pub(crate) fn open_branch_session_core(db: &SharedDb, workspace_id: &str, branch: &str, ts: u64, agent_capabilities: Vec<String>) -> Result<(Session, PathBuf), String>`:
  - runs design D4 steps 1–7 with the same lock order as the delete;
  - returns the session and the repo path;
  - its errors name the cause: invalid name, the owning session, "checked out", the existing directory, a detached result, or git's stderr.
- `pub(crate) fn finish_branch_session(db: &SharedDb, repo: &Path, worktree: &Path, session: Session) -> Result<Session, String>`:
  - under the guard, re-checks `branch_owners` for `session.worktree_branch`, then inserts the row;
  - on an owner or an insert error, drops the guard, calls `remove_worktree(repo, worktree)` and returns the error, naming the owner when there is one.
- `#[derive(Debug, Clone, Deserialize)] #[serde(tag = "kind", rename_all = "camelCase")] pub enum CommitScope { #[serde(rename_all = "camelCase")] Session { session_id: String }, #[serde(rename_all = "camelCase")] Workspace { workspace_id: String } }`. JSON: `{ "kind": "session", "sessionId": … }` or `{ "kind": "workspace", "workspaceId": … }`.
- Tauri commands. All are `async`; git and DB-plus-git work runs inside `spawn_blocking` with a cloned `SharedDb`.
  - `list_local_branches(db, workspace_id) -> Result<Vec<LocalBranchRow>, String>`
  - `get_branch_commits(db, workspace_id, branch) -> Result<Vec<CommitEntry>, String>`: validates the name with `nergal_slug`, takes the target from the owner's `merge_target`, resolves the SHA with `branch_state` (`Err` unless `Exists`), and caps at 100.
  - `get_scoped_commit_files(db, scope: CommitScope, hash) -> Result<Vec<String>, String>`
  - `get_commit_file_diff(db, scope: CommitScope, hash, file_path) -> Result<DiffResponse, String>` (`is_new: false`)
  - `open_branch_session(db, agents, plan_watcher, workspace_id, branch) -> Result<Session, String>`: calls `capabilities_for_agent_id` (made `pub(crate)`) and the core, then `register_session` and `extend_plan_watcher_for_session`.
  - `delete_local_branch(db, workspace_id, branch, expected_sha) -> Result<LocalBranchDelete, String>`: the record is `deleted_branches_record()` (made `pub(crate)`).
- `kept_message` becomes `pub(crate)`.
- `notices_after_removal` holds `branch_repo_lock(repo_path)` around its `drop_session_branch` call.
- `get_commit_files` is removed, together with its `lib.rs` registration.

Frontend:

- `src/lib/localBranches.ts`:
  - `export interface LocalBranchRow { name: string; sha: string; pending: number | null; last_commit_unix: number; last_subject: string; session: { id: string; name: string } | null; in_use: boolean; missing_worktree: string | null }`
  - `export type LocalBranchDelete = { kind: "deleted"; sha: string; repo: string } | { kind: "missing" } | { kind: "kept"; message: string }`
  - `export function branchActionsAllowed(row: LocalBranchRow): boolean` returns `row.session === null && !row.in_use`.
  - `export function orphanBranchCount(rows: LocalBranchRow[]): number` counts the rows with no session.
  - `export function deleteBranchConfirm(row: LocalBranchRow): ConfirmOptions`:
    - `title: "Delete branch?"`;
    - a body with `escapeHtml(row.name)` in `<strong>`, the count (`"?"` when null) and "exist only on this machine";
    - `confirmLabel: "Delete"`, `cancelLabel: "Cancel"`, `kind: "warning"`, `destructive: true`.
  - `export function branchDeleteToast(branch: string, result: LocalBranchDelete): { message: string; description: string; type: "info" | "error" }`:
    - deleted → "Branch deleted" / "To restore it run: git -C <posixQuote(repo)> branch <branch> <sha>" / info;
    - missing → "Branch already deleted" / branch / info;
    - kept → "Branch kept" / message / error.
- `src/stores/zenMode.ts`:
  - `export type CommitScope = { kind: "session"; sessionId: string } | { kind: "workspace"; workspaceId: string }`;
  - `ZenModeState.commit: { hash: string; scope: CommitScope } | null`, default `null`;
  - `openZenModeAtom` params gain an optional `commit`;
  - `closeZenModeAtom` resets it.
- `src/stores/sessionActivation.ts` (imports only `jotai`, `./workspace` and types): `export const addAndActivateSessionAtom = atom(null, (get, set, session: Session) => …)`:
  - appends the session to its workspace in `workspacesAtom`, without a duplicate id;
  - adds the workspace id to `expandedWorkspaceIdsAtom` (a `null` atom becomes a set with that id);
  - adds the id to `freshSessionsAtom`;
  - sets `activeSessionIdAtom`.
  - `Sidebar.spawnSession` uses it. Setting `focusZoneAtom` to `"terminal"` and calling `terminalService.focusActive()` stay at the call sites.

## Execution order

1. Backend git layer: `worktree/local_branches.rs`, `branch_mid_operation` extraction, `drop_branch` refactor.
2. DB: `BranchOwner` + `branch_owners`.
3. Commands: `commands/branches.rs` (cores, lock, six commands); the lock in `notices_after_removal`; visibility changes in `workspaces_sessions.rs`; removal of `get_commit_files`; registration in `lib.rs`.
4. Frontend commit diffs: `zenMode.ts` commit field, `DiffView` + `ZenMode` pass-through, `HistoryChip` fix.
5. Frontend chip: `localBranches.ts`, `sessionActivation.ts`, `git.ts` chip mode + cache, `ChipStrip`, `BranchesChip.tsx`, `GitPanel` body, `Sidebar` refactor.

## Builder coordination

| Group | Files | Depends on |
|---|---|---|
| A. Backend | `worktree.rs`, `worktree/local_branches.rs`, `models.rs`, `db/sessions.rs`, `commands/branches.rs`, `commands/git.rs` (removal), `commands/mod.rs`, `commands/workspaces_sessions.rs`, `lib.rs` | — |
| B. Commit diffs (frontend) | `stores/zenMode.ts`, `components/zen/ZenMode.tsx`, `components/plan/DiffView.tsx`, `components/git/chips/HistoryChip.tsx` | A's command names |
| C. Branches chip (frontend) | `lib/localBranches.ts`, `stores/sessionActivation.ts`, `stores/git.ts`, `components/git/chips/ChipStrip.tsx`, `components/git/chips/BranchesChip.tsx`, `components/git/GitPanel.tsx`, `components/layout/Sidebar.tsx` | A (command shapes), B (`openZenModeAtom` commit param) |

Run A first. After A lands, B and C may run in parallel; C must not edit B's files. Known traps:

- Never run bare `cargo fmt` (hash-locked test files). Use `rustfmt --edition 2024 <changed files>`.
- `worktree_branch_tests.rs` and `worktree_review_tests.rs` are hash-locked and must pass unchanged after the refactor.
- A sync `#[tauri::command]` that runs git blocks the UI thread. Every new command is `async` + `spawn_blocking`.
- Lock order: never wait for `branch_repo_lock` while holding the DB guard (design D6).
- `git worktree add` gets the short branch name; a `refs/heads/...` argument detaches `HEAD`.
- `docs/shortcuts.md` has an unrelated user edit: never stage or commit it.

## Per-phase risk

- [Phase 1: refactoring `drop_session_branch` and `branch_checked_out` changes the session-delete behavior] → The locked branch tests run unchanged. `drop_branch` with `RequireSafe` and no `expected_sha` is the old sequence, step for step.
- [Phase 1: `clear_missing_worktrees` removes a worktree that only looked missing] → It acts only on unlocked entries that hold the branch, re-checks the path with `symlink_metadata` right before each call, and runs only inside a user-confirmed action.
- [Phase 3: `open_branch_session_core` leaves a worktree when a later step fails] → The symbolic-ref check and `finish_branch_session` both call `remove_worktree` on failure. Tests cover both.
- [Phase 3: deadlock between the DB guard and the repo lock] → Fixed acquisition order plus a test. The test holds the repo lock, starts a delete on another thread, takes the DB guard from the test thread while the delete waits, then releases the lock and joins.
- [Phase 4: a `DiffView` keyed by `filePath` reuses a stale diff across commits] → Key it by `${commit?.hash ?? ""}:${filePath}`.
- [Phase 5: the `BranchesChip` keyboard listener races HistoryChip or Zen] → Same `listenerActive` gating as `HistoryChip.tsx:34-36`. Only one chip body is mounted at a time.

## Verification

- `cd src-tauri && cargo clippy --all-targets -- -D warnings && cargo test && rustfmt --edition 2024 --check <changed .rs files>`
- `npx tsc --noEmit && pnpm test`
- `~/.claude/skills/work/scripts/work-gates.sh main local-branches-chip`
- Manual walks: `tasks.md` § Verification.

## Revision 1 (walk feedback)

### Verified facts
- Chip listeners: `HistoryChip.tsx:34-36`, `StashesChip.tsx:42`, `PrsChip.tsx:89`, `FilesChip.tsx:78`, `BranchesChip.tsx:71`, `ChipStrip.tsx:33-53`. None checks `focusZoneAtom`. The sidebar gates on `focusZone === "sidebar"` (`Sidebar.tsx:62-63`). `RightPanel.handlePanelFocus` (`RightPanel.tsx:136-139`) sets the zone to `panel` on focus inside the panel.
- `FocusZone` is `"sidebar" | "terminal" | "panel" | "quake"` (`src/stores/shortcuts.ts:44`).
- Sync commands that the git panel calls on mount or poll: `get_session_git_info` (`commands/git.rs:137`), `get_git_status` (`git.rs:288`), `git_stash_list` (`git.rs:406`), `get_recent_commits` (`git.rs:497`), `list_prs` (`ship_pr.rs:22`), `get_pr_status` (`ship_pr.rs:182`), `has_pending_merge` (`ship_pr.rs:270`), `poll_pr_checks` (`ship_pr.rs:312`), `get_conflicted_files` (`conflicts.rs:9`). No Rust code calls these functions directly; only `lib.rs` registers them.
- `blocking()` helper: `commands/branches.rs:308-316` (`spawn_blocking`).

### Locked interfaces (`src/lib/branchTree.ts`, `src/lib/gitChipKeys.ts`)
```ts
// src/lib/branchTree.ts
export interface BranchCommit { hash: string; message: string }
export type BranchTreeItem =
  | { kind: "branch"; key: string; row: LocalBranchRow }
  | { kind: "commit"; key: string; branch: string; hash: string; message: string }
  | { kind: "file"; key: string; branch: string; hash: string; path: string };
export interface BranchTreeState {
  expandedBranch: string | null;
  expandedCommit: string | null;
  branchCommits: Record<string, BranchCommit[]>; // keyed by branch name
  commitFiles: Record<string, string[]>;         // keyed by commit hash
}
export function branchKey(name: string): string;                          // "b:<name>"
export function branchTreeItems(rows: LocalBranchRow[], s: BranchTreeState): BranchTreeItem[];
// keys: "b:<name>", "c:<name>:<hash>", "f:<name>:<hash>:<path>"; commits only under expandedBranch
// (when branchCommits has it), files only under expandedCommit of that branch (when commitFiles has it).
export function cursorIndexFor(items: BranchTreeItem[], key: string | null, fallback: number): number;
// index of key when present; else clamp(fallback, 0, items.length - 1); 0 for an empty list.
export type BranchTreeAction =
  | { type: "toggle-branch"; branch: string }
  | { type: "toggle-commit"; hash: string }
  | { type: "open-file"; hash: string; path: string }
  | { type: "open-session"; row: LocalBranchRow }
  | { type: "delete"; row: LocalBranchRow };
export function branchTreeKeyAction(item: BranchTreeItem | undefined, code: string): BranchTreeAction | null;
// Space|Enter on branch → toggle-branch; Space|Enter on commit → toggle-commit; Enter on file → open-file;
// KeyO / KeyD on a branch row with branchActionsAllowed(row) → open-session / delete; everything else → null.
export function changedBranches(prev: LocalBranchRow[], next: LocalBranchRow[]): string[];
// names in prev whose row is gone from next or whose sha differs.
export function openSessionConfirm(row: LocalBranchRow): ConfirmOptions;
// title "Open session?"; body contains <strong>escapeHtml(row.name)</strong> and "new Claude Code session";
// confirmLabel "Open session", cancelLabel "Cancel", kind "question", destructive false.

// src/lib/gitChipKeys.ts
export function chipKeysActive(o: {
  inZen: boolean; anyZenOpen: boolean; zenZone: string;
  focusZone: "sidebar" | "terminal" | "panel" | "quake";
}): boolean;
// inZen → anyZenOpen && zenZone === "sidebar"; otherwise → !anyZenOpen && focusZone === "panel".
```

### Execution order
1. Red: `src/lib/branchTree.test.ts`, `src/lib/gitChipKeys.test.ts`.
2. Rust: the nine commands become `pub async fn` and run their unchanged bodies in `blocking()` (move the helper to a shared spot in `commands/` or copy the 6-line helper).
3. Helpers, then `BranchesChip` (flat cursor, `O` + confirm, 5 s poll, keyed cursor), then `chipKeysActive` in the five chips and `ChipStrip`, then the session-set reload in `GitPanel`.

## Revision 2 (History keyboard, push freeze)

### Verified facts
- `HistoryChip.tsx:76-107`: the key listener moves a cursor over commits only; files render as buttons without a keyboard path (`HistoryChip.tsx:150-166`).
- Push-flow callers: `ShipDialog.tsx:164,253,276,278,301,349`, `FilesChip.tsx:132,151`, `TopBar.tsx:747`. Commands: `git_commit`, `git_push` (`commands/git.rs`), `git_ship`, `get_pr_preview_data` (`commands/ship_pr.rs`).

### Locked interfaces (`src/lib/historyTree.ts`)
```ts
export type HistoryTreeItem =
  | { kind: "commit"; key: string; hash: string }
  | { kind: "file"; key: string; hash: string; path: string };
export function historyTreeItems(
  commits: { hash: string }[],
  expandedCommit: string | null,
  commitFiles: Record<string, string[]>,
): HistoryTreeItem[];
// keys "c:<hash>", "f:<hash>:<path>"; files only under expandedCommit, when commitFiles has it.
export type HistoryTreeAction =
  | { type: "toggle-commit"; hash: string }
  | { type: "open-file"; hash: string; path: string };
export function historyTreeKeyAction(item: HistoryTreeItem | undefined, code: string): HistoryTreeAction | null;
// Space|Enter on commit → toggle-commit; Enter on file → open-file; everything else → null.
```
`cursorIndexFor` (`src/lib/branchTree.ts`) accepts `{ key: string }[]`.
