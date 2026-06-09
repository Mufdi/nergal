## Why

With the session directory in place (`cluihud-mcp-server`), an agent can *see* its sibling sessions but cannot *talk* to them. The real workflow is: session A (frontend, workspace W1) hits a question that only the agent who built the backend (session B, workspace W2) can answer; the user says "ask the backend session"; A and B converse to consensus; the answer may require pulling in a third session (workspace W3). Today this is a manual copy-paste relay using a shared markdown file and the word "CAMBIO".

cluihud owns every PTY, the hook pipeline, and now the MCP directory — it is the natural autonomous router. This change adds agent-to-agent messaging over MCP, agent-agnostic and supporting the transitive A→B→C case, replacing the messaging half of the archived `context-bridge` (file-bus + CAMBIO/CONSENSO, never implemented). The agent never sees the delivery mechanics; it just calls tools.

## What Changes

- New MCP tools on the cluihud server: `send_to_session`, `read_messages`, `list_threads`, and a read-only `search_sessions` (over active **and** inactive sessions).
- **cluihud-owned message store** (SQLite), separate from any agent transcript — the durable, auditable record of every cross-session exchange.
- **Hybrid delivery** that wakes a target without the user: idle → PTY stdin wake prompt; working → queue, deliver via the `cluihud hook stop` CLI emitting `hookSpecificOutput.additionalContext` on stdout (CC v2.1.163; the hook socket is fire-and-forget, so the CLI's stdout is where it's emitted). **Liveness guarantee**: every working→idle transition drains the pending queue via PTY wake for ALL agents, so a message sent just after a `Stop` is never stranded. Delivery keys off `agent_consumed_at`, a column separate from the UI's `human_seen_at` (the UI never cancels delivery).
- **Non-authoritative posture (labeling + documented limits only)**: relayed context is labeled advisory. cluihud cannot attribute a downstream autonomous action to a relayed message (unobservable reasoning), so no provenance gate is claimed; its own gates are human-decided by construction, and it cannot override the agent's `--permission-mode` (`models.rs:104`) — both documented, not implied.
- **Thread model** for the transitive case: a thread with id/originator/participants/status; each message carries its own `depth` (per-branch hop cap, not a thread scalar); dedup returns a distinct `duplicate_suppressed` status; budget is a **message-count cap + wall-clock deadline** (NOT tokens — cluihud can't measure agent-side tokens). Async replies; caller never blocks.
- **Kill-switch**: a `cross_session_messaging_enabled` config flag (default off) gates ALL delivery — the halt switch for a critical-tier autonomous PTY-injecting router.
- **Active vs inactive asymmetry**: messaging targets a live agent; an inactive session is *read-only* via `search_sessions` (the existing `search/mod.rs` `transcripts_dir` scope), not messaged — to involve it the agent uses `agent-spawned-worktrees`.
- New UI: a dedicated, navigable **right panel** ("Cross-session") holding the persistent thread history, plus a `human_seen`-based unread badge on `SessionRow`.

## Capabilities

### New Capabilities
- `cross-session-messaging`: The `send_to_session` / `read_messages` / `list_threads` / `search_sessions` tools, the migration-backed message store (consumed-vs-seen separated), hybrid state-aware delivery with the idle-transition liveness drain, the non-authoritative labeling posture, the per-message thread model (hop cap / dedup-status / count-time budget), and the kill-switch.
- `cross-session-history-ui`: The right-panel thread viewer (persistent, navigable, auditable) and the `SessionRow` unread badge.

### Modified Capabilities
<!-- Builds on cluihud-mcp-server (directory + identity). Reuses the existing PTY writer and mode map. No existing spec-level behavior changes. -->

## Impact

- **Backend**: extend `src-tauri/src/mcp/` with messaging tools; migration `015_cross_session.sql` (thread + message tables, registered in `db.rs:132`); a `SessionDelivery` abstraction wrapping PTY injection + the `cluihud hook stop` CLI stdout emit (the socket is fire-and-forget); thread router (per-message hop cap, dedup-status, count/time budget); kill-switch config.
- **Frontend**: new `stores/crossSession.ts`; right-panel view + thread list/detail components; `SessionRow` unread badge; TopBar icon + keyboard shortcut (verify `shortcuts.ts`).
- **File system**: messages live in the cluihud SQLite DB, not in project files.
- **Existing flows**: the Stop-hook handler gains a delivery-injection branch; the PTY writer gains a wake-prompt path. Both behind the delivery abstraction.

## Build contract

### Qué construyo
- MCP tools: `send_to_session`, `read_messages`, `list_threads`, `search_sessions`.
- Migration `015_cross_session.sql` (thread + message tables; `agent_consumed_at` vs `human_seen_at`; per-message `depth`; `dedup_key`).
- `SessionDelivery` hybrid: idle → sanitized PTY wake; working → `cluihud hook stop` CLI stdout `additionalContext`; **idle-transition drain for ALL agents** (no stranding).
- Non-authoritative posture: labeling + documented limits (no provenance gate — unattributable).
- Thread router: per-message hop cap, dedup → `duplicate_suppressed`, msg-count + wall-clock budget, async replies.
- Kill-switch config `cross_session_messaging_enabled` (default off).
- Active/inactive asymmetry (send → active only; search → read-only over both via `search/mod.rs` transcripts scope).
- Right-panel "Cross-session" thread viewer + `human_seen` `SessionRow` badge + shortcut.

### Cómo verifico
- `cd src-tauri && cargo clippy -- -D warnings && cargo test && cargo fmt --check`
- `npx tsc --noEmit`
- Automated `cargo test`: hop-cap rejection, dedup → `duplicate_suppressed`, budget exhaustion, `agent_consumed_at`/`human_seen_at` separation, idle-transition drain.
- Manual: A `send_to_session(B)` while B idle → B wakes and reads; while B working → B receives on next Stop without an error turn; send just after B's Stop → delivered on next idle (not stranded); A→B→C per-message hop cap rejects beyond the limit; opening the panel does not cancel a pending delivery.

### Criterio de done
- A and B exchange a full turn autonomously, in both idle and working states, including the send-just-after-Stop case (no stranding).
- The transitive A→B→C relay is bounded by the per-message hop cap; dedup returns `duplicate_suppressed`; msg-count/deadline budget closes the thread.
- Relayed context is labeled advisory; the kill-switch halts all delivery; documented limits are accurate (no over-claimed enforcement).
- `search_sessions` finds an inactive session read-only and refuses to message it.
- UI badge tracks `human_seen` and never cancels an agent delivery.

### Estimated scope
- files_estimate: 18
- risk_tier: critical
- tags: [feature, security]
- visibility: public
- spec_target: cross-session-messaging, cross-session-history-ui
