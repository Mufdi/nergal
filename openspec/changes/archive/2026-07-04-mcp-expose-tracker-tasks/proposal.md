## Why

Nergal already maintains rich server-side state agents would use — the ClickUp/Linear task mirrors (SQLite, poll-refreshed) and git/PR/CI state (ship-flow backend) — but its MCP server exposes none of it. Verified: `src-tauri/src/mcp/mod.rs` dispatches exactly 11 tools (`whoami`, `list_sessions`, `get_session`, `search_sessions`, `send_to_session`, `read_messages`, `list_threads`, `request_session_resume`, `create_worktree_session`, `get_worktree_request_status`, `cancel_worktree_request`) — all session/messaging/worktree. Transport, registration (CC/Codex/OpenCode registrars), and the socket-auth model already exist, so read-only tracker/PR tools are an incremental extension with outsized agent-workflow value (an agent can ask "what's assigned to me?" or "is my PR green?" without the user alt-tabbing). **Direction finding — this change designs the tool surface; the build is a follow-up.**

## What Changes

- **Design the read-only v1 tool surface** (design.md D1): `list_tracker_tasks` (unified over ClickUp/Linear mirrors, filterable by assignee/state/space), `get_tracker_task(id)` (detail + subdata from the mirror, never a live API call), `get_pr_status(session?)` (PR + checks state the ship-flow backend already polls), `get_git_status(session)` (branch/dirty/ahead — the status-bar data).
- **Read-only guarantee**: v1 tools never write to trackers and never trigger live tracker API calls (mirror-only reads, matching the panel's own read path) — writes stay behind the human-gated UI (and the existing closure-token flow).
- **Design decisions to settle** (design.md): unified-vs-per-tracker tool naming, mirror-staleness signaling, auth/scoping (which session may see which workspace's tasks), and payload budgets (mirror rows can be large).
- No source edits land from this change; if accepted, a follow-up build change implements per the design.

## Capabilities

### New Capabilities

_None._

### Modified Capabilities

- `nergal-mcp-server`: the MCP surface is extended (design-level) with read-only tracker/PR/git tools; requirements below bind when the build change lands.

## Impact

- **Artifacts only** now; follow-up touches `src-tauri/src/mcp/mod.rs` (tool defs + dispatch), thin read fns over `clickup::mirror`/`linear` mirror + ship-flow status, and the MCP capability docs.
- **Risk**: LOW for the design; the build's main risks (payload size, workspace scoping) are exactly what the design settles first.
- **Out of scope**: write tools (state changes, comments) — explicitly deferred; exposing Obsidian or scratchpad state (different discussion).
