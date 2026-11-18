# Implementation Plan: worktree-root-owned-cleanup

> Grounded in the current codebase, symbols verified 2026-11-17. Revised after iprev rounds 1–4 and the user's "Docker only" decision. Every item below except `ReclaimIssue`, `DockerReclaimer::new`, `remove_worktree` and `remove_worktree_reclaiming` is `#[cfg(target_os = "linux")]` (design D8); off Linux `remove_worktree_reclaiming` calls `remove_worktree`. Build after `session-resume-by-id`, which also edits `delete_session` and `cleanup_merged_session`.

## Verified codebase facts (do not re-assume)

- `src-tauri/src/worktree.rs:389` `remove_worktree(repo_path, worktree_path) -> Result<()>`: `git worktree remove --force`; on failure `remove_dir_all` (when the path exists), then `git worktree prune`, then bail with `could not remove worktree dir <path>: <e><remedy>`; `remedy` names the manual `sudo rm -rf <path>` step only for `ErrorKind::PermissionDenied` (`worktree.rs:401-430`).
- `git worktree remove --force` drops `.git/worktrees/<id>` even when it fails halfway on a permission error (reviewer experiment with git 2.34.1 on this host, iprev R2 #1). A registration check after that command always fails.
- Test `remove_worktree_permission_error_names_path_and_remedy` (`worktree.rs:~1591`, `cfg(unix)`) seals a subdirectory with mode `0o555` and asserts the path and the manual remedy. It keeps calling `remove_worktree` (no reclaim) and must keep passing.
- Worktrees are created at `<repo>/.worktrees/nergal/<slug>` (`worktree.rs:343-348`); `create_worktree` reuses an existing directory (`worktree.rs:351-353`); slugs carry a timestamp.
- `which = "6"` (`src-tauri/Cargo.toml:69`); `libc` is a `cfg(unix)` dependency (`Cargo.toml:115-116`).
- Callers (anchors after `session-resume-by-id` landed in the working tree; re-grep `remove_worktree(` before editing): user-initiated `commands/workspaces_sessions.rs` `delete_workspace` (`#[tauri::command] pub fn`, ~190; removal loop ~259-266), `delete_session` (`pub async fn`, removal ~537), `cleanup_merged_session` (`pub fn`, ~660; removal ~700). Automatic rollbacks: `mcp/worktree_sessions.rs:958`, `linear/mod.rs:594`, `clickup/mod.rs:976`.
- `alpine:3` is present in the local Docker image store (2026-11-17); the user is in the `docker` group.

## Locked interfaces (tests are written against these)

In `src-tauri/src/worktree.rs`:

- `#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub enum ReclaimIssue { GuardRejected, InProgress, DockerNotFound, DockerUnsupported, DockerFailed, Ineffective }` with `pub fn reason(&self) -> &'static str` = `"guard rejected" | "removal in progress" | "docker not found" | "docker unsupported" | "docker failed" | "reclaim ineffective"`.
- `pub fn registered_worktrees(repo_path: &Path) -> Vec<PathBuf>`: canonicalized `worktree <path>` entries of `git worktree list --porcelain` (empty on failure).
- `pub fn reclaim_target(repo_path: &Path, worktree_path: &Path, uid: u32, registered: &[PathBuf]) -> Result<PathBuf, ReclaimIssue>`: the guard of the spec with the pre-removal snapshot; returns the canonical path or `Err(ReclaimIssue::GuardRejected)`.
- `pub fn blocked_dirs(dir: &Path) -> bool`: true when any directory of the tree (not following symlinks) lacks `W_OK|X_OK` for the process (`libc::access`), or its `read_dir` fails with `PermissionDenied`.
- `pub fn unblock_own_dirs(dir: &Path, uid: u32)`: adds `u+rwx` to every directory of the tree owned by `uid`; best effort, errors ignored. It does nothing when `symlink_metadata(dir)` is not a real directory, never follows symlinks (including the root), and fixes each directory's mode before reading it, so a user-owned `0000` directory is still walked (own recursion, not `walkdir`'s pre-order) (iprev R5 #1, #2).
- `pub trait Reclaimer { fn unblock_own(&self, dir: &Path, uid: u32) { unblock_own_dirs(dir, uid) } fn docker_status(&self) -> Result<(), ReclaimIssue>; fn docker_unblock(&self, dir: &Path, uid: u32, gid: u32) -> bool; }` (`unblock_own` has the real default so tests can override it to a no-op; `docker_unblock`: true on exit 0 within the timeout).
- `pub fn reclaim_ownership(r: &dyn Reclaimer, dir: &Path, uid: u32, gid: u32) -> Result<(), ReclaimIssue>`: `docker_status`; `docker_unblock` false → `DockerFailed`; `blocked_dirs` still true → `Ineffective`.
- `#[derive(Debug, PartialEq, Eq)] pub enum ClaimError { InProgress, Gone }`.
- `pub fn claim_removal(path: &Path) -> Result<RemovalClaim, ClaimError>`: canonicalizes `path` (failure → `Gone`) and inserts it in a process-wide `Mutex<HashSet<PathBuf>>` (already present → `InProgress`). `RemovalClaim` removes it on `Drop`; a poisoned mutex is recovered with `into_inner`.
- `pub fn remove_worktree_with(repo_path: &Path, worktree_path: &Path, reclaimer: Option<&dyn Reclaimer>) -> Result<()>`: `None` = today's behavior exactly. `Some` = follow the design flowchart: snapshot `registered_worktrees`; path missing or `ClaimError::Gone` → today's prune-and-`Ok`; `ClaimError::InProgress` → error with `removal in progress`, nothing touched; else → `git worktree remove --force` → `remove_dir_all`; on `PermissionDenied`: `r.unblock_own` + retry; still failing → if the path no longer exists, prune + `Ok` (iprev R5 #5) → `reclaim_target` (snapshot) → `reclaim_ownership` → one more `remove_dir_all` → prune → `Ok` or the error. The error keeps `could not remove worktree dir <path>: <e>` (the real `io::Error` of the last attempt) + the manual remedy and appends `(reclaim: <reason>)`; a failed retry after a successful Docker step uses `reclaim ineffective`, and that reason adds "stop any container that mounts this worktree" to the remedy. A non-`PermissionDenied` error from the unprivileged retry goes straight to prune + the existing error.
- `pub fn remove_worktree(repo, wt) -> Result<()>` = `remove_worktree_with(repo, wt, None)`.
- `pub fn remove_worktree_reclaiming(repo: &Path, wt: &Path, reclaimer: &DockerReclaimer) -> Result<()>`: Linux = `remove_worktree_with(repo, wt, Some(reclaimer))`; other targets = `remove_worktree(repo, wt)`.
- `pub struct DockerReclaimer` with `pub fn new() -> Self` on every target (empty struct off Linux). Its cached status uses `std::sync::OnceLock` so it is `Sync`. Off Linux, the stub of `remove_worktree_reclaiming` names the parameter `_reclaimer`, and `ReclaimIssue` carries `#[cfg_attr(not(target_os = "linux"), allow(dead_code))]` (iprev R5 #4).
- `pub(crate) fn docker_unblock_args(name: &str, dir: &Path, uid: u32, gid: u32) -> Vec<OsString>` = `["run", "--rm", "--name", "<name>", "--pull", "never", "--network", "none", "--user", "0:0", "--entrypoint", "find", "--mount", "type=bind,source=<dir>,target=/w", "alpine:3", "/w", "-xdev", "-type", "d", "-exec", "chown", "<uid>:<gid>", "{}", "+", "-exec", "chmod", "u+rwx", "{}", "+"]` (smoke-tested on this host, design D3).
- `pub(crate) fn docker_supported(version_output: &str, security_options_json: &str, endpoint: &str) -> bool`: false when the version output mentions `podman`, the options mention `rootless` or `userns`, or the endpoint does not start with `unix://`.

`DockerReclaimer` on Linux stores the path `which::which("docker")` returned and always invokes that path. `docker_status` (result cached in the reclaimer): not found → `DockerNotFound`; `docker --version`, `docker info --format '{{json .SecurityOptions}}'` and `docker context inspect --format '{{.Endpoints.docker.Host}}'` (15 s timeout each). A `--version` that mentions `podman` → `DockerUnsupported` before the other two run. A non-zero `info`/`context` → `DockerFailed`. `docker_supported` false → `DockerUnsupported`. `docker_unblock`: container name `nergal-reclaim-<pid>-<counter>`, 120 s timeout (poll `try_wait` every 100 ms; on expiry `docker rm -f <name>` with its own 15 s limit (same spawn + poll, killed on expiry), then kill + wait the CLI regardless); stdin null, stdout discarded, stderr drained to EOF by a thread that keeps the first 4 KB for the `warn!` log. uid/gid from `libc::getuid()`/`libc::getgid()`.

## Execution order

1. `ReclaimIssue`, `registered_worktrees`, `reclaim_target`, `blocked_dirs`, `unblock_own_dirs`, `docker_unblock_args`, `docker_supported`.
2. `Reclaimer`, `reclaim_ownership`, `claim_removal`/`RemovalClaim`, `DockerReclaimer`.
3. `remove_worktree_with`, `remove_worktree` (delegates with `None`), `remove_worktree_reclaiming`.
4. Callers: `delete_session`, `delete_workspace`, `cleanup_merged_session` use `remove_worktree_reclaiming` inside `tauri::async_runtime::spawn_blocking`; the two sync commands become `async` (keep their `Result` return types). `delete_workspace` builds one `DockerReclaimer` and runs its whole removal loop (worktree removal + `delete_branch`) inside a single `spawn_blocking` closure that owns the reclaimer and cloned paths; `cleanup_merged_session` does the same for its removal + branch-delete steps (iprev R5 #3). Rollback callers stay on `remove_worktree`.

## Per-phase risk

- [Guard reads git output] → `git worktree list --porcelain`, canonicalize each `worktree <path>` line; no shell.
- [async commands] → no `std::sync::MutexGuard` may live across an `.await` (the future must stay `Send`); keep every DB scope synchronous and closed before awaiting `spawn_blocking`.
- [In-flight set] → released by `RemovalClaim::drop`, so a panic or early return never leaves a path claimed; a poisoned mutex is recovered with `into_inner`.
- [Logging] → one `warn!` per reclaim attempt with the reason, the exit status and the stderr head.

## Verification

- `cd src-tauri && cargo clippy --all-targets -- -D warnings && cargo test && cargo fmt --check`
- Manual walk: fill a test worktree with a large root-owned tree (a container with `--mount type=bind,source=<wt>,target=/w alpine:3` that creates 200k files under `/w/c`), delete the session from the sidebar and interact with the window during the delete: no prompt, directory and registration gone, window responsive.
- Manual walk (negative): with `alpine:3` retagged away (`docker tag alpine:3 alpine:keep`, then remove the `alpine:3` tag; restore afterwards), the delete shows the error with `docker failed` and the manual remedy.

## Revision 1 (code review, 2026-11-17)

Locked interface changes (tests are rewritten against these):

- `pub struct ReclaimTarget { pub path: PathBuf, pub dev: u64, pub ino: u64 }`; `reclaim_target(repo, wt, uid, registered) -> Result<ReclaimTarget, ReclaimIssue>` (`dev`/`ino` from `symlink_metadata` of the canonical path).
- `Reclaimer::docker_unblock(&self, target: &ReclaimTarget, uid: u32, gid: u32) -> bool`.
- `reclaim_ownership(r, target: &ReclaimTarget, uid, gid)`.
- `pub(crate) const RECLAIM_SCRIPT: &str = "[ \"$(stat -c %d:%i /w)\" = \"$1\" ] || exit 3; exec find /w -xdev -type d -exec chown -h \"$2\" {} + -exec chmod u+rwx {} +";`
- `docker_unblock_args(name: &str, endpoint: &str, target: &ReclaimTarget, uid: u32, gid: u32) -> Vec<OsString>` = `["-H", "<endpoint>", "run", "--rm", "--name", "<name>", "--pull", "never", "--network", "none", "--user", "0:0", "--entrypoint", "sh", "--mount", "type=bind,source=<path>,target=/w", "alpine:3", "-c", RECLAIM_SCRIPT, "reclaim", "<dev>:<ino>", "<uid>:<gid>"]`.
- `#[derive(Debug)] pub struct ReclaimFailure { pub issue: ReclaimIssue, pub message: String }` implementing `Display` (= `message`) and `std::error::Error`; every reclaim-path error of `remove_worktree_with` is an `anyhow::Error` wrapping it. `pub fn is_reclaim_failure(err: &anyhow::Error) -> bool` (downcast).
- `remove_worktree_reclaiming(repo, wt, reclaimer)` adds `canonicalize(wt)` (when it exists) to the snapshot it passes to the guard: `wt` is the session's recorded path.
- `unblock_own_dirs` sets `(mode & 0o1755) | 0o700` (no group/other write, no setuid/setgid).
- `ReclaimIssue::Ineffective` errors always include the manual remedy, whatever the `io::Error` kind.
- `DockerReclaimer`: the docker child gets `process_group(0)`; on timeout kill the group (`libc::kill(-pgid, SIGKILL)`) after `docker -H <endpoint> rm -f <name>` (15 s); the stderr reader is joined through a channel with a 5 s limit.

Callers (`src-tauri/src/commands/workspaces_sessions.rs`):

- `delete_session` and `cleanup_merged_session`: when the removal error `is_reclaim_failure`, return `Err(message)` before deleting the branch or the row (the footer/MOC work that already ran is acceptable residue, as in session-resume-by-id).
- `delete_workspace`: collect reclaim failures per session; delete the rows (and branches) of the other sessions; when any failed, keep the workspace row and the failed sessions and return `Err` naming them; otherwise today's flow.

Frontend (`src/stores/pendingDeletes.ts`): `delete_workspace` rejection → reload workspaces via `get_workspaces` (fallback: reinsert the workspace at its index), restore the tab ids of the sessions that came back, show one error toast with the backend message.
- `pub fn reclaim_snapshot(repo_path: &Path, recorded: &Path) -> Vec<PathBuf>` (added at Red for Revision 1): `registered_worktrees(repo_path)` plus `canonicalize(recorded)` when it exists, without duplicates. `remove_worktree_reclaiming` passes it to the guard; `remove_worktree_with` keeps computing `registered_worktrees` for other callers.
