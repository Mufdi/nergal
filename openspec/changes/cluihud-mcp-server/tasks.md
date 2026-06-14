# Tasks — cluihud-mcp-server

> Implementation status (2026-06-14). Core session directory (phases 1-5, CC
> registration, frontend toggle, unit tests) **landed + verified green**
> (clippy -D warnings / 484 tests / fmt / tsc / vite build). Two slices are
> **deferred** with rationale (flagged inline): the opt-in AI summarizer
> (phase 6 — external LLM API + secret-storage decision, can't be verified
> headless) and the multi-agent registrars + `claude agents --json` enrichment
> (phase 4.4 / 7 — config formats need live verification). The manual two-session
> runtime walk (8.6) requires a running app.

## 1. Dedicated MCP transport (backend)

- [x] 1.1 `src-tauri/src/mcp/transport.rs`: framing (4-byte LE length + payload) over a **new** socket `/tmp/cluihud-mcp.sock`; hook socket untouched.
- [x] 1.2 Socket mode `0600`, stale-socket removal on bind (mirrors `server.rs:198`).
- [x] 1.3 `src-tauri/src/mcp/mod.rs`: JSON-RPC types, `initialize`/`tools/list`/`tools/call`, tool registry (daemon owns schemas via `tool_definitions`).
- [x] 1.4 `mcp_server_enabled` in `config.rs`, **default off**; daemon binds always, `tools/call` → `mcp_disabled` when off.

## 2. stdio shim (CLI)

- [x] 2.1 `cluihud mcp` subcommand: stdio relay to the dedicated socket.
- [x] 2.2 Degraded mode: local `initialize` + vendored `tools/list` (same `tool_definitions` source → no drift); structured error on `tools/call`.
- [x] 2.3 Fast, non-hanging connect; missing/dead socket degrades immediately.

## 3. Identity + uid boundary (daemon)

- [x] 3.1 uid wall: `0600` socket + `peer_cred().uid()` reject of other uids (the only access boundary). No `/proc` pid-walk.
- [x] 3.2 Cooperative identity: validate the reported `CLUIHUD_SESSION_ID` / CC side map against the live registry; unknown → unidentified.
- [x] 3.3 Lazy per-call re-resolution (connect-before-register race); side map torn down with `forget_session`.
- [x] 3.4 `list_sessions`/`get_session` global-read within the uid; cross-workspace exposure documented in the descriptor + Settings disclosure.

## 4. Descriptor assembly + directory tools

- [x] 4.1 Snapshot-then-release: brief DB lock → owned `Vec<Workspace>` → guard dropped before assembly; branch from the persisted column (no git subprocess on the read path).
- [x] 4.2 Descriptor (name, workspace, branch, agent, mode from `SessionStatus`, last-activity, bg-tasks/crons; `waiting_for`/`files` null/empty, never fabricated).
- [x] 4.3 `whoami` / `list_sessions(include_self?)` (self-exclusion) / `get_session(id)` (not-found error).
- [ ] 4.4 **DEFERRED**: out-of-band `claude agents --json` cache for `waiting_for`/state enrichment (currently null). Lands with the multi-agent registrars; needs live `claude agents --json` shape verification.

## 5. Background tasks / crons (additive)

- [x] 5.1 `HookEvent::Stop` extended with `background_tasks` + `session_crons` (`#[serde(default)]`, pass-through JSON). (SubagentStop is not modeled in cluihud today — additive when it is.)
- [x] 5.2 Captured into session state (`set_session_background`), surfaced in `get_session`; legacy-payload deserialize unit-tested.

## 6. Opt-in AI summaries — DEFERRED (net-new LLM machinery)

> Deferred as a deliberate second increment: it adds an external Anthropic API
> call (unverifiable headless) and carries a **secret-storage decision** (keyring
> vs plaintext config for the dedicated key) that warrants explicit sign-off on a
> security-tier change. The core directory ships without it.

- [x] 6.2 Migration `021_session_summaries.sql` created + registered in `db.rs` (table ready; numbering corrected 014→021 after ClickUp migrations 015-020).
- [ ] 6.1 Settings `ai_summaries_enabled` (global + per-project) in `config.rs`.
- [ ] 6.3 `mcp/summary.rs` inference path (cheap model, dedicated key, transcript read, token accounting).
- [ ] 6.4 Detached/debounced runner on `Stop`; surface `summary` in descriptor; nothing read/stored when disabled.
- [ ] 6.5 Single entrypoint reusable by M4's MOC summary.

## 7. Registration + frontend

- [x] 7.1 (CC) Idempotent registration of `cluihud mcp` into `~/.claude.json` `mcpServers`, pinned `/usr/bin/cluihud`; best-effort disable-time deregistration; startup sync; pure helper unit-tested.
- [ ] 7.1b **DEFERRED**: Codex/Pi/OpenCode registrars — each MCP config schema needs verification against a live install before writing (avoid corrupting agent configs by guessing).
- [x] 7.2 (MCP toggle) Settings → MCP section: enable toggle (default off) + global-read disclosure. AI-summaries UI lands with phase 6.

## 8. Tests

- [x] 8.1 Transport framing: fragmented/partial, oversized, zero-length, short write, clean-EOF, partial-EOF (7 tests).
- [x] 8.2 JSON-RPC dispatch: initialize / tools/list / tools/call / unknown method / unknown tool / disabled path / invalid params (8 tests).
- [x] 8.3 Identity table: registered-id resolve / unknown → unidentified / CC side-map resolve / forget teardown; shim degraded-mode (3 tests).
- [x] 8.4 Descriptor assembly: empty directory + whoami-unidentified; registration add/idempotent/preserve/remove/missing/non-object (6 tests); Stop legacy + with-fields deserialize (2 tests).
- [x] 8.5 `cargo clippy -- -D warnings && cargo test && cargo fmt --check` + `npx tsc --noEmit` + `vite build` — all green.
- [ ] 8.6 **PENDING (runtime, needs running app)**: two-session walk — cross-workspace `list_sessions`; `whoami` (CC + one non-CC); other-uid connection rejected; bg-tasks surfacing.
