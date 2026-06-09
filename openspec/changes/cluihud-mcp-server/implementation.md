# Implementation Plan: cluihud-mcp-server

> Grounded in the current backend (`src-tauri/src/`). Symbols cited are real as of 2026-06-08; re-verify before editing.

## Execution order

1. Daemon skeleton + tool registry (`mcp/`) — no tools wired yet, just the socket-reachable dispatch.
2. stdio shim CLI subcommand (`cluihud mcp`) bridging to the daemon socket.
3. Identity correlation (extend `AgentRuntimeState`).
4. Session descriptor assembly + `whoami`/`list_sessions`/`get_session`.
5. Background tasks/crons additive capture (`hooks/events.rs`).
6. Opt-in AI summary runner (reuse `obsidian/post_session.rs` summarizer path).
7. Agent MCP-config registration + frontend settings.

## 1. Daemon skeleton — new `src-tauri/src/mcp/mod.rs`

The "daemon" is the running Tauri app. Reuse the existing IPC: `hooks/server.rs:202` already binds a `UnixListener` on the cluihud socket. Add an MCP message kind to that server (or a sibling listener path) rather than a second socket.

- New `mcp/mod.rs`: `McpRequest`/`McpResponse` JSON-RPC enums, a `ToolRegistry` mapping tool name → handler, and a `dispatch(req, &AgentRuntimeState, &AppHandle) -> McpResponse`.
- Gate on a new config flag `mcp_server_enabled` (default `true`) in `config.rs`; when off, `dispatch` returns a structured `{ error: "mcp_disabled" }`.
- **Transport trait** (multiplatform constraint): define `trait McpTransport { fn recv(); fn send(); }` with a `UnixSocketTransport` impl; call sites use the trait so a future `NamedPipeTransport` drops in.

## 2. stdio shim — new subcommand in `hooks/cli.rs` (or `mcp/shim.rs`)

`cluihud` already is the hook CLI binary. Add `cluihud mcp`:
- Speaks MCP JSON-RPC on stdin/stdout (the agent is the MCP client).
- On boot, read `CLUIHUD_SESSION_ID` + `CLAUDE_CODE_SESSION_ID` from env, connect to the socket, send a `Handshake { cluihud_session_id, claude_code_session_id }`.
- Forward each tool call to the daemon, await, return over stdio.
- **Daemon unreachable**: complete the MCP `initialize` handshake locally, then return structured errors on `tools/call` (validate socket path early — mirror CC v2.1.162's deep-`$TMPDIR` socket fix).

## 3. Identity correlation — extend `agents/state.rs`

`AgentRuntimeState` (`agents/state.rs:25`) already maps `cluihud_session_id -> AgentId` via `register_session`/`resolve`/`forget_session`. Extend:
- Add a `claude_code_session_id -> cluihud_session_id` side map populated from the shim handshake.
- `resolve_caller(handshake) -> Option<CluihudSessionId>`: `CLUIHUD_SESSION_ID` authoritative; cross-check `CLAUDE_CODE_SESSION_ID` for CC.
- Unidentified caller → `None`; daemon omits from directory, `whoami` returns null.

## 4. Descriptor assembly + directory tools

Source data already exists; assemble, don't recompute:
- name/agent/workspace → session store + `AgentRuntimeState`.
- branch/dirty → `worktree.rs` git helpers.
- mode/`waitingFor` → `SessionStatus` (`models.rs:8`) + the status emitted via `AgentStatusEmit` (`hooks/server.rs:992`).
- last-activity / files-touched → activity/feed store (`feeds.rs`, `tasks/`).
- Tools: `whoami` (caller descriptor), `list_sessions(filter?)` (all live, self-exclusion), `get_session(id)` (full + not-found error).
- Optional CC enrichment: shell out to `claude agents --json` for `waitingFor`/`state` (v2.1.162/168); daemon state stays primary.

## 5. Background tasks/crons — `hooks/events.rs`

`HookEvent::Stop` (`hooks/events.rs:27`) currently carries `session_id`, `stop_reason`, `transcript_path`. Add:
```rust
#[serde(default)] background_tasks: Vec<BackgroundTask>,
#[serde(default)] session_crons: Vec<SessionCron>,
```
Define both structs from the CC v2.1.150 payload shape. Capture into session state in `hooks/server.rs`; surface in `get_session`. Verify legacy payloads (without the fields) still deserialize (existing tests in `hooks/`).

## 6. Opt-in AI summaries — reuse `obsidian/post_session.rs`

`obsidian/post_session.rs` already runs a detached post-session step. Extract/share its summarizer entrypoint (haiku, reads transcript → short summary) so both M4's MOC summary and the directory summary call one path.
- Settings `ai_summaries_enabled` (global, default off) + per-project override in `config.rs`.
- Detached runner, store summary + timestamp in session state; refresh on `Stop` (debounced) + on demand; frequency cap.
- `summary` null when absent; never block a `list_sessions`/`get_session` read.

## 7. Registration + frontend

- When spawning CC (`agents/claude_code/adapter.rs` — env wiring at `:228`), add `cluihud mcp` to the session's MCP config. Codex/Pi/OpenCode: equivalent MCP-server config in their adapters.
- Frontend: settings toggles for MCP server + AI summaries (global/per-project).

## Per-phase risk

- **Phase 1-2 (socket reuse)**: adding an MCP message kind to the hook socket risks interleaving with hook traffic. Mitigate: distinct message tag + its own handler branch; consider a dedicated socket path if framing gets messy.
- **Phase 3 (identity)**: race where the shim connects before the adapter registered the session. Mitigate: daemon retries resolution briefly, else returns unidentified (no guess).
- **Phase 6 (summaries)**: token cost + transcript privacy. Mitigate: off by default, per-project override, cheap model only, nothing read/sent when disabled.

## Verification

`cd src-tauri && cargo clippy -- -D warnings && cargo test && cargo fmt --check` · `npx tsc --noEmit` · two-session manual walk (see proposal Build contract).
