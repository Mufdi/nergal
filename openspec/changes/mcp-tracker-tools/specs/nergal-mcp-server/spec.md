# nergal-mcp-server

## ADDED Requirements

### Requirement: Read-only tracker/PR/git MCP tools

The MCP server SHALL expose read-only tools for tracker tasks, PR status, and git
status. Tracker-task reads (`list_tracker_tasks`/`get_tracker_task`) read only the local
ClickUp/Linear SQLite mirrors — the same single per-app tracker account the panels
already display — never issuing a live tracker API call and never mutating state.
PR/git reads (`get_pr_status`/`get_git_status`) are gated to the caller's own session
identity: a request naming a different session SHALL be rejected.

The schema has no per-workspace tracker binding (the mirrors are one global account, not
partitioned per workspace/session), so tracker-task tools are NOT per-session filtered;
their scope is exactly the global mirror the user already sees in every session's panel.
Per-session scoping applies only where per-session state exists — the caller's own git/PR
state.

#### Scenario: unified tracker task listing

- **WHEN** an MCP client calls `list_tracker_tasks`
- **THEN** it receives capped summary rows (default 50, hard-clamped max) unified over
  ClickUp + Linear (each tagged `source`), ordered `date_updated DESC`, with a per-tracker
  `mirror_updated_at`, read from the global local mirror (no live tracker call)

#### Scenario: no write, no live tracker call

- **WHEN** any of the tracker/PR/git tools runs
- **THEN** tracker-task tools read only the local mirror and PR/git tools read only local
  `git`/`gh` state — no ClickUp/Linear API call is issued and no state is mutated

#### Scenario: PR + git status gated to the caller's session

- **WHEN** `get_pr_status` or `get_git_status` is called
- **THEN** it returns the PR rollup / branch-dirty-ahead state for the caller's own
  session by default, and rejects a request naming a session other than the caller's own
  (no cross-session read)
