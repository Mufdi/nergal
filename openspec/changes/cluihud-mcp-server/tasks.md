# Tasks — cluihud-mcp-server

> Redaction only. Do NOT start implementation until the three-change set is approved. Verify cited symbols against current code before editing.

## 1. Dedicated MCP transport (backend)

- [ ] 1.1 Create `src-tauri/src/mcp/transport.rs`: `trait McpTransport { accept; recv_framed; send_framed }` + `UnixSocketTransport` over a **new** socket `/tmp/cluihud-mcp.sock`, length-framed JSON-RPC (4-byte LE length + payload). Do NOT modify the hook socket (`hooks/server.rs:202` is fire-and-forget, read-only — confirmed `server.rs:205-261`).
- [ ] 1.2 Create the socket with mode `0600` in a per-user dir; remove stale socket on startup (mirror `server.rs:198`).
- [ ] 1.3 Create `src-tauri/src/mcp/mod.rs`: JSON-RPC request/response types, `initialize`/`tools/list`/`tools/call` handling, a `ToolRegistry` (daemon owns tool schemas).
- [ ] 1.4 Setting `mcp_server_enabled` in `config.rs`, **default off**; gate socket acceptance, structured "mcp_disabled" error when off.

## 2. stdio shim (CLI)

- [ ] 2.1 `cluihud mcp` subcommand: stdio MCP server, relays to the dedicated socket.
- [ ] 2.2 Degraded mode: complete `initialize` locally; answer `tools/list` from a vendored static list generated from the daemon registry at build time; structured error on `tools/call`.
- [ ] 2.3 Validate socket path early (mirror CC v2.1.162 deep-`$TMPDIR` fix); no hang when daemon down.

## 3. Identity + uid boundary (daemon)

- [ ] 3.1 Enforce the uid boundary: socket mode `0600` in a per-user dir + reject connections whose `peer_cred().uid()` differs from the app's. This is the only access boundary. Do NOT implement a `/proc` PPID-walk (TOCTOU-unsound; no uplift vs the env hint against a same-uid process per design Decision 2).
- [ ] 3.2 Cooperative identity: validate the reported `CLUIHUD_SESSION_ID` / `CLAUDE_CODE_SESSION_ID` against the live session registry; unknown id → unidentified. Maintain the `claude_code_session_id -> cluihud_session_id` side map.
- [ ] 3.3 Lazy re-validation per tool call (connect-before-register race). Teardown binding + side map on disconnect; align with `forget_session` (`state.rs:81`).
- [ ] 3.4 `list_sessions`/`get_session` are global-read within the uid (no identity gate) — design Decision 2b; document the cross-workspace exposure in the descriptor schema.

## 4. Descriptor assembly + directory tools

- [ ] 4.1 Snapshot-then-release: the assembly fn takes an owned snapshot under the `AgentRuntimeState` lock (`state.rs:25`, `std::sync::Mutex`), drops the guard, THEN does git (`worktree.rs`, synchronous `std::process::Command`) / fs / subprocess work. Enforcement: `clippy::await_holding_lock` covers the async sub-case; the synchronous-blocking-under-lock case (no `.await`) is prevented structurally + verified in code review (the lint can't see it).
- [ ] 4.2 Build descriptor (name, workspace, branch, agent, mode/`waitingFor` from `SessionStatus` `models.rs:8`, last-activity, files-touched).
- [ ] 4.3 `whoami` / `list_sessions(filter?)` (self-exclusion) / `get_session(id)` (not-found error).
- [ ] 4.4 Out-of-band cache for `claude agents --json` enrichment (timer/hook-driven); never spawned on a read.

## 5. Background tasks / crons (additive)

- [ ] 5.1 Extend `HookEvent::Stop` / `SubagentStop` (`hooks/events.rs:27`) with `background_tasks: Vec<BackgroundTask>` + `session_crons: Vec<SessionCron>` (`#[serde(default)]`), structs from CC v2.1.150 payload.
- [ ] 5.2 Capture into session state; surface in `get_session`. Unit test: legacy payload (without fields) still deserializes.

## 6. Opt-in AI summaries (net-new LLM machinery)

- [ ] 6.1 Settings `ai_summaries_enabled` (global, default off) + per-project override in `config.rs`.
- [ ] 6.2 Migration `src-tauri/migrations/014_session_summaries.sql` (latest on disk is `013_env_shells`): `session_summaries(session_id TEXT PRIMARY KEY, summary TEXT NOT NULL, model TEXT, token_cost INTEGER, updated_at INTEGER NOT NULL)`; register `include_str!` in `db.rs` migrations array (`db.rs:132`).
- [ ] 6.3 New inference path `mcp/summary.rs`: cheap model (haiku) invocation via a **dedicated configured API key** (`config.rs` setting) — NOT session agent auth (fragile, conflates billing; round-2 finding). No usable key → summaries stay off with a settings hint. Transcript read + token accounting. No reuse claim — `post_session.rs` has no LLM path.
- [ ] 6.4 Detached runner; refresh on `Stop` (debounced) + on demand; frequency cap; timestamped. Surface `summary` in descriptor; null when absent; never block a read. No row + no transcript read when disabled.
- [ ] 6.5 Single entrypoint reusable later by M4's MOC summary.

## 7. Registration + frontend

- [ ] 7.1 Register `cluihud mcp` in spawned agents' MCP config (CC `mcpServers` in `~/.claude.json`; Codex/Pi/OpenCode equivalents) **idempotently**, pinning the installed absolute path `/usr/bin/cluihud` (NOT `$PATH` resolution — avoids the `~/.cargo/bin` shadow per CLAUDE.md). **Best-effort deregistration at disable time** (app running); NO uninstall-time maintainer-script cleanup (multi-user `$HOME` is fragile). Orphaned entry → structured error at agent startup, not hard failure.
- [ ] 7.2 Settings UI: toggle MCP server (default off) + AI summaries (global + per-project).

## 8. Tests (not manual-only)

- [ ] 8.1 Unit: transport framing — fragmented frame (partial read), oversized length field, zero-length payload, short write. (Highest-risk new code.)
- [ ] 8.2 Unit: JSON-RPC dispatch (`initialize`/`tools/list`/`tools/call`, unknown tool, disabled-daemon error).
- [ ] 8.3 Unit: identity-validation table — valid env id matching a live session / unknown id → unidentified / connect-before-register → lazy resolve / disconnect teardown. Plus the pure uid-comparison decision fn (accept own uid / reject other uid) — the single enforced boundary must not be manual-only.
- [ ] 8.4 Unit: descriptor assembly from fixture state. Snapshot-release: `clippy::await_holding_lock` for the async case + code-review gate for the synchronous-blocking-under-lock case (lint-invisible).
- [ ] 8.5 `cd src-tauri && cargo clippy -- -D warnings && cargo test && cargo fmt --check` + `npx tsc --noEmit`.
- [ ] 8.6 Manual two-session walk: cross-workspace `list_sessions`; `whoami` (CC + one non-CC); other-uid connection rejected; bg-tasks surfacing; AI-summary on/off (no row + no transcript read when off).
