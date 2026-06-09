## Context

cluihud is a long-lived Tauri process that already holds, in memory and SQLite, the state of every session it spawns: PTY handles, `CLUIHUD_SESSION_ID` per session, mode map (idle/active/tool), git metadata, activity stream, and transcript paths. The agent adapters (`agents/claude_code/adapter.rs:228`, `pi:168`, `opencode:209`, `codex:175`) inject `CLUIHUD_SESSION_ID` at spawn for every agent.

**Verified codebase facts that shape this design (checked 2026-06-08):**
- The existing hook Unix socket (`hooks/server.rs:202`) is **fire-and-forget**: the accept loop (`server.rs:205-261`) reads newline-delimited lines via `BufReader::lines()` and dispatches `process_event`; it has **no response/write path**. It cannot carry MCP request/response. → MCP needs a **new bidirectional transport**, not a message tag on this socket.
- There is **no LLM-invocation path anywhere in the backend**. `obsidian/post_session.rs` is marker/lock/detached-drain plumbing; `moc.rs` builds summaries by string concatenation. There is no model selection, auth-for-inference, token accounting, or transcript-read-for-prompt. → `session-summary` is **net-new LLM machinery**, not a reuse of existing code.
- `AgentRuntimeState` (`agents/state.rs:25`) is guarded by a blocking `std::sync::Mutex` and maps `cluihud_session_id -> AgentId` (`register_session`/`resolve`/`forget_session`).

CC injects `CLAUDE_CODE_SESSION_ID` + `CLAUDECODE=1` into stdio MCP server env (v2.1.154, also on `--resume` per v2.1.163). Codex, Pi, OpenCode support MCP server configuration.

## Goals / Non-Goals

**Goals:**
- A single daemon owning the live session directory across all workspaces.
- Agent-agnostic access via a thin stdio shim over a dedicated, authenticated transport.
- Cooperative env-hint identity within a uid wall — honest about being self-asserted, not adversarially authenticated.
- Cheap, always-fresh directory metadata with bounded lock scope (no reactor stalls).
- Opt-in AI summaries built honestly as net-new LLM machinery, off by default.
- Background tasks/crons (CC v2.1.150) folded into the descriptor additively.

**Non-Goals:**
- Cross-session *messaging* — `cross-session-messaging`.
- Agent-initiated *creation* of sessions — `agent-spawned-worktrees`.
- Per-caller authorization on directory reads (Decision 2b: the directory is global-read within the uid; identity is not an access gate here). The uid wall is the only enforced boundary.
- HTTP/SSE MCP transport (the dedicated socket is sufficient for v1; the daemon boundary leaves HTTP as a later option).

## Decisions

### 1. New bidirectional transport, NOT reuse of the hook socket

**Decision**: The MCP daemon exposes a **dedicated Unix socket** (`/tmp/cluihud-mcp.sock`, mode `0600`, in a per-user dir) speaking length-framed JSON-RPC with per-request response correlation. The existing hook socket is left untouched (it is fire-and-forget and cannot answer requests).

**Why**: MCP (`initialize`, `tools/list`, `tools/call`) requires request→response over a persistent bidirectional connection. `server.rs` has no write-back path and frames by newline, which can't carry multi-line JSON-RPC. Bolting responses onto it would be a larger, riskier change than a clean dedicated transport.

**Framing**: length-prefixed JSON (4-byte LE length + payload) to avoid newline-in-JSON ambiguity. The shim and daemon share one transport module.

**Transport trait** (multiplatform): `trait McpTransport { fn accept(); fn recv_framed(); fn send_framed(); }` with a `UnixSocketTransport` impl; a future `NamedPipeTransport` (Windows) drops in without touching dispatch.

### 2. Cooperative identity from the env hint; the uid is the only real boundary

**Decision**: Identity is the `CLUIHUD_SESSION_ID` (and `CLAUDE_CODE_SESSION_ID`) the shim reports, **validated against the daemon's live session registry** (the daemon knows which session ids are real, live sessions — it spawned them). It is treated as a **cooperative** identifier, not an adversarial authentication. The only enforced boundary is the **uid**: the socket is mode `0600` in a per-user directory, and the daemon additionally checks `peer_cred().uid()` and rejects other uids.

**Why the elaborate pid-walk was dropped** (round-2 finding): a `/proc` PPID-walk from the peer pid up to a session `child_pid` is **TOCTOU-unsound** (the peer pid is fixed at `connect(2)`, but `/proc/<pid>/stat` and the parent chain are read later and can change; pid recycling could even walk into a *different* session's `child_pid`) and it defends a threat that does not exist here: against a **same-uid** adversary there is no boundary to defend — that process can already `ptrace` the shim or read `/proc/<agent-pid>/environ` to lift `CLUIHUD_SESSION_ID` directly. So peer-cred-pid resolution buys **no confidentiality** over the env hint against the only adversary in scope, while adding racy code. The honest baseline is: cooperative env identity + a uid wall.

**What identity is for here**: `whoami`'s self-answer, and laying the groundwork for `cross-session-messaging` sender attribution (knowing which session a message claims to be from). It is **not** an access gate in this change — see Decision 2b.

**Race handling**: a shim that connects before its session is registered is `unidentified`; identity is re-validated lazily on each tool call, so it becomes identified as soon as the registry knows the id. Teardown on disconnect drops the connection→session binding and the `claude_code_session_id -> cluihud_session_id` side map (alongside `forget_session`, `state.rs:81`).

**Multiplatform**: the uid check + socket perms are unix; behind the identity abstraction so a Windows port supplies its own per-user restriction.

### 2b. The directory is intentionally global-read within the uid

**Decision**: `list_sessions` / `get_session` do **not** gate on caller identity. Any same-uid caller that reaches the socket reads every live session's descriptor (cross-workspace paths, branches, recently-touched files, and — when enabled — summaries). This is stated plainly rather than implied away.

**Why**: This is a single-user desktop app; the sessions all belong to one person. A same-uid access wall (Decision 2) is the real boundary; per-caller authorization inside it would be theater (the same user owns all sessions and the env that identifies them). The descriptor schema documents this cross-workspace exposure (`session-directory` spec). If a future multi-user or sandboxed scenario appears, read-gating on identity tier is the extension point — out of scope now.

**Lazy re-resolution**: identity is resolved **per tool call**, not once on connect, so a shim that connected before its session registered (race) becomes identified as soon as the daemon learns the mapping — no permanently-null connection.

**Lifecycle**: on socket disconnect (peer close) the daemon drops the connection→session binding. The `claude_code_session_id -> cluihud_session_id` side map is torn down alongside `forget_session` (`state.rs:81`). "Live session" = a session with an active PTY child known to the daemon; that set is the directory's liveness source.

### 3. Cheap directory with bounded lock scope; enrichment off the hot path

**Decision**: `list_sessions`/`get_session` assemble from data cluihud already holds, using a **snapshot-then-release** discipline: acquire the `AgentRuntimeState` mutex only to copy the cheap fields out, release it, then do any git/IO **outside** the lock. No blocking call (git, subprocess) runs while the mutex is held.

`claude agents --json` enrichment (CC `waitingFor`/`state`, v2.1.162/168) is **out-of-band**: refreshed on a timer / on hook events into a cache, never spawned synchronously on a directory read. The daemon's own state stays primary and is what a read returns; the cache is a non-blocking overlay.

**Why**: `AgentRuntimeState` is a blocking `std::sync::Mutex` inside async handlers; holding it across git/subprocess I/O would serialize all sessions and stall the tokio reactor. The spec's "always fresh" and "cheap hot path" only hold if the lock is released before slow work and subprocess spawns never sit on the read path.

### 4. Background tasks/crons — additive

**Decision**: Extend `HookEvent::Stop` / `SubagentStop` (`hooks/events.rs:27`) with `background_tasks: Vec<BackgroundTask>` and `session_crons: Vec<SessionCron>` (`#[serde(default)]`). Capture into session state; surface in `get_session`.

**Why**: CC v2.1.150 already sends these. Additive deserialization keeps older payloads valid (existing `hooks/` tests cover this).

### 5. Opt-in AI summary — net-new LLM machinery, honestly scoped

**Decision**: `session-summary` is **new** inference machinery, not an extraction from `post_session.rs`. It requires: a model-invocation path (cheap model, e.g. haiku), credential resolution via a **dedicated configured API key** (`config.rs` setting), token accounting, transcript read, a detached runner, and **SQLite persistence** (a migration in `src-tauri/migrations/`, not ad-hoc state). Off by default; global setting + per-project override. When disabled, nothing is read, invoked, stored, or sent.

**Credential decision** (round-2 finding): do **not** attempt to reuse the session's own agent auth. CC/Codex/Pi credentials live in their own config/keychains, are scoped to that agent's own usage, and lifting them to issue independent inference calls is fragile, conflates billing, and repeats the "reuse what doesn't cleanly exist" mistake. A separate configured key is the only clean path; if no key is configured, summaries stay off with a clear settings hint.

**Why**: The review verified no summarizer exists to reuse. Pretending otherwise hid real cost. M4's eventual MOC summary can later share this new entrypoint, but this change builds it from zero.

**Storage**: a `session_summaries(session_id PK, summary TEXT, model TEXT, token_cost INT, updated_at INT)` table via migration. Precedence: per-project-enabled gates whether a row is written at all. "Never on disk when disabled" = no row, no transcript read.

**Refresh**: debounce on `Stop` (avoid mid-turn), frequency cap, timestamped; a directory read never blocks on it (returns last row or null).

### 6. MCP protocol surface ownership

**Decision**: The **daemon owns the tool registry and JSON schemas**. The shim is a pure relay: `initialize` and `tools/list` are forwarded to the daemon. In daemon-unreachable (degraded) mode the shim answers `initialize` locally and returns a **static, vendored tool list** for `tools/list`, and a structured error for `tools/call`. **Both** the vendored tool list **and** the `initialize` capabilities/`protocolVersion` reported in degraded mode are generated from the same daemon-registry source at build time, so a degraded `initialize` cannot drift from what the daemon advertises.

**Why**: `tools/list` must return input/output schemas and `initialize` must report capabilities; without a single build-time source, the shim would either duplicate (drift) or be unable to answer when degraded.

### 7. Registration is idempotent and reversible

**Decision**: Registering `cluihud mcp` into agent configs (CC `mcpServers` in `~/.claude.json`; Codex/Pi/OpenCode equivalents) is idempotent (no duplicate entries on re-run). The registered command **pins the installed absolute path `/usr/bin/cluihud`** (the `.deb`/`.rpm` install location), explicitly **not** a `$PATH` resolution at registration time — `$PATH` would bake in the `~/.cargo/bin/cluihud` shadow that CLAUDE.md documents. Cleanup is **best-effort at disable time** (the app is running and can edit the configs). Uninstall-time deregistration is **not** attempted from maintainer scripts (those run as root, but the configs live in each user's `$HOME` — fragile and unspecified); an orphaned entry after uninstall degrades to a structured error when the agent tries to spawn the missing binary, not a hard agent failure.

**Why**: The project already hit binary-path fragility (cargo-install shadowing `/usr/bin/cluihud`, per CLAUDE.md). Pinning the install path avoids re-introducing the shadow; honest best-effort cleanup avoids promising an uninstall hook that can't be reliably implemented across multi-user `$HOME` layouts.

### 8. Default-off until the trust baseline is proven

**Decision**: `mcp_server_enabled` defaults to **off** for the initial release. It is opt-in until the directory's global-read posture (Decision 2b) is something the user knowingly turns on and the dependent changes exist.

**Why**: A partial-failure daemon still gets registered into agent configs; default-off is the safe rollback posture, and the directory exposes cross-workspace data to any same-uid caller (Decision 2b) so it should be an explicit opt-in.

## Risks / Trade-offs

**[Risk] New transport is more code than "reuse the socket"** → Accepted and re-estimated (see proposal `files_estimate`). It is the honest cost; the fire-and-forget hook socket genuinely cannot answer requests.

**[Risk] Transport framing bugs (partial reads, short writes, length corruption)** → The length-prefixed read-exact loop is the highest-risk new code. Enforced by explicit unit tests for fragmented frames, oversized length, and zero-length payloads (tasks §8.1). Unix socket perms (0600) + uid check behind the identity abstraction for a Windows port.

**[Risk] Mutex held across slow I/O stalls the reactor** → Snapshot-then-release (Decision 3) has two enforcement layers because no single check covers both hazard shapes: (a) holding the guard across an `.await` is caught automatically by **`clippy::await_holding_lock`** (in `cargo clippy -- -D warnings`); (b) holding the `std::sync::Mutex` across a **synchronous** blocking call (git via `std::process::Command` in `worktree.rs`, blocking `std::fs`) has **no `.await`**, so the lint cannot see it — this case is prevented structurally (the assembly function takes a snapshot of owned data and the guard is dropped before any git/fs/subprocess call) and verified in **code review**. The structural rule, not the lint, is the primary guarantee; the lint is a backstop for the async sub-case.

**[Risk] AI summary cost/privacy** → Off by default, per-project override, cheap model, dedicated configured key (no agent-auth reuse), SQLite row only when enabled, nothing read/invoked when disabled.

**[Trace] Tests** → JSON-RPC dispatch, transport framing (fragmented/oversized/zero-length), the identity-validation table (valid env id matching a live session / invalid id → unidentified / connect-before-register → lazy resolve), the disabled-daemon error path, and descriptor assembly are pure or near-pure and MUST have unit tests (not manual-only). See tasks §8.
