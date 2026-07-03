## Context

`commands.rs` (3979 lines, 120 commands) and `db.rs` (2298 lines, single `impl Database` at db.rs:186) are the backend's two god modules. Rust supports splitting an inherent impl across files within the crate (multiple `impl Database` blocks), and Tauri's `generate_handler!` only needs the paths it names to resolve — both splits can be zero-behavior-change. The domain seams below come from clustering the actual command names and DB method groups on disk.

## Goals / Non-Goals

**Goals:**
- Domain-named files a contributor can navigate; smaller PR collision surface; natural homes for per-domain tests.
- Byte-identical command surface: `generate_handler![commands::x, …]` resolves unchanged.

**Non-Goals:**
- Logic/signature/visibility changes beyond re-export plumbing; lock-discipline fixes (`release-db-lock-during-io` owns those); frontend anything.

## Decisions

### D1: command domain seams (from the real 120-command inventory)

| Module | Contents (representative) |
|---|---|
| `commands/workspaces_sessions.rs` | create/delete/rename session+workspace, reorder, merge_session, cleanup_merged_session, env-shell get/set, list_available_agents, resolve_default_agent, apply_theme_to_agents, setup_hooks |
| `commands/git.rs` | git_* (stage/stash/commit/push/rename), get_git_status, get_recent_commits, get_commit_files, get_file_diff, get_session_git_info, get_session_changed_files, init_git_repo, list_branches, resolve_repo_root, pull_target_into_session |
| `commands/ship_pr.rs` | git_ship, create_pr, gh_pr_merge, gh_available, enable_pr_auto_merge, get_pr_* (checks/diff/preview/status), list_prs, poll_pr_checks, has/complete_pending_merge |
| `commands/conflicts.rs` | get_conflicted_files, get_file_conflict_versions, save_conflict_resolution, build_conflict_prompt, enqueue_conflict_context |
| `commands/plans.rs` | get/save/load/diff/approve/reject_plan, list_session_plans, get_session_plan_capability, submit_plan_decision, set_pending_annotations |
| `commands/annotations.rs` | *_annotation(s), *_spec_annotation(s), count_spec_annotations_by_prefix |
| `commands/tasks_costs.rs` | get_tasks, delete_task(s), clear_completed_tasks, get_cost, get_transcript |
| `commands/obsidian.rs` | obsidian_*, pin/unpin_vault_note, list_pinned_notes, read/resolve_vault_note |
| `commands/openspec.rs` | list_openspec_changes, read/write_openspec_artifact, get/set_workspace_openspec_dir, watch_openspec_for_session, get/set_workspace_plans_dir |
| `commands/files.rs` | list_directory, read/write_file_content, search, search_files, validate_path, open_in_editor, detect_editors |
| `commands/config_misc.rs` | get/save_config, send_notification, get_context_injection_tier, drain_pending_deeplinks, submit_ask_answer¹ |
| `commands/shared.rs` | `pub(crate)` helpers: resolve_session_cwd, resolve_session_base, and whatever the extraction surfaces |

¹ dies entirely if `remove-dead-askuser-slice` lands first — either order works.

`commands/mod.rs`: `mod` decls + `pub use <module>::*;` per file so `commands::<name>` paths in `generate_handler!` (lib.rs:367) resolve without touching lib.rs. Duplicate-name collisions are impossible (the current single file already proves global uniqueness).

### D2: db.rs split via multiple `impl Database` blocks

`db/mod.rs` keeps: struct, `open()` (db.rs:206), migration runner + list, connection helpers, shared row-mapping utilities; domain files (`workspaces`, `sessions`, `transcripts` (+summaries), `cross_session`, `tasks_costs`, `annotations` (+spec), `scratchpad`, `obsidian`, `panels_misc` (geometry, pinned notes, env-shell suggestions, openspec/plans dirs)) each hold one `impl Database` block. **Alternative — trait-per-domain (`WorkspaceStore`, …)**: dyn-friendly and mock-friendly, but changes every call site's imports and adds indirection with no current consumer needing abstraction. Rejected — inherent impls keep call sites byte-identical.

### D3: split order — db.rs first, then commands.rs

db.rs is smaller, has zero Tauri macro involvement, and its success (cargo test green) de-risks the bigger commands split. Within commands.rs: extract one domain per commit, `cargo check` between commits, `shared.rs` first (helpers are cross-domain).

### D4: history preservation

Each split commit is 100% move (verifiable with `git diff --color-moved=dimmed-zebra`); record the commit hashes in `.git-blame-ignore-revs` (create it; document in the commit message). No logic edits ride along.

### D5: sequencing vs pending changes

Pending changes editing these files surgically (`release-db-lock-during-io` — 3 commands; `remove-dead-askuser-slice` — 1 command + handler entry; `session-child-fk-cascade` — db migration list) land **before** the split or rebase trivially (the split moves whole fns; git rerere/move-detection handles it). Do NOT run the split concurrently with those branches in flight.

### D6: preserve `#[cfg]` gates across the split (CLAUDE.md cross-platform invariant)

`commands.rs` (7 `#[cfg(` sites), `lib.rs` (9), and `db.rs` (2) contain platform-gated code — `cfg(unix)` / `cfg(target_os = "…")` commands, invoke_handler entries, and their `cfg(not(...))` stubs. A domain split MUST move each gated item **with its gate and its counterpart stub intact** into the destination module (or, when a gate spans an invoke_handler block in `lib.rs`, keep the block's `#[cfg]` structure). The split is 100% move (D4), so no gate is added or removed — but a careless move that drops a `cfg(not(unix))` stub or re-homes a Unix-only fn without its Windows stub silently breaks a target platform. Because the split touches only the Linux dev host's compiler, the two cross-platform CI gates (`windows-check`, `macos-cross-check`) are the authoritative check that the gates survived — they MUST be green post-split (see Verification), not just the local Linux `cargo check`.

## Risks / Trade-offs

- [Hidden coupling: a command references a private item defined elsewhere in the old file] → compiler-guided; promote to `pub(crate)` in `shared.rs` (Impact notes visibility widening is allowed only for this).
- [Review of a ±6000-line diff] → per-domain commits + `--color-moved` review protocol stated in the PR description.
- [In-flight branch conflicts] → D5 sequencing; announce the split window.
- [A moved `#[cfg]`-gated command loses its gate/stub → silent macOS/Windows breakage the Linux build won't catch] → D6; the `windows-check` + `macos-cross-check` CI gates are the backstop and must stay green.

## Open Questions

- None blocking. Exact file membership for a handful of ambiguous commands (e.g. `merge_session` git-vs-session) is the implementer's judgment call within D1's table — document deviations in the PR.
