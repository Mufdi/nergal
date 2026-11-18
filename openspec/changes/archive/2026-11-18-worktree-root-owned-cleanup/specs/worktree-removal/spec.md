## ADDED Requirements

### Requirement: User-initiated deletes unblock directories before giving up

When a user-initiated worktree removal (`delete_session`, `delete_workspace`, `cleanup_merged_session`) cannot delete the worktree directory because of `PermissionDenied`, the system SHALL first add `u+rwx` to every directory of the tree owned by the current user and retry. When directories owned by another user still block the deletion, it SHALL reclaim them through Docker: every directory of the tree is re-owned to the current uid/gid and gets `u+rwx`; regular files, symlinks and device nodes are not touched. Then it SHALL retry once and prune the git registration. The reclaim counts as successful only when the container exits 0 and no directory of the tree is left unwritable or unreadable afterwards. When the reclaim does not run or does not help, the error SHALL keep today's path and manual remedy text, with the path single-quoted for the shell so a path with spaces or quotes stays one argument, and SHALL add the reason: `guard rejected`, `removal in progress`, `docker not found`, `docker unsupported`, `docker failed` or `reclaim ineffective`.

#### Scenario: Read-only directories owned by the user need no Docker

- **GIVEN** a registered worktree `<repo>/.worktrees/nergal/x` whose subdirectory `cache` is owned by the user with mode `0555`
- **WHEN** the removal runs with reclaim enabled
- **THEN** the directory SHALL be gone, the result SHALL be `Ok`, and the Docker reclaimer SHALL not be called

#### Scenario: Docker reclaims and the retry succeeds

- **GIVEN** the same worktree, a reclaimer whose unprivileged step does nothing, and a Docker step that makes `cache` writable
- **WHEN** the removal runs with reclaim enabled
- **THEN** the Docker step SHALL be called once with the canonical worktree path and the current uid/gid
- **AND** the directory SHALL not exist afterwards and the result SHALL be `Ok`

#### Scenario: Docker exits 0 without effect

- **GIVEN** the same worktree and a reclaimer whose Docker step exits 0 but leaves `cache` unwritable
- **WHEN** the removal runs with reclaim enabled
- **THEN** the result SHALL be an error that contains the path, the manual remedy and `reclaim ineffective`

#### Scenario: Docker not available

- **GIVEN** a worktree still blocked after the unprivileged step and no `docker` executable
- **WHEN** the removal runs with reclaim enabled
- **THEN** the error SHALL contain the path, the manual remedy and `docker not found`

#### Scenario: Unsupported Docker setups are skipped

- **GIVEN** a blocked worktree and a `docker` that is Podman, whose daemon reports `rootless` or `userns` in its security options, or whose endpoint is not a local `unix://` socket
- **WHEN** the removal runs with reclaim enabled
- **THEN** no container SHALL run and the error SHALL contain `docker unsupported`

#### Scenario: Other errors do not trigger a reclaim

- **GIVEN** a worktree whose deletion fails with an error other than `PermissionDenied`
- **WHEN** the removal runs with reclaim enabled
- **THEN** neither step SHALL run

#### Scenario: Directory already gone

- **GIVEN** a worktree path that no longer exists on disk
- **WHEN** the removal runs with reclaim enabled
- **THEN** it SHALL prune and return `Ok`, as it does today

#### Scenario: Automatic rollbacks never reclaim

- **GIVEN** a worktree with root-owned files whose creation is being rolled back by the MCP, Linear or ClickUp integration
- **WHEN** the rollback removes it
- **THEN** no reclaim step SHALL run and the existing error SHALL be returned

### Requirement: A failed reclaim keeps the session

When a user-initiated removal fails on the reclaim path (any of the reasons `guard rejected`, `removal in progress`, `docker not found`, `docker unsupported`, `docker failed`, `reclaim ineffective`), the command SHALL keep the session row, return an error with the path, the reason and the manual remedy, and SHALL not delete the branch. `delete_workspace` SHALL delete only the sessions whose worktree was removed and SHALL keep the workspace when any session was kept. Other removal failures keep today's behavior (warning, row deleted). The frontend SHALL reload the workspaces and show one error toast when a session or workspace delete is refused this way.

#### Scenario: Session kept after docker failed

- **GIVEN** a session whose worktree holds a root-owned directory and a Docker step that fails
- **WHEN** the user deletes the session
- **THEN** `delete_session` SHALL return an error containing `docker failed`, the session row SHALL remain, and its branch SHALL remain

#### Scenario: Retry after fixing the cause

- **GIVEN** a session kept after `docker failed` (git no longer lists its worktree)
- **WHEN** the user deletes it again with a working Docker
- **THEN** the guard SHALL accept the session's recorded worktree path and the deletion SHALL succeed

#### Scenario: Workspace kept when one session is kept

- **GIVEN** a workspace with sessions `a` (clean worktree) and `b` (reclaim fails)
- **WHEN** the user deletes the workspace
- **THEN** `a` SHALL be deleted, `b` and the workspace SHALL remain, and the command SHALL return an error naming `b`

### Requirement: The container acts only on the guarded directory

Before changing anything, the container SHALL check that the mounted directory has the device and inode numbers the guard read on the host, and SHALL exit without changes when they differ.

#### Scenario: Directory swapped after the guard

- **GIVEN** the guard accepted `<repo>/.worktrees/nergal/x` with `dev:ino` `A`, and the path now resolves to a directory with `dev:ino` `B`
- **WHEN** the Docker step runs
- **THEN** the container SHALL exit non-zero without changing any owner or mode, and the result SHALL be `docker failed`

### Requirement: One removal per worktree path at a time

Only one reclaiming removal SHALL run per existing canonical worktree path. A second one SHALL fail with `removal in progress` without touching the directory or calling the reclaimer. A path that does not exist needs no claim.

#### Scenario: Concurrent removal of the same worktree

- **GIVEN** a removal of `<repo>/.worktrees/nergal/x` is in progress
- **WHEN** a second removal of the same path starts
- **THEN** it SHALL return an error containing `removal in progress` and SHALL not call the reclaimer

### Requirement: Docker reclaim runs only on a guarded worktree path

Before the Docker step, the guard SHALL accept the path only when all hold: `<repo>/.worktrees` and `<repo>/.worktrees/nergal` are real directories; the worktree is exactly one component below `<repo>/.worktrees/nergal`; the worktree is a real directory owned by the current uid; its canonical path has no `,`, `:`, `"` or newline; and it was a `worktree` entry of `git worktree list --porcelain` captured before `git worktree remove` ran, or it is the worktree path Nergal recorded for the session being deleted. The reclaimer SHALL receive the canonical path as one argument, never through a shell.

#### Scenario: Registered worktree is accepted

- **WHEN** the guard checks a registered worktree `<repo>/.worktrees/nergal/x` owned by the user
- **THEN** it SHALL return its canonical path

#### Scenario: Registration snapshot taken before git removes it

- **GIVEN** a registered worktree `<repo>/.worktrees/nergal/x` with an unwritable subdirectory, so `git worktree remove --force` fails halfway and drops the registration
- **WHEN** the guard checks the path against the snapshot taken before that command
- **THEN** it SHALL accept it

#### Scenario: Wrong depth or location is rejected

- **WHEN** the guard checks `<repo>`, `<repo>/.worktrees`, `<repo>/.worktrees/nergal`, `<repo>/.worktrees/nergal/x/sub`, `<repo>/src`, or `/`
- **THEN** it SHALL reject each one

#### Scenario: Symlinked component is rejected

- **GIVEN** `<repo>/.worktrees` is a symlink to another directory that contains `nergal/x`
- **WHEN** the guard checks `<repo>/.worktrees/nergal/x`
- **THEN** it SHALL reject it

#### Scenario: Symlinked worktree is rejected

- **GIVEN** `<repo>/.worktrees/nergal/link` is a symlink to `/tmp/elsewhere`
- **WHEN** the guard checks it
- **THEN** it SHALL reject it

#### Scenario: Unregistered directory is rejected

- **GIVEN** `<repo>/.worktrees/nergal/stray` is a directory that the pre-removal snapshot does not list
- **WHEN** the guard checks it
- **THEN** it SHALL reject it

#### Scenario: Traversal and mount-syntax characters are rejected

- **WHEN** the guard checks `<repo>/.worktrees/nergal/../../src`, `<repo>/.worktrees/nergal/a,b`, `<repo>/.worktrees/nergal/a:b` or a slug containing `"`
- **THEN** it SHALL reject each one

### Requirement: Deletes do not block the UI thread

`delete_session`, `delete_workspace` and `cleanup_merged_session` SHALL run worktree removal on Tauri's blocking pool, never on the main thread or an async worker.

#### Scenario: A slow reclaim keeps the window responsive

- **GIVEN** a reclaim that takes 10 seconds
- **WHEN** the user deletes the session
- **THEN** the window SHALL keep processing input while the delete runs
