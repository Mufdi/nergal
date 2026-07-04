## Why

The archived `release-db-lock-during-io` change fixed three verified offenders (`delete_session`, `create_pr`, `git_ship`) and explicitly deferred "auditing every other `db.lock()` caller" to a follow-up sweep. That sweep found **53 more sites across 10 files** where the global `Mutex<Database>` guard is held across git/`gh` subprocesses, network pushes, or filesystem I/O — freezing every other DB consumer (session list, panels, hook persistence, the ClickUp/Linear pollers, the MCP server) for the duration. On a slow disk or remote this holds the data layer hostage for seconds. The invariant exists but is only enforced in three spots, so a reviewer can't tell whether a lock-across-git is a bug or intentional.

## What Changes

- **Broaden `db-lock-discipline` from three named commands to a general invariant**: the global DB mutex is never held across git/`gh` subprocess, network, or filesystem I/O — anywhere. Every one of the 53 offenders is restructured to a read → drop → work (→ re-acquire) shape.
- **Three uniform fix patterns**:
  - **Simple-drop** (39 sites, e.g. `git_push`, `git_commit`, `poll_pr_checks`, `list_prs`, all read-only `worktree::*` command wrappers): scope the guard to a block that extracts owned values (cwd, branch, base), let it drop, then run the subprocess.
  - **Read → drop → I/O → re-acquire** (interleaved cases with a trailing DB write: `git_rename_branch`, `create_session`, `cleanup_merged_session`, and the `clickup`/`linear`/`mcp` `create_worktree`→`create_session` trio): extract inputs under a short guard, drop, do the git/network work, re-acquire briefly for the final write, re-validating row existence and no-op'ing if the row vanished (TOCTOU-safe).
  - **Gather → drop → loop-I/O** (interleaved per-N loops that call `MocBuilder::build`/`BacklinkUpdater::propagate`/`write_session_log_footer`: `delete_workspace`, `delete_session`, `hooks/server.rs::finalize_session_obsidian`, `lib.rs` crash-recovery drain + `queue_close_markers`): gather every session's DB-derived inputs under one short guard, drop, then loop the filesystem I/O guard-free.
- **Split `MocBuilder::build` into a DB-read phase and a file-write phase** so the markdown write no longer runs under `&Database`. This is the shared enabler for the gather-drop-loop pattern (the archived change flagged this exact split as follow-up-worthy). `MocBuilder::build` currently does 3 quick DB reads then pure file I/O — it does NOT shell out to git, so the split is clean.

## Capabilities

### New Capabilities

_None._

### Modified Capabilities

- `db-lock-discipline`: broaden the ADDED requirement (currently scoped to `delete_session`/`create_pr`/`git_ship`) into a general invariant covering every DB-mutex holder, plus a requirement that DB-plus-I/O helpers (`MocBuilder::build`) expose a guard-free file-write phase.

## Impact

- **`src-tauri/src/commands/git.rs`** (24 sites), **`ship_pr.rs`** (10), **`conflicts.rs`** (5), **`openspec.rs`** (3), **`workspaces_sessions.rs`** (5 incl. 4 interleaved), **`src/clickup/mod.rs`** (1), **`src/linear/mod.rs`** (1), **`src/mcp/worktree_sessions.rs`** (1), **`src/hooks/server.rs`** (1), **`src/lib.rs`** (2 recovery/close sweeps).
- **`src-tauri/src/obsidian/moc.rs`**: `MocBuilder::build` split into `gather` + `render_and_write` (or equivalent); all callers updated.
- No frontend-visible signature or behavior changes — pure internal concurrency discipline.
- **Risk**: MEDIUM — reordering around the guard can introduce TOCTOU on the interleaved write-back cases; mitigated by re-validate-on-re-acquire. The 39 simple-drop sites are near-mechanical. The `MocBuilder` split touches a shared helper used by 5 callers.
- **Out of scope**: replacing `Mutex<Database>` with a connection pool / async DB layer (architectural, separate discussion). Adding a git-diff to `MocBuilder` (the :230 comment's aspiration — unrelated).
