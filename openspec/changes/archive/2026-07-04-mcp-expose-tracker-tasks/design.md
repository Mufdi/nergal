## Context

The MCP server (session-dir socket, 0600, off by default per `nergal-mcp-server` spec) serves 11 session/messaging/worktree tools. The tracker mirrors (`clickup_tasks` + subdata, `linear_issues` + cycles) and ship-flow PR/checks state are maintained regardless — the marginal cost of exposure is tool definitions + read mappers. The panels' own read paths (`clickup/mirror.rs read_tasks`, Linear equivalents) prove the query shapes.

## Goals / Non-Goals

**Goals:**
- A v1 tool surface an agent can use for "my tasks / this task / my PR / my branch" without user round-trips.
- Zero new write paths; zero live tracker API calls from tools.

**Non-Goals:**
- Tracker write tools; exposing anything requiring live API auth beyond what mirrors hold; building now.

## Decisions

### D1: four tools, unified tracker namespace

`list_tracker_tasks(filter)` / `get_tracker_task(id)` unify ClickUp+Linear behind one vocabulary (source field per row: `"clickup" | "linear"`), because the agent's question is "my tasks", not "my ClickUp tasks". Per-tracker names (`list_clickup_tasks`, …) double the surface and leak vendor vocabulary into prompts. The unified row is the intersection + a `fields` bag for tracker-specifics (mirrors the panels' shared column set).

`get_pr_status(session_id?)` and `get_git_status(session_id)` read what the status bar / PR chips already compute; default `session_id` = caller's own (`whoami` precedent).

### D2: mirror-only reads with staleness metadata

Every response carries `mirror_updated_at` (per tracker) so the agent can judge freshness instead of the server guessing; no tool triggers a live poll (the 45s cadence is the freshness contract, same as the panels). **Alternative — on-demand refresh parameter**: invites rate-limit abuse from agent loops and makes tool latency unpredictable. Rejected for v1.

### D3: scoping = caller's workspace

A session's tools see only its own workspace's tracker bindings and git state (the socket already authenticates per-session identity for `whoami`/messaging). Cross-workspace task listing is a later, deliberate decision — not a default. This is the material security/scoping decision; flagged for user sign-off with the design review.

### D4: payload budgets

`list_tracker_tasks` caps rows (default ~50, `limit` param, ordered by `date_updated DESC` like the panel) and returns summary rows (no descriptions/comments); `get_tracker_task` returns detail + subdata counts + comments trimmed to a budget. Follows the session-directory precedent (summaries lazy, no fan-out).

## Risks / Trade-offs

- [Unified vocabulary hides tracker-specific fields an agent needs] → `fields` escape hatch per row; revisit per-tracker tools only on demonstrated need.
- [Agents poll the tools in a loop] → cheap mirror reads by design (index-backed after the pending `clickup-subdata-indexes` change — soft dependency worth sequencing first).
- [Scope creep toward writes] → the delta spec pins read-only; write proposals require their own change with the human-gate discussion.

## Open Questions

- **User sign-off on D3 scoping** (workspace-scoped vs cross-workspace) before the build change is seeded.
- Whether `get_pr_status` should include check-run details or just the rollup (build change can start with rollup).

## Revision 1: user sign-off (2026-07-04)

The user's decision — "incluyamos todo lo referente a clickup y linear en el mcp"
(expose all ClickUp + Linear via MCP) — resolves the open sign-off items:

- **D1 unified naming: ACCEPTED.** Both trackers surface through `list_tracker_tasks` /
  `get_tracker_task` with a `source: "clickup" | "linear"` field, plus `get_pr_status` /
  `get_git_status`. All four v1 tools are in scope.
- **D3 scoping: workspace-scoped v1 (ACCEPTED, conservative default).** A session's tools
  see only its own workspace's tracker bindings + git state. "Expose all ClickUp/Linear"
  means both trackers, unified — NOT cross-workspace, which would be a *widening* of the
  per-session scope that stays deferred (it would need its own explicit sign-off).
- **`get_pr_status`: rollup-only for v1** (check-run details deferred) — the build change
  starts with the rollup, per the design's own Open Question.
- **Read-only guarantee stands**: zero write paths, zero live tracker API calls (mirror
  reads only). Writes remain behind the human-gated UI.

**Decision: GO.** Seed the build change (the four read-only tools + dispatch + mirror read
mappers + `mirror_updated_at` staleness + D4 payload caps). Soft dependency
`clickup-subdata-indexes` (index-backed mirror reads) already landed (Wave 2.11), so the
build is unblocked.
