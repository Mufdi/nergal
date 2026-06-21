# Implementation Plan: cluihud-mcp-server

> Grounded in current backend (`src-tauri/src/`), symbols verified 2026-06-08. Behaviour (not just symbol existence) verified for the load-bearing claims below.

## Verified codebase facts (do not re-assume)

- **Hook socket is fire-and-forget.** `hooks/server.rs:202` binds `tokio::net::UnixListener`; the accept loop (`server.rs:205-261`) reads newline-delimited lines via `BufReader::lines()` and dispatches `process_event`/`process_control_message`. There is **no `write_all`/response path**. A `kind` discriminator exists (`control` vs `hook_event`) but it never writes back. → MCP needs a **new bidirectional transport**.
- **No LLM-invocation path exists.** `obsidian/post_session.rs` = marker/lock/detached-drain; `moc.rs` summaries are string concat. No model selection, inference auth, token accounting, or transcript-read-for-prompt anywhere. → `session-summary` is **net-new**.
- **Per-session child pid is tracked.** `pty.rs:43` `child_pid: Option<u32>` (captured `pty.rs:146`); `/proc` reads already done by `process_cwd` (`pty.rs:49`). → `child_pid` remains available for cwd/other features; it is **NOT** used for identity (Decision 2 rejects the pid-walk as TOCTOU-unsound with zero uplift). Identity is the cooperative env hint; the uid is the boundary.
- **Migrations pattern.** `src-tauri/migrations/NNN_*.sql` registered via `include_str!` in `db.rs:132`, `schema_version` table, `db.migrate()` at `db.rs:111`. Latest is `013_env_shells.sql`. → new tables = `014+`.
- **State lock.** `AgentRuntimeState` (`agents/state.rs:25`) is `std::sync::Mutex`; `register_session`/`resolve`/`forget_session` (`state.rs:75/88/81`).
- **Stop event.** `HookEvent::Stop { session_id, stop_reason?, transcript_path? }` (`hooks/events.rs:27`), all `#[serde(default)]`.

## Execution order

1. Dedicated transport + JSON-RPC dispatch (`mcp/transport.rs`, `mcp/mod.rs`).
2. stdio shim (`cluihud mcp`) + degraded mode.
3. uid boundary + cooperative env-hint identity in the daemon (NO pid-walk).
4. Descriptor assembly (snapshot-release) + directory tools.
5. Background tasks/crons additive (`hooks/events.rs`).
6. Net-new AI summarizer + migration.
7. Idempotent registration + frontend settings.
8. Unit tests.

## 1. Dedicated transport — `src-tauri/src/mcp/transport.rs` + `mcp/mod.rs`

- New socket `/tmp/cluihud-mcp.sock`, mode `0600`, per-user dir; remove stale on startup (mirror `server.rs:198`).
- Length-framed JSON (4-byte LE length + payload) — avoids newline-in-JSON ambiguity that `lines()` can't handle.
- `trait McpTransport { accept; recv_framed; send_framed }`, `UnixSocketTransport` impl; future `NamedPipeTransport` for Windows behind the trait.
- `mcp/mod.rs`: JSON-RPC types, `initialize`/`tools/list`/`tools/call`, `ToolRegistry` (daemon owns schemas). Setting `mcp_server_enabled` (default off) gates acceptance.

## 2. stdio shim — `src-tauri/src/mcp/shim.rs` (`cluihud mcp`)

- Stdio MCP server; relays framed requests to the dedicated socket.
- Degraded mode (daemon down): `initialize` answered locally; `tools/list` from a vendored static list generated from the registry at build time (single source → no drift); `tools/call` → structured error. Validate socket path early.

## 3. Identity + uid boundary — daemon side

- **uid boundary (the only access control)**: socket mode `0600` + per-user dir + reject connections whose `peer_cred().uid()` differs from the app uid. Do NOT walk `/proc` PPIDs for a session-resolution boundary — it is TOCTOU-unsound (peer pid fixed at connect, `/proc` read later; pid recycling) and a same-uid process can read `/proc/<agent-pid>/environ` to lift `CLUIHUD_SESSION_ID` anyway, so it adds racy code for zero confidentiality uplift (round-2 finding). (`child_pid` `pty.rs:43` stays useful for other features, just not for authz.)
- **Cooperative identity**: validate the reported `CLUIHUD_SESSION_ID` (from PTY env `pty.rs:142`) / `CLAUDE_CODE_SESSION_ID` against the live session registry. Unknown id → unidentified. Re-validate lazily per tool call (connect-before-register race).
- `list_sessions`/`get_session` are global-read within the uid (no identity gate) — design Decision 2b.
- Teardown on disconnect; extend `AgentRuntimeState` with a `claude_code_session_id -> cluihud_session_id` side map torn down alongside `forget_session`.

## 4. Descriptor assembly — snapshot-then-release

- Lock `AgentRuntimeState` only to copy cheap fields (id, agent, mode); **release**; then read git (`worktree.rs`) / activity / files outside the lock. Never hold the mutex across blocking I/O.
- `mode`/`waitingFor` from `SessionStatus` (`models.rs:8`) + status emitted via `AgentStatusEmit` (`hooks/server.rs:992`).
- `claude agents --json` enrichment cached out-of-band (timer/hook), read from cache only.
- Tools `whoami`/`list_sessions`/`get_session`.

## 5. Background tasks/crons — `hooks/events.rs`

- Add `background_tasks: Vec<BackgroundTask>` + `session_crons: Vec<SessionCron>` (`#[serde(default)]`) to `Stop`/`SubagentStop`; structs from CC v2.1.150 shape. Capture in `hooks/server.rs`; surface in `get_session`. Unit-test legacy payload.

## 6. Net-new AI summarizer — `mcp/summary.rs` + migration

- Migration `014_session_summaries.sql`: `session_summaries(session_id TEXT PRIMARY KEY, summary TEXT NOT NULL, model TEXT, token_cost INTEGER, updated_at INTEGER NOT NULL)`; register `include_str!` in `db.rs:132` array + bump count.
- New inference path: cheap model invocation via a **dedicated configured API key** (`config.rs`) — not session agent auth (fragile/billing-conflating; round-2). No key → off + settings hint. Transcript read, token accounting. Detached runner; debounce on `Stop`; freq cap; timestamp. Null/skip when disabled (no row, no transcript read). Single entrypoint reusable by M4.

## 7. Registration + frontend

- Idempotent registration into agent MCP configs (CC `mcpServers` in `~/.claude.json`; Codex/Pi/OpenCode equivalents); pin installed absolute path `/usr/bin/cluihud` (NOT `$PATH` — avoids `~/.cargo/bin` shadow per CLAUDE.md); best-effort deregistration at disable time only (no uninstall maintainer-script edit of multi-user `$HOME`); orphaned entry → structured error.
- Settings UI: MCP toggle (default off), AI summaries (global + per-project) + summary API key.

## 8. Tests

- Unit: transport framing (fragmented/partial read, oversized length, zero-length, short write) — highest-risk new code.
- Unit: dispatch (`initialize`/`tools/list`/`tools/call`/unknown/disabled); identity-validation table (valid id matching live session / unknown id → unidentified / lazy resolve / disconnect teardown); descriptor assembly from fixtures.
- Snapshot-release enforcement: `clippy::await_holding_lock` (in `cargo clippy -- -D warnings`) for the async case; the synchronous-blocking-under-lock case (git via `std::process::Command`, no `.await`) is lint-invisible and prevented structurally (drop guard before I/O) + code review.

## Per-phase risk

- **Phase 1**: framing bugs (partial reads/short writes). Mitigate: length-prefix + read-exact loop; explicit fragmented/oversized/zero-length unit tests.
- **Phase 3**: cooperative identity is forgeable by a same-uid process — accepted (single-user threat model; that process can read the env anyway). The uid wall is the real boundary.
- **Phase 4**: accidental blocking under the mutex. Mitigate: snapshot-release + `clippy::await_holding_lock`.
- **Phase 6**: token cost / transcript privacy. Mitigate: off by default, per-project override, dedicated key, cheap model, no row/read when disabled.

## Verification

`cd src-tauri && cargo clippy -- -D warnings && cargo test && cargo fmt --check` · `npx tsc --noEmit` · unit suites above · two-session manual walk incl. spoofed-id rejection (see proposal Build contract).
