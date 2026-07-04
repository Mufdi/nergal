# mcp-tracker-tools

## Why

`mcp-expose-tracker-tasks` (design, archived) settled the surface and the user signed off
GO: expose all ClickUp + Linear via read-only MCP tools so an agent can ask "what's
assigned to me / is my PR green / what's my branch state" without the user alt-tabbing.
This is the build of that design. The mirrors (`clickup_tasks`+subdata, `linear_issues`+
cycles) and ship-flow PR/git state are already maintained; the marginal cost is tool defs
+ read mappers. `clickup-subdata-indexes` (index-backed reads) landed in Wave 2.11, so the
reads are cheap.

## What Changes

- **Add 4 read-only MCP tools** to the dispatcher (`src-tauri/src/mcp/mod.rs`, which today
  serves 11 session/messaging/worktree tools):
  - `list_tracker_tasks(filter?)` — unified over ClickUp+Linear mirrors, each row carries
    `source: "clickup" | "linear"` + the shared column set + a `fields` bag for
    tracker-specifics (design D1). Summary rows only (no descriptions/comments), capped
    (default ~50, `limit` param, ordered `date_updated DESC` — design D4).
  - `get_tracker_task(id)` — detail + subdata counts + comments trimmed to a budget, from
    the mirror only (design D4).
  - `get_pr_status(session_id?)` — the PR + checks ROLLUP the ship-flow backend already
    computes (rollup-only v1 per the sign-off; default session = caller's own).
  - `get_git_status(session_id)` — branch/dirty/ahead (the status-bar data).
- **Read-only guarantee (design spec)**: no tool writes to a tracker or triggers a live
  tracker API call — mirror reads only, matching the panels' read path.
- **Staleness (D2)**: every tracker response carries `mirror_updated_at` per tracker so the
  agent judges freshness; no tool triggers a live poll.
- **Scoping (D3, signed off)**: a session's tools see only its own workspace's tracker
  bindings + git state. Cross-workspace is deferred.

## Revision 1: scoping reality (build finding)

D3 assumed a queryable per-workspace tracker binding. The schema has none — the ClickUp/
Linear mirrors are a single global per-app account (no `workspace_id` on `clickup_tasks`/
`linear_issues`; the only per-session tracker linkage is `active/pinned_*_id` on
`Session`). So the implemented scoping is: `list_tracker_tasks`/`get_tracker_task` return
the **global** mirror (exactly what the panels already show — no new exposure, the mirror
is the user's own single account), while `get_pr_status`/`get_git_status` are **identity-
gated** (a `session_id` other than the caller's own is rejected). Also: PR/git status is
not a mirror read — it shells out to local `git`/`gh` live on each call (no persisted
cache exists), which is still read-only and issues no tracker API call. The spec delta
reflects this reality.

## Capabilities

### Modified Capabilities

- `nergal-mcp-server`: the MCP tool surface gains 4 read-only tracker/PR/git tools
  (list/get tracker task, PR status rollup, git status), mirror-only + workspace-scoped.

## Impact

- **`src-tauri/src/mcp/mod.rs`**: 4 tool defs + dispatch arms; thin read mappers over
  `clickup::mirror`/`linear` mirror queries (reuse the `read_tasks`-shape) + the ship-flow
  status fns the status bar uses.
- **Risk**: LOW-MEDIUM — read-only, but payload size + workspace scoping are the care
  points (bounded by D4 caps + D3 scoping, both settled in the design).
- **Out of scope**: write tools, live API calls, cross-workspace listing, check-run detail
  (rollup only) — all deferred per the design.
