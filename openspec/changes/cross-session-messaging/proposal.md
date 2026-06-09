## Why

With the session directory in place (`cluihud-mcp-server`), an agent can *see* its sibling sessions but cannot *talk* to them. The real workflow is: session A (frontend, workspace W1) hits a question that only the agent who built the backend (session B, workspace W2) can answer; the user says "ask the backend session"; A and B converse to consensus; the answer may require pulling in a third session (workspace W3). Today this is a manual copy-paste relay using a shared markdown file and the word "CAMBIO".

cluihud owns every PTY, the hook pipeline, and now the MCP directory — it is the natural autonomous router. This change adds agent-to-agent messaging over MCP, agent-agnostic and supporting the transitive A→B→C case, replacing the messaging half of the archived `context-bridge` (file-bus + CAMBIO/CONSENSO, never implemented). The agent never sees the delivery mechanics; it just calls tools.

## What Changes

- New MCP tools on the cluihud server: `send_to_session`, `read_messages`, `list_threads`, and a read-only `search_sessions` (over active **and** inactive sessions).
- **cluihud-owned message store** (SQLite), separate from any agent transcript — the durable, auditable record of every cross-session exchange.
- **Hybrid delivery** that wakes a target session without the user: target idle → PTY stdin injection of a short "you have messages, call `read_messages`" wake prompt; target working → queue, then deliver on its next `Stop` via `hookSpecificOutput.additionalContext` (CC v2.1.163), falling back to PTY injection for agents without that capability.
- **Non-authoritative injection**: relayed cross-session context carries no user authority — cluihud SHALL NOT auto-approve permission requests or destructive actions triggered by acting on a relayed message (mirrors CC v2.1.166).
- **Thread model** for the transitive case: every exchange belongs to a thread with an id, originator, participant set, hop depth, and a budget; the router enforces a max-hop cap, deduplicates identical questions, and applies a per-thread timeout/budget to prevent infinite or runaway relays. Responses are delivered asynchronously so a caller is never blocked waiting on a long chain.
- **Active vs inactive asymmetry**: messaging targets a session with a live agent; an inactive (closed) session can only be *read* (transcript/summary via `search_sessions`), not messaged — to involve it the agent uses `agent-spawned-worktrees` to revive/create.
- New UI: a dedicated, navigable **right panel** ("Cross-session") holding the persistent thread history, plus a lightweight unread badge on `SessionRow`.

## Capabilities

### New Capabilities
- `cross-session-messaging`: The `send_to_session` / `read_messages` / `list_threads` / `search_sessions` tools, the message store, hybrid state-aware delivery, the non-authoritative rule, and the thread model with hop cap / dedup / budget.
- `cross-session-history-ui`: The right-panel thread viewer (persistent, navigable, auditable) and the `SessionRow` unread badge.

### Modified Capabilities
<!-- Builds on cluihud-mcp-server (directory + identity). Reuses the existing PTY writer and mode map. No existing spec-level behavior changes. -->

## Impact

- **Backend**: extend `src-tauri/src/mcp/` with messaging tools; new message/thread store (SQLite tables); a `SessionDelivery` abstraction wrapping PTY injection + Stop-hook `additionalContext`; Stop-hook handler returns `additionalContext` when deliveries are queued; thread router (hop cap, dedup, budget).
- **Frontend**: new `stores/crossSession.ts`; right-panel view + thread list/detail components; `SessionRow` unread badge; TopBar icon + keyboard shortcut (verify `shortcuts.ts`).
- **File system**: messages live in the cluihud SQLite DB, not in project files.
- **Existing flows**: the Stop-hook handler gains a delivery-injection branch; the PTY writer gains a wake-prompt path. Both behind the delivery abstraction.

## Build contract

### Qué construyo
- MCP tools: `send_to_session`, `read_messages`, `list_threads`, `search_sessions`.
- SQLite message + thread store (cluihud-owned).
- `SessionDelivery` hybrid: idle → PTY wake; working → Stop-hook `additionalContext` (fallback PTY).
- Non-authoritative enforcement on relayed-message-triggered permission/destructive actions.
- Thread model: id, originator, participants, hop depth, max-hop cap, dedup, per-thread budget/timeout, async responses.
- Active/inactive asymmetry (send → active only; search → read-only over both).
- Right-panel "Cross-session" thread viewer + `SessionRow` unread badge + shortcut.

### Cómo verifico
- `cd src-tauri && cargo clippy -- -D warnings && cargo test && cargo fmt --check`
- `npx tsc --noEmit`
- Manual: A `send_to_session(B)` while B idle → B wakes and reads; repeat while B working → B receives on next Stop without an error turn; drive A→B→C and confirm the hop cap rejects beyond the limit; confirm a relayed message cannot auto-approve a permission prompt; confirm the right panel shows the full thread and the badge clears on read.

### Criterio de done
- A and B exchange a full turn autonomously with no user copy-paste, in both idle and working target states.
- The transitive A→B→C relay works and is bounded by the hop cap with dedup and budget enforced.
- A relayed message never carries user authority (no auto-approve, no destructive trigger).
- `search_sessions` finds an inactive session read-only and refuses to message it.
- The thread history is fully reviewable later in the right panel; the unread badge reflects state.

### Estimated scope
- files_estimate: 16
- risk_tier: critical
- tags: [feature, security]
- visibility: public
- spec_target: cross-session-messaging, cross-session-history-ui
