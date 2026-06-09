## Why

Today cluihud only *observes* agent state (via hooks + transcript watchers). It has no way to *expose* that state back to the agents it wraps. An agent cannot ask "what other sessions are running, and what are they doing?" without the user pasting the answer manually.

This is the foundation for cross-session collaboration (changes `cross-session-messaging` and `agent-spawned-worktrees` build on it) and has standalone value: an agent can discover its sibling sessions, read short summaries, and orient itself in a multi-session workspace. It is the first time Nergal turns its observed state into an agent-facing API.

This change replaces the foundation half of the archived `context-bridge` change (file-bus approach, never implemented). It absorbs the backlog items "MCP server cluihud con correlación nativa via `CLAUDE_CODE_SESSION_ID`", the directory half of "AI session summary" (M4), and "Surface background tasks y crons en Activities".

## What Changes

- New **single MCP daemon** owned by the cluihud app process, holding the global view of all live sessions across all workspaces.
- New **stdio MCP shim** subcommand (`cluihud mcp`) that each agent spawns; it bridges MCP JSON-RPC to the daemon over the existing Unix socket. Agent-agnostic (works for any agent with stdio MCP support: CC, Codex, Pi, OpenCode).
- **Identity correlation**: the shim reports `CLAUDE_CODE_SESSION_ID` (when present, CC v2.1.154+/163) and `CLUIHUD_SESSION_ID` (always, injected by our adapters) so the daemon maps each MCP connection to its cluihud session without user config.
- **Session directory** exposed via MCP tools (`list_sessions`, `get_session`, `whoami`) returning cheap, always-fresh metadata: name, workspace, branch, agent, mode/`waitingFor`, last-activity, recently-touched files, and (additively) `background_tasks` / `session_crons` captured from Stop hooks (CC v2.1.150).
- **Opt-in AI session summary** as an enrichment layer: when enabled, a detached cheap-model (haiku) runner produces a short rolling summary per session, surfaced in the directory. Shares the summarizer with M4's post-session MOC summary.
- New settings: enable/disable the MCP server, enable/disable AI summaries (global + per-project), summary refresh policy.

## Capabilities

### New Capabilities
- `cluihud-mcp-server`: The MCP daemon, the stdio shim transport, agent registration/config, identity correlation, and the `whoami` self-identification tool.
- `session-directory`: The `list_sessions` / `get_session` MCP tools, the session descriptor schema (incl. background tasks/crons), and the rule that directory data is cheap and always fresh (no AI required).
- `session-summary`: Opt-in AI summary enrichment — the shared summarizer, refresh policy, storage, and surfacing in the directory.

### Modified Capabilities
<!-- None. The hook pipeline extension that captures background_tasks/session_crons into session state is additive plumbing, not a spec-level behavior change to an existing capability. -->

## Impact

- **Backend**: new `mcp/` module (daemon + tool dispatch + descriptor assembly), new `cluihud mcp` CLI subcommand (`hooks/cli.rs` or a new `mcp/shim.rs`), session store gains summary + background-tasks/crons fields, `hooks/events.rs` Stop schema extended (`#[serde(default)]`), new Tauri commands + settings.
- **Frontend**: settings UI for MCP server + AI summaries; directory data is consumed by agents (no new user-facing panel in this change — panels arrive in `cross-session-messaging`).
- **File system**: registration snippet in agent MCP config (e.g. `~/.claude.json` / `mcpServers`) pointing at `cluihud mcp`. No new project files.
- **Existing flows**: the adapter spawn path already injects `CLUIHUD_SESSION_ID`; this change adds reading it back in the shim. The Stop hook handler gains additive fields.

## Build contract

### Qué construyo
- MCP daemon inside the cluihud process + `cluihud mcp` stdio shim bridging to the daemon socket.
- Identity correlation map (`CLAUDE_CODE_SESSION_ID` ↔ `CLUIHUD_SESSION_ID`).
- MCP tools: `whoami`, `list_sessions`, `get_session`.
- Session descriptor assembly from existing session store + mode map + git metadata + recent activity + (additive) background tasks/crons.
- Opt-in AI summary runner (haiku) + storage + directory surfacing.
- Settings (MCP enable, summaries enable global/per-project) + agent MCP-config registration.

### Cómo verifico
- `cd src-tauri && cargo clippy -- -D warnings && cargo test && cargo fmt --check`
- `npx tsc --noEmit`
- Manual: register `cluihud mcp` in CC, run two sessions, call `list_sessions` from one and confirm it sees the other with correct workspace/branch/mode; call `whoami` and confirm the returned id matches the routing map; toggle AI summaries and confirm a summary appears.

### Criterio de done
- An agent in session A can enumerate session B (different workspace) with fresh metadata via `list_sessions`.
- `whoami` resolves the caller to the correct cluihud session id for CC and at least one non-CC agent.
- Background tasks/crons from a Stop hook appear in `get_session` when present.
- AI summaries are off by default; when enabled, a non-empty summary appears within one refresh cycle and never on disk when disabled.

### Estimated scope
- files_estimate: 14
- risk_tier: medium
- tags: [feature]
- visibility: public
- spec_target: cluihud-mcp-server, session-directory, session-summary
