Linear: none (no room for new issues; recorded as a comment on FRI-490)

## Why

Containers that write through a bind mount as root leave root-owned files in a worktree (`__pycache__`, `.ruff_cache`, `.next`). Deleting the session then fails with `PermissionDenied`, and Nergal only prints a `sudo rm -rf <path>` hint (BUG-42). The user has to clean every such worktree by hand: on 2026-11-17 a 1.2 GB orphan directory in XD001 needed a manual container run to delete.

## What Changes

- When a user-initiated delete cannot remove a worktree directory because of `PermissionDenied`, Nergal reclaims ownership through a throwaway Docker container and retries the removal once. Root-owned files in a worktree come from containers, so Docker is present whenever this happens.
- The container gets only that worktree, no network, no image pull, a time limit, and changes ownership without following symlinks or touching device nodes.
- Reclaim runs only on a guarded path: exactly `<repo>/.worktrees/nergal/<slug>`, every component a real directory, the worktree directory owned by the current user, and registered in `git worktree list`.
- Docker in rootless mode, or Podman behind a `docker` command, is skipped: its `chown` would hand the files to a subordinate uid.
- Rollbacks that Nergal starts on its own (MCP, Linear, ClickUp worktree creation) never reclaim.
- The three delete commands stop blocking the UI thread while the removal runs.
- When the reclaim is skipped or fails, the current error with the `sudo rm -rf` remedy stays, and it states why the reclaim did not run.
- Linux only. Other targets keep the current behavior (focus freeze 2026-10-04).

## Capabilities

### New Capabilities

- `worktree-removal`: removing a session worktree, including directories with root-owned files left by containers.

### Modified Capabilities

(none)

## Impact

- Backend: `src-tauri/src/worktree.rs` (`remove_worktree`, new reclaim helpers), `src-tauri/src/commands/workspaces_sessions.rs` (`delete_session`, `delete_workspace`, `cleanup_merged_session` run the removal on the blocking pool), rollback callers in `src-tauri/src/mcp/worktree_sessions.rs`, `src-tauri/src/linear/mod.rs`, `src-tauri/src/clickup/mod.rs` (explicit no-reclaim).
- External tool: `docker`, resolved with `which`, optional.
- Security: membership in the `docker` group is already root-equivalent, so the reclaim adds no privilege the user lacks. `pkexec` was considered and dropped: a same-user process could swap a path component between the check and a root `chown` (user decision, 2026-11-17).
