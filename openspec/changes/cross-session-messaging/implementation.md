# Implementation Plan: cross-session-messaging

> Depends on `cluihud-mcp-server` (daemon, directory, identity). Grounded in `src-tauri/src/`; re-verify symbols before editing.

## Execution order

1. Message + thread store (SQLite via `db.rs`).
2. `SessionDelivery` abstraction (PTY wake + Stop `additionalContext`).
3. Messaging tools on the MCP daemon (`send_to_session`/`read_messages`/`list_threads`/`search_sessions`).
4. Thread router (hop cap / dedup / budget).
5. Non-authoritative enforcement (security) on the permission/auto-mode path.
6. Frontend history panel + `SessionRow` badge.

## 1. Store — extend `src-tauri/src/db.rs`

`db.rs` already owns the SQLite connection. Add tables + a small store API:
- `cross_session_threads(id, originator_session, participants_json, depth, status, budget_json, created_at)`.
- `cross_session_messages(id, thread_id, from_session, to_session, body, read_at, created_at)`.
- API: `create_or_join_thread`, `append_message`, `mark_read(session, ids)` (take-on-read), `threads_for(session)`, `unread_for(session)`.

## 2. SessionDelivery — new `src-tauri/src/mcp/delivery.rs`

```rust
trait SessionDelivery {
    fn wake_idle(&self, session: &str, note: &str) -> Result<()>;   // PTY stdin + \r
    fn queue_for_stop(&self, session: &str, note: &str);            // additionalContext on next Stop
}
```
- **Idle path**: reuse the PTY writer in `pty.rs` (`write_to_session_pty` / `SharedWriter`). Only inject when `SessionStatus == Idle` (`models.rs:8`). Wake note instructs the agent to call `read_messages`; `\r` submits. NOTE: aux/quake shells skip `CLUIHUD_SESSION_ID` (`pty.rs:104`) — only inject into the owning agent PTY, never an aux shell.
- **Working path**: maintain a `HashMap<SessionId, PendingDelivery>`. The Stop hook handler in `hooks/server.rs` checks this map and, when present, returns `hookSpecificOutput.additionalContext` with the wake note (CC v2.1.163) — keeps the turn alive, no hook error.
- **Fallback**: agents without Stop `additionalContext` (capability flag per adapter) → `wake_idle` on next idle transition.
- Behind the trait so unix PTY today, other mechanisms later (multiplatform).

## 3. Messaging tools — extend `src-tauri/src/mcp/`

- `send_to_session(to, message, thread_id?)`: resolve target via directory; if inactive → structured error pointing to `create_worktree_session`; else `append_message` + `create_or_join_thread`, trigger `SessionDelivery`, return `{ thread_id, status }`.
- `read_messages(thread_id?)`: `unread_for(caller)`, mark read, return minimal payload.
- `list_threads()`: `threads_for(caller)`.
- `search_sessions(query)`: read-only across active (directory) + inactive (transcripts/summaries via `search/mod.rs` — the existing search engine). Inactive results flagged not-messageable.

## 4. Thread router — `src-tauri/src/mcp/router.rs`

- Config `cross_session_max_hops` (default 4) in `config.rs`.
- On `send_to_session`: compute new `depth`; reject over cap with `{ error: "hop_limit_reached" }`.
- Dedup: hash `(from, to, normalize(message))`; if seen in the thread → no-op.
- Budget/timeout per thread; on exhaustion mark `status = closed`, notify participants (a final delivery), refuse further sends.
- Async responses: replies are normal `send_to_session` calls tagged with `thread_id`; originator is never blocked (no synchronous wait).

## 5. Non-authoritative enforcement (security — critical)

- Tag the injected note/`additionalContext` as relayed + non-authoritative, naming the origin session.
- Find the permission/auto-mode approval path (plan-review / permission-request handling in `hooks/` + `plan_state.rs`). Ensure a permission request that arises while a session is acting on a relayed message is NOT auto-approved, even in auto-mode. Concretely: mark the delivery context with a `relayed: true` flag carried through to the permission decision; auto-approve logic must treat `relayed` as "always ask". Mirror CC v2.1.166.
- This phase MUST get the security reviewer (B5 escalation) — it is the spine of the critical tier.

## 6. Frontend — history panel + badge

- `src/stores/crossSession.ts`: `threadsAtom`, `unreadMapAtom`; subscribe to Tauri events `crossmsg:new` / `crossmsg:read`.
- Right-panel "Cross-session" view (mirror the Activities/Tasks panel pattern): thread list + thread detail (sender, workspace, hop indicator, timestamp, status). Persistent + navigable.
- TopBar icon + shortcut — **verify `src/stores/shortcuts.ts` for collisions** (`event.code`, WebKitGTK convention).
- `SessionRow` unread badge: reuse the `cluihud-ask-pending` indicator pattern; clears on read.

## Per-phase risk

- **Phase 2 (delivery)**: PTY injection into a non-idle or aux shell corrupts terminal state. Mitigate: strict `Idle` guard + never target aux shells; prefer `additionalContext` for the working case.
- **Phase 4 (router)**: dedup against the wrong set causes either loops or premature drops. Dedup against the thread's seen-set, not against confirmed-only.
- **Phase 5 (authority)**: the highest-risk phase. No relayed message may escalate privilege. Adversarial test: craft a message that tries to make the target auto-approve a write; must fail.

## Verification

`cd src-tauri && cargo clippy -- -D warnings && cargo test && cargo fmt --check` · `npx tsc --noEmit` · manual A↔B (idle + working), A→B→C hop cap, non-authoritative permission test, inactive search refusal, panel review + badge clear (see proposal Build contract).
