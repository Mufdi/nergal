## 1. MCP tracker-tool timeout

- [ ] 1.1 Add a named `TRACKER_TOOL_TIMEOUT: Duration` const (e.g. 20s) in mcp/mod.rs
- [ ] 1.2 Add an async helper that runs a `FnOnce() -> Result<Value> + Send + 'static` via `tokio::task::spawn_blocking` bounded by `tokio::time::timeout`, mapping timeout → a JSON-RPC error and JoinError → INTERNAL_ERROR
- [ ] 1.3 Route the `get_pr_status` + `get_git_status` dispatch arms (mod.rs:612-650) through the helper, moving a `ctx.clone()` + owned `caller`/`session_id` into the closure; keep `tracker.rs` sync
- [ ] 1.4 Confirm the timeout error path returns a clean tool error (client already handles tool errors)

## 2. Extract read_openspec_artifact dispatch helper

- [ ] 2.1 Extract active-first-then-archive path resolution (openspec.rs:209-222) into pure `fn resolve_artifact_path(openspec_dir: &Path, change_name: &str, artifact_path: &str) -> Result<PathBuf, String>`
- [ ] 2.2 `read_openspec_artifact` delegates to it (behavior-identical)
- [ ] 2.3 Unit tests: active-dir hit, archive fallback when active missing, `_master` → specs dir, traversal (`../`) rejected via fs_guard

## 3. Vet stage/unstage paths (security-adjacent)

- [ ] 3.1 `worktree::stage_file`: `fs_guard::resolve_within_base(cwd, path)?` before `git add --`, pass the vetted path
- [ ] 3.2 `worktree::unstage_file`: same vetting before `git restore --staged --`
- [ ] 3.3 Confirm legit repo-relative paths still stage (no regression to the git panel flow)

## 4. Verify

- [ ] 4.1 `cargo clippy --all-targets -- -D warnings` + `cargo test` + `cargo fmt --check` green
- [ ] 4.2 `tsc --noEmit` green (no frontend change)
