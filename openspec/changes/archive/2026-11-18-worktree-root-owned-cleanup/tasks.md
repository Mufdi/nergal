## 1. Guard and command line

- [x] 1.1 Test (Red): `reclaim_target` with an explicit `registered` snapshot — accepts a registered, user-owned `<repo>/.worktrees/nergal/x` (returns the canonical path); rejects `<repo>`, `<repo>/.worktrees`, `<repo>/.worktrees/nergal`, `<repo>/.worktrees/nergal/x/sub`, `<repo>/src`, `/`, a missing path, `<repo>/.worktrees` as a symlink, a symlinked worktree, a directory absent from the snapshot, a `..` traversal, paths containing `,`, `:` or `"`, and a worktree owned by another uid (pass a uid different from the owner); `registered_worktrees` lists a real `git worktree add`; `docker_unblock_args` exact vector; `docker_supported` (rootful unix Docker → true; podman version, `rootless`, `userns`, `tcp://` → false); `blocked_dirs` false on a fresh temp tree, true with a `0555` subdir and with a `0000` subdir (unreadable); `unblock_own_dirs` makes a `0555` subdir and a user-owned `0000` subdir (with a child dir) deletable, and leaves the target of a symlinked worktree untouched; `ReclaimIssue::reason` strings; `claim_removal` on a missing path → `Gone`, twice on one path → `InProgress`, released on drop. All tests in this group and group 2 are `#[cfg(target_os = "linux")]`
- [x] 1.2 Add `ReclaimIssue`, `ClaimError`, `claim_removal`, `registered_worktrees`, `reclaim_target`, `blocked_dirs`, `unblock_own_dirs`, `docker_unblock_args`, `docker_supported` in `src-tauri/src/worktree.rs`, Linux-gated per design D8

## 2. Reclaim and retry

- [x] 2.1 Test (Red): through the real sequence (`git worktree add`, sealed `0o555` subdir, then `remove_worktree_with`) with a fake `Reclaimer` that records calls — default `unblock_own` → `Ok` with no Docker call; with `unblock_own` overridden to a no-op: `docker_unblock` that makes the sealed dir writable → one call with the canonical path, `Ok`, directory and registration gone; `docker_unblock` true but the dir stays unwritable → `reclaim ineffective` and the remedy mentions stopping containers; `docker_unblock` false → `docker failed`; `docker_status` `DockerNotFound`/`DockerUnsupported` → no `docker_unblock`, error with the reason; a second concurrent removal of the same path (claim held by the test) → `removal in progress`, no calls; a path that no longer exists → `Ok`; `remove_worktree_with(.., None)` keeps today's error; the existing `remove_worktree_permission_error_names_path_and_remedy` keeps passing unchanged
- [x] 2.2 Add `Reclaimer`, `reclaim_ownership` and `DockerReclaimer` (Linux: resolved path, podman/rootless/userns/endpoint checks, 120 s timeout, bounded `docker rm -f` on expiry, stderr drained to EOF, cached status; other targets: empty struct)
- [x] 2.3 Add `remove_worktree_with`, `remove_worktree_reclaiming` (off Linux it calls `remove_worktree`); `remove_worktree` delegates with `None`

## 3. Callers off the UI thread

- [x] 3.1 `src-tauri/src/commands/workspaces_sessions.rs`: `delete_session`, `delete_workspace`, `cleanup_merged_session` call `remove_worktree_reclaiming` inside `tauri::async_runtime::spawn_blocking`; make `delete_workspace` and `cleanup_merged_session` `async`
- [x] 3.2 Leave the MCP, Linear and ClickUp rollback callers on `remove_worktree`

## 5. Revision 1: retry path and inode pin

- [x] 5.1 Test (Red): update the locked tests to the Revision 1 interfaces (`ReclaimTarget`, new `docker_unblock` and `docker_unblock_args` signatures and exact vector with `RECLAIM_SCRIPT`); add: `reclaim_target` returns the canonical path with its `dev`/`ino`; `remove_worktree_reclaiming`-style retry accepts a recorded path absent from git's list (call `remove_worktree_with` twice: first with a failing fake, then a succeeding one); reclaim-path errors satisfy `is_reclaim_failure` and a non-reclaim error does not; `reclaim ineffective` always contains the manual remedy; `unblock_own_dirs` turns a `0777` dir into `0755`
- [x] 5.2 Test (Red): `src/stores/pendingDeletesWorkspace.test.ts` — refused `delete_workspace` reloads workspaces, restores tab ids, shows one error toast; when `get_workspaces` also fails the workspace is reinserted at its index
- [x] 5.3 Implement the Revision 1 interfaces in `src-tauri/src/worktree/reclaim.rs` and `worktree.rs` (script + positional args, `chown -h`, `-H <endpoint>`, process group kill, bounded reader join, mode mask, recorded-path snapshot entry, `ReclaimFailure`)
- [x] 5.4 Callers keep the row on `is_reclaim_failure` (`delete_session`, `cleanup_merged_session`); `delete_workspace` keeps the workspace and failed sessions
- [x] 5.5 Frontend: refused `delete_workspace` handling in `src/stores/pendingDeletes.ts`
- [x] 5.6 Manual walk: with `alpine:3` retagged away, deleting a session with a root-owned dir keeps the session with a `docker failed` toast; after restoring the tag, deleting again succeeds

## 4. Verification

- [x] 4.1 `cd src-tauri && cargo clippy --all-targets -- -D warnings && cargo test && cargo fmt --check`
- [x] 4.2 Manual walk: a session whose worktree holds a large root-owned tree (200k files) deletes from the sidebar with no prompt, leaves no directory and no `git worktree list` entry, and the window stays responsive during the delete
- [x] 4.3 Manual walk: with `alpine:3` retagged away, the delete shows an error with `docker failed` and the manual remedy; restore the tag afterwards
