## Why

`squash_merge` (`src-tauri/src/worktree.rs:58-71`) builds its temporary detached worktree at a **fixed literal path per repo** — `<repo>/.worktrees/nergal/_merge_tmp` (59-62) — checked, force-removed (64-70), and recreated with no lock. Two ship/merge operations into the same repo close together interleave on that shared path: the second call's `git worktree remove --force` deletes the first's in-progress merge worktree mid-merge, so a squash-merge commit can be built from the wrong branch's content (or fail confusingly). The codebase already has the standard fix pattern: `atomic_write.rs:20-35` disambiguates temp names with a process-local `AtomicU64` counter + pid.

## What Changes

- **Derive `tmp_dir` from a unique per-call suffix**: `_merge_tmp.<pid>.<seq>` where `seq` comes from a module-local `static AtomicU64` (the `atomic_write.rs:20` technique). Each merge gets its own worktree; concurrent merges cannot touch each other's.
- **Replace the "clean up any leftover" pre-remove of the shared path with a stale-sweep**: on entry, remove only *stale* leftovers (any `_merge_tmp.*` sibling whose embedded pid is not alive), plus best-effort `git worktree prune`; and always remove the call's own tmp worktree on exit (success and error paths — today's cleanup relies on the *next* call). **Cross-platform invariant (CLAUDE.md)**: the pid-liveness check uses the `sysinfo` crate (already a dependency, `Cargo.toml:107`, used in `pty.rs`/`platform/mod.rs` among others) — no new `cfg(unix)`/`cfg(windows)` seam, no `/proc` reads, no `libc::kill`.
- Behavior otherwise unchanged: detached worktree at target tip, squash merge, commit, remove.

## Capabilities

### New Capabilities

_None._

### Modified Capabilities

- `ship-flow`: concurrent squash-merges into the same repository use isolated temporary worktrees and cannot corrupt each other.

## Impact

- **`src-tauri/src/worktree.rs`**: `squash_merge` (~58-71 + its exit paths) — unique path derivation, own-cleanup on exit, stale-sweep on entry. Single caller: `commands.rs:1271`.
- **Risk**: MEDIUM severity averted (merging wrong content into main) with LOW complexity; the subtlety is leftover accumulation, handled by the stale-sweep + exit cleanup + `git worktree prune`.
- **Out of scope**: a repo-level merge mutex (serializing merges — heavier and unnecessary once paths are isolated; noted as rejected alternative); the multi-worktree ship pipeline itself.
