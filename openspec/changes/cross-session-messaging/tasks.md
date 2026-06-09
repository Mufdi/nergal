# Tasks — cross-session-messaging

> Redaction only. Depends on `cluihud-mcp-server`. Do NOT start implementation until the three-change set is approved.

## 1. Message + thread store (backend)

- [ ] 1.1 SQLite tables `cross_session_threads` (`id`, `originator_session`, `participants`, `depth`, `status`, `budget`, `created_at`) and `cross_session_messages` (`id`, `thread_id`, `from_session`, `to_session`, `body`, `read_at`, `created_at`).
- [ ] 1.2 Store API: create/join thread, append message, mark-read (take-on-read), list threads for a session, query unread for a session.

## 2. Messaging MCP tools

- [ ] 2.1 `send_to_session(to, message, thread_id?)`: validate target is active (else structured error → worktree-spawn path); record message; join/create thread; trigger delivery; return thread id + status.
- [ ] 2.2 `read_messages(thread_id?)`: return caller's unread (minimal payload), mark read.
- [ ] 2.3 `list_threads()`: return caller's threads with status/participants.
- [ ] 2.4 `search_sessions(query)`: read-only over active + inactive (name/summary/transcript), inactive marked not-messageable.

## 3. Thread router (caps)

- [ ] 3.1 Enforce configurable max-hop cap; reject over-cap `send_to_session` with structured error.
- [ ] 3.2 Dedup identical (from, to, normalized message) within a thread → no-op.
- [ ] 3.3 Per-thread budget/timeout; on exhaustion close thread + notify participants + refuse further sends.
- [ ] 3.4 Async response tagging (replies carry `thread_id` + "response to your query"); originator never blocks.

## 4. SessionDelivery abstraction (state-aware)

- [ ] 4.1 Define `SessionDelivery` trait (multiplatform-ready); implement the unix path.
- [ ] 4.2 Idle target → PTY stdin wake prompt ("call `read_messages`"), `\r` submit, only when `mode == idle`.
- [ ] 4.3 Working target → queue; on next `Stop`, return `hookSpecificOutput.additionalContext` (CC v2.1.163) with the wake note, no hook error.
- [ ] 4.4 Fallback: agents without Stop `additionalContext` → PTY injection on next idle transition.

## 5. Non-authoritative enforcement (security)

- [ ] 5.1 Tag injected wake/context as relayed + non-authoritative; framing names the origin session.
- [ ] 5.2 Ensure relayed-message-triggered permission requests are NOT auto-approved, even in auto-mode (mirror CC v2.1.166). Verify against the existing permission/auto-mode path.

## 6. Frontend — history UI

- [ ] 6.1 `src/stores/crossSession.ts`: threads atom, unread map atom, Tauri event subscriptions (new-message, message-read).
- [ ] 6.2 Right-panel "Cross-session" view: thread list + thread detail (sender, workspace, hop indicator, timestamp, status). Persistent + navigable.
- [ ] 6.3 TopBar icon + keyboard shortcut (verify `src/stores/shortcuts.ts` collisions).
- [ ] 6.4 `SessionRow` unread badge (reuse `cluihud-ask-pending` pattern); clears on read.

## 7. Verification

- [ ] 7.1 `cd src-tauri && cargo clippy -- -D warnings && cargo test && cargo fmt --check`
- [ ] 7.2 `npx tsc --noEmit`
- [ ] 7.3 Manual: A↔B autonomous turn in both idle and working target states; A→B→C bounded by hop cap with dedup + budget; relayed message cannot auto-approve a permission; `search_sessions` finds an inactive session read-only and refuses to message it; right panel shows full thread + badge clears on read.
