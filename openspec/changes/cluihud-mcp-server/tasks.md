# Tasks — cluihud-mcp-server

> Redaction only. Do NOT start implementation until the three-change set is approved.

## 1. MCP daemon (backend)

- [ ] 1.1 Create `src-tauri/src/mcp/mod.rs` with the daemon: a tool-dispatch surface reachable over `/tmp/cluihud.sock`, holding a reference to the global session store.
- [ ] 1.2 Define the MCP tool registry (`whoami`, `list_sessions`, `get_session`) and JSON-RPC request/response types.
- [ ] 1.3 Add a setting `mcp_server_enabled` (default on) gating daemon connection acceptance; return a structured "MCP disabled" error when off.
- [ ] 1.4 Define a transport trait so the unix-socket path is swappable (future Windows named pipes), per the multiplatform constraint.

## 2. stdio shim (CLI)

- [ ] 2.1 Add `cluihud mcp` subcommand: a stdio MCP server speaking JSON-RPC on stdin/stdout, forwarding to the daemon socket.
- [ ] 2.2 On startup, read `CLUIHUD_SESSION_ID` + `CLAUDE_CODE_SESSION_ID` and send a handshake announcing both to the daemon.
- [ ] 2.3 Handle daemon-unreachable: complete the MCP handshake, return structured errors on tool calls (validate socket path early, mirror CC v2.1.162 deep-`$TMPDIR` fix).

## 3. Identity correlation (daemon)

- [ ] 3.1 Maintain a map MCP-connection ↔ cluihud session, `CLUIHUD_SESSION_ID` authoritative, `CLAUDE_CODE_SESSION_ID` confirming for CC.
- [ ] 3.2 Mark callers with no resolvable id as unidentified: omit from directory, null `whoami`, read-only at most.

## 4. Session descriptor assembly

- [ ] 4.1 Build the descriptor from existing session store + mode map + git metadata + recent activity (name, workspace, branch, agent, mode, `waitingFor`, last-activity, files-touched).
- [ ] 4.2 Implement `list_sessions` (with optional filter, self-exclusion) and `get_session` (by id, with not-found error).
- [ ] 4.3 Implement `whoami` returning the caller's descriptor or null.
- [ ] 4.4 (Optional, CC-only) cross-check `waitingFor`/`state` against `claude agents --json`; keep daemon state primary.

## 5. Background tasks / crons (additive)

- [ ] 5.1 Extend `HookEvent::Stop` / `SubagentStop` in `src-tauri/src/hooks/events.rs` with `background_tasks` + `session_crons` (`Option<Vec<...>>`, `#[serde(default)]`).
- [ ] 5.2 Capture into session state; surface in `get_session`. Verify legacy payloads (without the fields) still deserialize.

## 6. Opt-in AI summaries

- [ ] 6.1 Add settings `ai_summaries_enabled` (global, default off) + per-project override.
- [ ] 6.2 Build/extract a shared summarizer entrypoint (haiku, reads transcript → short rolling summary), reused by M4's MOC summary.
- [ ] 6.3 Detached runner + storage in the session store; refresh on `Stop` (debounced) + on demand; frequency cap; timestamped.
- [ ] 6.4 Surface `summary` in the descriptor; null when absent; never block a directory read.

## 7. Agent registration + frontend settings

- [ ] 7.1 Register `cluihud mcp` in spawned agents' MCP config (CC `mcpServers`; Codex/Pi/OpenCode equivalents) when the server is enabled.
- [ ] 7.2 Settings UI: toggle MCP server, toggle AI summaries (global + per-project).

## 8. Verification

- [ ] 8.1 `cd src-tauri && cargo clippy -- -D warnings && cargo test && cargo fmt --check`
- [ ] 8.2 `npx tsc --noEmit`
- [ ] 8.3 Manual two-session walk: `list_sessions` cross-workspace visibility, `whoami` correctness (CC + one non-CC), bg-tasks surfacing, AI-summary on/off behavior.
