## Why

Three small polish items deferred from the audit work order (follow-up 2 + 2.6):

1. **MCP tracker tools can block a tokio worker.** `get_pr_status`/`get_git_status` are dispatched directly inside the async `handle_connection` (`mcp/mod.rs:612-650`) and call blocking `worktree::pr_status`/`pr_checks`/`current_branch` (`gh`/`git` subprocesses via `.output()`, no timeout). A hung `gh` (network stall) blocks a tokio worker thread for the whole subprocess lifetime — several concurrent hangs can starve the daemon.
2. **`read_openspec_artifact`'s active/archive dispatch is inline and untested.** The "try active dir, else archive dir" path resolution (`commands/openspec.rs:209-222`) is baked into the Tauri command, so it can't be unit-tested without a DB + Tauri `State`.
3. **`worktree::stage_file`/`unstage_file` don't vet their path.** They run `git add -- <path>` with the raw caller-supplied `path` and no `fs_guard::resolve_within_base` (the guard used by the sibling fn at `worktree.rs:1147`). `git` bounds traversal in practice and `--` blocks flag injection, so this is defense-in-depth consistency, not a live vuln.

## What Changes

- **MCP timeout wrapper**: run `get_pr_status`/`get_git_status` via `tokio::task::spawn_blocking` bounded by `tokio::time::timeout` (a named `TRACKER_TOOL_TIMEOUT`). On timeout, return a JSON-RPC error to the client instead of blocking the worker. `DaemonContext` is `#[derive(Clone)]` (Arc-based), so a clone moves into the blocking closure cleanly; `tracker.rs` stays sync and unit-testable.
- **Extract dispatch helper**: pull the active/archive path resolution out of `read_openspec_artifact` into a pure `fn resolve_artifact_path(openspec_dir, change_name, artifact_path) -> Result<PathBuf, String>` and cover it with unit tests (active-hit, archive-fallback, `_master`, traversal-rejected).
- **Vet stage/unstage paths**: route `worktree::stage_file`/`unstage_file` through `fs_guard::resolve_within_base(cwd, path)` before invoking git, matching the existing vetted-path convention.

## Capabilities

### New Capabilities

_None._

### Modified Capabilities

_None — internal resilience/testability/hardening; no spec-level behavior change (same observable outputs, plus a bounded-timeout error path)._

## Impact

- **`src-tauri/src/mcp/mod.rs`**: the two tracker-tool dispatch arms wrapped in spawn_blocking + timeout (+ a small helper).
- **`src-tauri/src/commands/openspec.rs`**: `resolve_artifact_path` extracted + tests; `read_openspec_artifact` delegates.
- **`src-tauri/src/worktree.rs`**: `stage_file`/`unstage_file` vet the path.
- No frontend change. **Risk**: LOW. The stage/unstage vetting is security-adjacent (path-traversal defense) → security-auditor review. The timeout adds a new error path (timeout) the client must tolerate — it already handles tool errors.
- **Out of scope**: adding subprocess timeouts at the `worktree` layer for the whole ship-flow (root-cause, broad blast radius) — deliberately scoped to the MCP tools per the agreed follow-up.
