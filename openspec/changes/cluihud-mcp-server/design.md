## Context

cluihud is a long-lived Tauri process that already holds, in memory and SQLite, the state of every session it spawns: PTY handles, `CLUIHUD_SESSION_ID` per session, mode map (idle/active/tool), git metadata, activity stream, and transcript paths. It communicates with hook CLI subprocesses over a Unix socket (`/tmp/cluihud.sock`) and FIFOs. The agent adapters (`src-tauri/src/agents/<agent>/adapter.rs`) inject `CLUIHUD_SESSION_ID` at spawn for every agent.

Modern agent CLIs support MCP servers as stdio subprocesses. CC injects `CLAUDE_CODE_SESSION_ID` + `CLAUDECODE=1` into stdio MCP server env (v2.1.154, also on `--resume` per v2.1.163). Codex, Pi, and OpenCode all support MCP server configuration.

This change exposes cluihud's session state to those agents through MCP, without inventing a new protocol the agent must learn.

## Goals / Non-Goals

**Goals:**
- A single source of truth (one daemon) for the live session directory across all workspaces.
- Agent-agnostic access via a thin stdio shim, reusing the existing socket IPC.
- Zero-config identity correlation between the MCP connection and the cluihud session.
- Cheap, always-fresh directory metadata that needs no AI.
- Opt-in AI summaries as a separable enrichment, never a hard dependency.
- Fold background tasks/crons (CC v2.1.150) into the session descriptor additively.

**Non-Goals:**
- Cross-session *messaging* (send/read between sessions) — that is `cross-session-messaging`.
- Agent-initiated *creation* of sessions — that is `agent-spawned-worktrees`.
- Exposing transcripts wholesale over MCP (a summary + recent activity is enough here; full search lands in `cross-session-messaging`).
- HTTP/SSE MCP transport (stdio shim is sufficient for v1; the daemon boundary leaves HTTP as a later option).

## Decisions

### 1. Single daemon + stdio shim, not a server per session

**Decision**: The MCP "server" is the cluihud app process (the daemon). Each agent spawns a thin stdio shim (`cluihud mcp`) that speaks MCP JSON-RPC on its stdin/stdout and forwards requests to the daemon over `/tmp/cluihud.sock`. The daemon answers from its global state.

**Why**: The directory and (later) routing need a global view of all sessions. A per-session in-process server would each see only its own session. A single daemon already exists (the app) and already owns the state. The shim is stateless and cheap.

**Alternatives considered**:
- *In-process MCP server per agent*: no global view; would need inter-process gossip. Rejected.
- *HTTP/SSE endpoint from the daemon, agents configured with HTTP transport*: viable and avoids a subprocess, but stdio reuses the existing socket + the `cluihud` binary that is already installed and registered, and gets the `CLAUDE_CODE_SESSION_ID` env injection for free. HTTP is kept as a future option behind the same daemon boundary. Rejected for v1.

### 2. Identity correlation via env, reported by the shim on connect

**Decision**: On startup the shim reads `CLUIHUD_SESSION_ID` (always present — our adapters inject it) and `CLAUDE_CODE_SESSION_ID` (present for CC). It announces both to the daemon on the socket handshake. The daemon records the mapping; `CLUIHUD_SESSION_ID` is authoritative, `CLAUDE_CODE_SESSION_ID` is a confirming cross-check for CC sessions.

**Why**: This makes `whoami` and all per-caller routing zero-config. It works for non-CC agents because `CLUIHUD_SESSION_ID` is agent-agnostic.

**Edge**: If neither env var is present (agent launched outside cluihud, or MCP started before adapter injection), the daemon returns a clearly-marked "unidentified caller" and `whoami` reports null. The shim does not guess.

### 3. Directory metadata is cheap and always fresh; AI summary is separate

**Decision**: `list_sessions` / `get_session` return only data cluihud already has in memory or can read instantly: name, workspace, branch, agent, mode + `waitingFor`, last-activity timestamp, recently-touched files, and background tasks/crons. No AI, no transcript parse on the hot path.

**Why**: The common case (an agent choosing whom to talk to) needs current, free metadata. Coupling the directory to AI summarization would make every `list_sessions` slow and costly.

**CC enrichment (optional)**: For CC sessions the daemon MAY cross-check `waitingFor`/`state` against `claude agents --json` (v2.1.162/168), but the daemon's own state is primary so the directory stays agent-agnostic.

### 4. Background tasks / crons folded in additively

**Decision**: Extend `HookEvent::Stop` (and `SubagentStop`) with `background_tasks: Option<Vec<BackgroundTask>>` and `session_crons: Option<Vec<SessionCron>>`, both `#[serde(default)]`. Capture into session state; surface in `get_session`.

**Why**: CC v2.1.150 already sends these. Additive deserialization keeps older payloads valid. This closes the "Surface background tasks y crons" backlog item by exposing them through the directory rather than (only) a panel.

### 5. Opt-in AI summary via a shared detached summarizer

**Decision**: A `session-summary` capability. When enabled (global setting, per-project override), a detached runner invokes a cheap model (haiku) to read the transcript and produce a short rolling summary (a few sentences). Stored in the session store; refreshed on `Stop` (debounced) and on demand. The same summarizer entrypoint serves M4's post-session MOC summary (different consumer, same machinery).

**Why**: Summaries cost tokens and touch transcript content (privacy). Off by default. Sharing the summarizer with M4 avoids two divergent summarization paths.

**Refresh policy**: debounce on `Stop` (avoid summarizing mid-turn); cap frequency; never block a directory read — if no summary exists yet, the descriptor's `summary` is null.

### 6. Delivery/IPC behind an adapter boundary (multiplatform constraint)

**Decision**: The socket transport between shim and daemon is accessed through a small transport trait, and any future wake/delivery mechanism (used by `cross-session-messaging`) sits behind a `SessionDelivery` abstraction. v1 implements the unix path (`/tmp/cluihud.sock`); the trait leaves room for Windows named pipes without touching call sites.

**Why**: The "App multiplataforma" backlog item is a known future direction. Pinning unix socket assumptions into the MCP/messaging core would inflate that future port. The abstraction is cheap to add now.

## Risks / Trade-offs

**[Risk] Shim cannot reach the daemon (app not running / socket missing)** → The shim still satisfies the MCP handshake and returns a structured error on tool calls ("cluihud daemon not reachable"), so the agent gets a clean failure rather than a hang. Mirror CC v2.1.162's fix for deep `$TMPDIR` socket paths: validate the socket path early.

**[Risk] Unidentified caller pollutes the directory** → Unidentified shims are not added to the directory and cannot resolve `whoami`; they receive a read-only view at most. No silent guessing.

**[Risk] AI summary leaks transcript content / costs tokens** → Strictly opt-in, off by default, per-project override; nothing is written or sent when disabled. Cheap model only.

**[Risk] Stale directory data** → Metadata is read from live state on each call, not cached, so mode/branch/activity are current. The only potentially-stale field is the AI summary, which is explicitly best-effort and timestamped.

**[Trade-off] stdio shim adds a subprocess per session** → Accepted. It is thin and short-lived per request batch, reuses the existing binary, and buys agent-agnostic identity correlation for free.
