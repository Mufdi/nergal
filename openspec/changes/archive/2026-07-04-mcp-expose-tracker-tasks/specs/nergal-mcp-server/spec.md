# nergal-mcp-server

Read-only tracker/PR/git tools on the existing MCP surface (design-level; binds when the build change lands).

## ADDED Requirements

### Requirement: Read-only tracker and repo state tools

The MCP server SHALL expose read-only tools over state nergal already maintains: `list_tracker_tasks` and `get_tracker_task` (unified over the ClickUp/Linear mirrors, mirror-only reads with staleness metadata, never live tracker API calls) and `get_pr_status` / `get_git_status` (ship-flow and status-bar state). These tools SHALL NOT write to any tracker and SHALL be scoped to the calling session's workspace.

#### Scenario: agent lists its tasks

- **WHEN** an agent calls `list_tracker_tasks` with an assignee filter
- **THEN** it receives summary rows from the local mirrors (both trackers, source-tagged) with `mirror_updated_at` freshness metadata, and no tracker API call is made

#### Scenario: agent checks its PR

- **WHEN** an agent calls `get_pr_status` for its own session
- **THEN** it receives the PR + checks rollup the ship-flow backend already tracks

#### Scenario: scoping is per workspace

- **WHEN** a session calls the tools
- **THEN** results cover only its own workspace's tracker bindings and repo state

#### Scenario: no write surface

- **WHEN** the v1 tools are enumerated
- **THEN** none accepts a mutation; tracker writes remain behind the human-gated UI flows
