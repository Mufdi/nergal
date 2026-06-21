## Why

Today cluihud only *observes* agent state (via hooks + transcript watchers). It has no way to *expose* that state back to the agents it wraps. An agent cannot ask "what other sessions are running, and what are they doing?" without the user pasting the answer manually.

This is the foundation for cross-session collaboration (changes `cross-session-messaging` and `agent-spawned-worktrees` build on it) and has standalone value: an agent can discover its sibling sessions, read short summaries, and orient itself in a multi-session workspace. It is the first time Nergal turns its observed state into an agent-facing API.

This change replaces the foundation half of the archived `context-bridge` change (file-bus approach, never implemented). It absorbs the backlog items "MCP server cluihud con correlación nativa via `CLAUDE_CODE_SESSION_ID`", the directory half of "AI session summary" (M4), and "Surface background tasks y crons en Activities".

## What Changes

- New **single MCP daemon** owned by the cluihud app process, holding the global view of all live sessions across all workspaces.
- New **dedicated bidirectional transport** for MCP: a hardened Unix socket (`/tmp/cluihud-mcp.sock`, mode `0600`, length-framed JSON-RPC). The existing hook socket (`hooks/server.rs:202`) is **fire-and-forget** (read-only, no response path) and is **not** reused for MCP — verified against `server.rs:205-261`.
- New **stdio MCP shim** subcommand (`cluihud mcp`) that each agent spawns; it relays MCP JSON-RPC to the daemon over the dedicated socket. Agent-agnostic (CC, Codex, Pi, OpenCode).
- **uid-restricted socket + cooperative identity**: the socket (mode `0600`, per-user dir, `peer_cred().uid()` check) is the only access boundary. Identity is the `CLUIHUD_SESSION_ID` / `CLAUDE_CODE_SESSION_ID` the shim reports, validated against the live session registry — cooperative within the uid, not adversarially authenticated (a same-uid process can read the agent's env directly, so a `/proc` pid-walk would be TOCTOU-racy theater). Validation is lazy (per tool call) to survive the connect-before-register race. The directory is **intentionally global-read within the uid** (stated, not implied).
- **Session directory** via MCP tools (`whoami`, `list_sessions`, `get_session`) returning cheap, always-fresh metadata assembled with **snapshot-then-release** lock discipline (no git/subprocess I/O under the `AgentRuntimeState` mutex). `claude agents --json` enrichment is cached out-of-band, never on the hot path.
- **Background tasks/crons** captured additively from Stop hooks (CC v2.1.150) into `get_session`.
- **Opt-in AI session summary** built as **net-new LLM machinery** (no inference path exists in the backend today — verified): model invocation, credential resolution, token accounting, transcript read, detached runner, and SQLite persistence via a migration. Off by default; global + per-project. M4's MOC summary can later share this new entrypoint.
- **Idempotent registration** of `cluihud mcp` into agent MCP configs (pinned `/usr/bin/cluihud`), deregistered best-effort on disable; an orphaned entry after uninstall degrades to a structured error.
- New settings; `mcp_server_enabled` defaults **off** until the trust baseline is validated.

## Capabilities

### New Capabilities
- `cluihud-mcp-server`: The MCP daemon, the dedicated bidirectional transport + stdio shim, uid-restricted socket + cooperative env-hint identity (validated against the live registry, lazy re-validation, connection teardown), the explicit global-read-within-uid directory posture, idempotent agent registration (pinned install path, best-effort disable-time deregistration), protocol-surface ownership (daemon owns tool schemas + degraded-mode capabilities; shim degrades to a build-time vendored list), and the `whoami` tool.
- `session-directory`: The `list_sessions` / `get_session` tools, the descriptor schema (incl. background tasks/crons), snapshot-then-release freshness without reactor stalls, and the data-classification note for cross-workspace fields.
- `session-summary`: Opt-in net-new AI summary machinery — model invocation, SQLite-backed storage (migration), refresh policy, and surfacing in the directory; nothing read/invoked/stored when disabled.

### Modified Capabilities
<!-- None. The Stop-hook extension that captures background_tasks/session_crons is additive (`#[serde(default)]`), not a spec-level behavior change to an existing capability. -->

## Impact

- **Backend**: new `mcp/` module (daemon + dedicated transport + JSON-RPC dispatch + tool registry + descriptor assembly + identity resolver), new `cluihud mcp` CLI subcommand (`mcp/shim.rs`), net-new LLM summarizer path (`mcp/summary.rs` or `summary/`), new SQLite migration(s) under `src-tauri/migrations/` (session summaries; bg-tasks/crons if persisted), `hooks/events.rs` Stop schema extended (`#[serde(default)]`), new Tauri commands + settings.
- **Frontend**: settings UI for MCP server (default off) + AI summaries (global + per-project).
- **File system**: dedicated MCP socket `/tmp/cluihud-mcp.sock` (0600); idempotent registration snippet (pinned `/usr/bin/cluihud`) in agent MCP config (`~/.claude.json` `mcpServers`, Codex/Pi/OpenCode equivalents) with best-effort disable-time deregistration.
- **Existing flows**: adapters already inject `CLUIHUD_SESSION_ID`; the daemon now resolves identity from pid + hint. The Stop hook handler gains additive fields. The hook socket is untouched.

## Build contract

### Qué construyo
- MCP daemon + dedicated length-framed JSON-RPC transport over a uid-restricted Unix socket (0600 + `peer_cred().uid()` check) + `cluihud mcp` stdio shim.
- Cooperative env-hint identity validated against the live registry; lazy re-validation; teardown on disconnect. No `/proc` pid-walk (TOCTOU theater).
- MCP tools: `whoami`, `list_sessions`, `get_session` (global-read within the uid); descriptor assembly with snapshot-then-release lock discipline (enforced by `clippy::await_holding_lock`) + out-of-band `claude agents --json` cache.
- Stop-hook additive `background_tasks` + `session_crons`.
- Net-new opt-in AI summarizer: model invocation via a dedicated configured key (no agent-auth reuse) + token accounting + transcript read + detached runner + SQLite migration (`session_summaries`).
- Idempotent agent MCP-config registration (pinned `/usr/bin/cluihud`, best-effort disable-time deregistration); protocol-surface ownership (daemon registry, shim build-time vendored `tools/list` + `initialize` capabilities for degraded mode).
- Settings (MCP enable default off, AI summaries global/per-project + key).
- Unit tests: transport framing (fragmented/oversized/zero-length), JSON-RPC dispatch, identity-validation table, disabled-daemon path, descriptor assembly, legacy Stop deserialization.

### Cómo verifico
- `cd src-tauri && cargo clippy -- -D warnings && cargo test && cargo fmt --check`
- `npx tsc --noEmit`
- Unit: transport framing + dispatch + identity-validation table (valid id matching live session / unknown id → unidentified / lazy resolve / disconnect teardown) + disabled-path + descriptor assembly.
- Manual: two sessions, `list_sessions` cross-workspace visibility; `whoami` correctness (CC + one non-CC); a different-uid connection is rejected; bg-tasks surfacing; AI summary on/off (no SQLite row + no transcript read when off).

### Criterio de done
- An agent in session A enumerates session B (different workspace) with fresh metadata; no reactor stall under repeated `list_sessions` (`clippy::await_holding_lock` clean).
- `whoami` resolves CC + one non-CC correctly; a different-uid process cannot connect; identity within the uid is cooperative and the directory is global-read by design.
- Background tasks/crons from a Stop hook appear in `get_session`; legacy Stop payloads still deserialize.
- AI summaries off by default; when enabled a non-empty summary appears within one refresh cycle, persisted in SQLite; when disabled there is no row and no transcript read.
- Registration is idempotent; disabling the server deregisters it from agent configs.

### Estimated scope
- files_estimate: 20
- risk_tier: critical
- tags: [feature, security]
- visibility: public
- spec_target: cluihud-mcp-server, session-directory, session-summary
