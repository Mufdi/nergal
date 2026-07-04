# Build log — mcp-tracker-tools

## Files changed

- `src-tauri/src/mcp/tracker.rs` (new, 582 lines) — the 4 tools' bodies:
  `list_tracker_tasks`, `get_tracker_task`, `get_pr_status`, `get_git_status`, plus
  `ListTrackerTasksFilter`/`parse_list_filter`, the ClickUp/Linear → unified-row mappers
  (`clickup_row_to_unified`/`linear_row_to_unified`), comment-budget trimming
  (`trim_text`, `COMMENT_BUDGET`/`COMMENT_TEXT_BUDGET`), and 6 unit tests over a seeded
  in-memory DB.
- `src-tauri/src/mcp/mod.rs` — `pub mod tracker;`; 4 new tool definitions in
  `tool_definitions()` (each description states the read-only + workspace-scoped
  contract per task 2.5); 4 new dispatch arms in `dispatch()`; updated
  `tools_list_returns_directory_and_messaging_tools` to the 15-tool list; added 3
  dispatch-level tests (`list_tracker_tasks_via_dispatch_empty_mirrors`,
  `get_pr_status_without_identity_is_invalid_params`,
  `get_git_status_without_identity_is_invalid_params`) to cover the identity-gate path
  that `tracker.rs`'s own tests (mapper-level, not through `dispatch()`) don't exercise.
- `src-tauri/src/commands/mod.rs` — `mod shared;` → `pub(crate) mod shared;` so
  `tracker.rs` can call `commands::shared::resolve_session_cwd`/`resolve_session_branch`
  (the exact fns `ship_pr`/`git` commands already use) instead of re-deriving cwd/branch
  resolution.

## Design decisions made during the build

**D3 "workspace-scoped" resolves differently per tool, because the data model has no
per-workspace tracker binding.** The design's D3 assumed a workspace's tracker bindings
are a queryable scope. In the actual schema, ClickUp/Linear mirrors are a single global
per-app account (no `workspace_id` column on `clickup_tasks`/`linear_issues`, no
workspace→space/team binding in `config.rs`) — the only per-session tracker linkage that
exists is `Session.active_clickup_task_id`/`pinned_clickup_task_ids`/
`active_linear_issue_id`/`pinned_linear_issue_ids`. Given that, scoping is implemented as:

- `list_tracker_tasks`/`get_tracker_task`: mirror-only reads with **no per-session
  filtering** — they return the full unified mirror (capped/ordered per D4), same as the
  ClickUp/Linear panels themselves show (which are also global, not per-workspace-session
  filtered today). This is a narrower interpretation of D3's "workspace-scoped" than
  the design assumed the code supports; flagging for reviewer eyeball since it wasn't
  spelled out in tasks.md.
- `get_pr_status`/`get_git_status`: scoped literally — `session_id` (when passed) must
  equal the caller's own resolved identity, else the call fails with a
  `"workspace-scoped: ..."` error. This is the scoping model the design's language maps
  onto cleanly (a session's own git/PR state), and it's what's tested
  (`get_pr_status_rejects_cross_session_lookup`, `get_git_status_rejects_cross_session_lookup`).

**PR/git status is not a mirror read — no cache exists for it.** `get_pr_status`/
`get_git_status` shell out to local `git`/`gh` on every call via `worktree::pr_status`,
`worktree::pr_checks`, `worktree::current_branch`, `worktree::is_worktree_dirty`,
`worktree::commits_ahead_count` — the exact same fns `commands::ship_pr` and the status
bar's git command already call. There is no persisted PR/git cache in this codebase to
read instead; the spec's "read-only guarantee" (no live *tracker* API call, no mutation)
still holds — `git`/`gh` are not ClickUp/Linear calls and nothing is written — but this is
a live subprocess call, unlike D2's cache-only tracker reads. Flagging so the reviewer
doesn't assume `get_pr_status`/`get_git_status` are mirror-backed like the other two.

## Verification (all commands run for real; no rtk false-green)

```
$ cd src-tauri && rtk proxy cargo clippy --all-targets -- -D warnings; echo $?
    Checking nergal v0.4.1 (/home/felipe/Projects/cluihud/src-tauri)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 8.97s
0
```

```
$ cd src-tauri && cargo test
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.31s
     Running unittests src/lib.rs ...
     Running unittests src/main.rs ...
     Running tests/agent_foundation_cost.rs ...
     Running tests/agent_foundation_migration.rs ...
   Doc-tests nergal
cargo test: 816 passed, 1 ignored (5 suites, 2.34s)
```

New tests included in the 816: 6 in `mcp::tracker::tests` (unified ordering, source
filter + limit, get_tracker_task cross-tracker id lookup, PR/git cross-session rejection,
PR default-to-caller) + 3 in `mcp::tests` (dispatch-level: empty-mirror listing,
unidentified-caller INVALID_PARAMS for `get_pr_status`/`get_git_status`).

```
$ cd src-tauri && cargo fmt --check; echo $?
0
```

(First `cargo fmt --check` run caught 3 pre-existing formatting deltas — one in the new
dispatch code, two in `tracker.rs` — fixed with `cargo fmt`; re-ran clean.)

```
$ npx tsc --noEmit
TypeScript: No errors found
0
```

## Left for reviewer to eyeball

- The D3 scoping divergence above (`list_tracker_tasks`/`get_tracker_task` are
  unscoped/global, not per-session-filtered) — confirm this matches the user's
  "incluyamos todo lo referente a clickup y linear en el mcp" sign-off intent, or whether
  a follow-up change should add per-session filtering (e.g. restrict to the session's
  pinned/active task+issue ids) before this ships enabled-by-default anywhere.
- `get_pr_status`/`get_git_status` hit `git`/`gh` synchronously on the daemon's
  connection-handling task per call — no timeout wrapper visible in `worktree::pr_status`/
  `pr_checks` beyond whatever `gh`/`git` themselves impose. Not a regression (the ship-flow
  panel already accepts this), but worth a glance given MCP callers are less
  patient/observable than a UI panel.
- Task 4.3 (manual MCP client smoke test) left unchecked per instructions — needs a human
  or an MCP-client session with `mcp_server_enabled=true` to drive `tools/call` for real.
