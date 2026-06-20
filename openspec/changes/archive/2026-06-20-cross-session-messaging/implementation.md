# Implementation Plan: cross-session-messaging

> Depends on `cluihud-mcp-server` (daemon, directory, identity). Grounded in `src-tauri/src/`; re-verify symbols before editing.

## Execution order

1. Message + thread store (migration `015` + `db.rs`).
2. `SessionDelivery` abstraction (PTY wake + Stop-CLI stdout `additionalContext`).
3. Messaging tools on the MCP daemon (`send_to_session`/`read_messages`/`list_threads`/`search_sessions`).
4. Thread router (per-message reach hop cap / dedup-status / count-time budget).
5. Non-authoritative posture (labeling + docs only — NO permission interceptor; cluihud has none).
6. Kill-switch config + frontend history panel + `SessionRow` badge.

## 1. Store — migration + `src-tauri/src/db.rs`

Per config.yaml, SQLite schema lands as a migration, not ad-hoc. Add `src-tauri/migrations/015_cross_session.sql` (after change 1's `014_session_summaries`; renumber if change 1 lands a different number) and register `include_str!` in the `db.rs:132` array. Schema MUST match tasks §1.1:
- `cross_session_threads(id TEXT PRIMARY KEY, originator_session TEXT NOT NULL, participants TEXT NOT NULL, status TEXT NOT NULL DEFAULT 'active', max_hops INTEGER NOT NULL, msg_count INTEGER NOT NULL DEFAULT 0, msg_budget INTEGER, deadline_at INTEGER, created_at INTEGER NOT NULL)`. Budget = message-count + wall-clock, NOT tokens.
- `cross_session_messages(id TEXT PRIMARY KEY, thread_id TEXT NOT NULL REFERENCES cross_session_threads(id), from_session TEXT NOT NULL, to_session TEXT NOT NULL, body TEXT NOT NULL, depth INTEGER NOT NULL, dedup_key TEXT NOT NULL, agent_consumed_at INTEGER, human_seen_at INTEGER, created_at INTEGER NOT NULL)` + index `(to_session, agent_consumed_at)`. `depth` is per-message **reach** (see §4); `agent_consumed_at` (delivery) and `human_seen_at` (UI) are SEPARATE columns.
- Store API in `db.rs`: `create_or_join_thread`, `append_message` (computes reach `depth`, `dedup_key`), `mark_agent_consumed(session, ids)`, `mark_human_seen(thread)`, `threads_for(session)`, `agent_undelivered_for(session)`.

## 2. SessionDelivery — new `src-tauri/src/mcp/delivery.rs`

```rust
trait SessionDelivery {
    fn wake_idle(&self, session: &str, note: &str) -> Result<()>;   // sanitized PTY stdin + \r
    fn queue_for_stop(&self, session: &str, note: &str);            // emitted by `cluihud hook stop` CLI stdout
    fn drain_on_idle(&self, session: &str);                         // called by the mode-map writer on →idle (NEW-4)
}
```
- **Idle path**: reuse the PTY writer in `pty.rs` (`write_to_session_pty` / `SharedWriter`). Only inject when `SessionStatus == Idle` (`models.rs:8`). Wake note instructs the agent to call `read_messages`; `\r` submits. NOTE: aux/quake shells skip `CLUIHUD_SESSION_ID` (`pty.rs:104`) — only inject into the owning agent PTY, never an aux shell.
- **Working path**: maintain a `HashMap<SessionId, PendingDelivery>` in the daemon. The hook **socket is fire-and-forget** (`server.rs:205-261`) and cannot return data, so the `additionalContext` is emitted by the **`cluihud hook stop` CLI command's stdout**, not by the server: the CLI queries the daemon for pending deliveries (request/response over a FIFO, mirroring `plan-review` `cli.rs:123-188`) and, if present, prints `{"hookSpecificOutput":{"additionalContext": "<note>"}}` (CC v2.1.163) — same stdout-JSON return as `inject_edits` (`cli.rs:64-106`) and ask-user (`cli.rs:303-328`). Requires a `Stop` hook command registered in the agent hook config.
- **Liveness / TOCTOU (NEW-4)**: the strand window (send reads stale `working` after `Stop` already fired) is closed by making the **mode-map writer** — the single code path that flips a session to idle — own the drain: on every →idle flip it calls `drain_on_idle`, which wakes if the pending queue is non-empty. Additionally `send_to_session` re-reads mode immediately after enqueue and wakes if already idle. Serializing enqueue against the mode flip (both go through the same lock) removes the race. This applies to ALL agents; `additionalContext` is the best-effort fast path layered on top.
- **Fallback**: agents without Stop `additionalContext` rely solely on the idle-drain above. Mode-observation reliability per adapter is the load-bearing assumption (round-1 finding 12) — documented as an open dependency for non-CC delivery.
- Behind the trait so unix PTY today, other mechanisms later (multiplatform).

## 3. Messaging tools — extend `src-tauri/src/mcp/`

- `send_to_session(to, message, thread_id?)`: resolve target via directory; if inactive → structured error pointing to `create_worktree_session`; else `append_message` (computes `depth`, `dedup_key`) + `create_or_join_thread`, trigger `SessionDelivery`, return `{ thread_id, status }` where status ∈ {delivered, queued, **duplicate_suppressed**, hop_limit_reached}.
- `read_messages(thread_id?)`: `agent_undelivered_for(caller)`, set `agent_consumed_at`, return minimal payload.
- `list_threads()`: `threads_for(caller)`.
- `search_sessions(query)`: read-only across active (directory) + inactive via `search/mod.rs` with `SearchContext.transcripts_dir` scope (verified: the engine already supports a `transcripts` scope, `search/mod.rs:81,165`; ripgrep-backed, perf is ripgrep-bounded). Inactive results flagged not-messageable.

## 4. Thread router — `src-tauri/src/mcp/router.rs`

- Config `cross_session_max_hops` (default 4), `cross_session_msg_budget` (default e.g. 30 messages), `cross_session_deadline` (default e.g. 30 min) in `config.rs`.
- **Reach hop cap (NEW-3 — distinguishes reach from conversation turns)**: `depth = sender_message_depth + (to_session is NOT already a thread participant ? 1 : 0)`. A reply between existing participants does NOT increment (A↔B ping-pong stays at one reach level); only pulling in a NEW participant increments (A→B→C→D = reach 1→2→3). Reject when reach would exceed `max_hops` with `{ error: "hop_limit_reached" }`. Conversation *length* is bounded by `msg_budget`, not the hop cap.
- Dedup: `dedup_key = hash(from, to, normalize(body))` (`normalize` = trim + collapse whitespace); if seen → return `{ status: "duplicate_suppressed" }` (distinct status, so the caller doesn't await a phantom reply).
- Budget: `msg_count` cap + wall-clock `deadline_at` (NOT tokens). On exhaustion: `status = closed`, notify participants **via `SessionDelivery`** (so the close-notification respects the kill-switch + idle/working guards — NEW-6), refuse further sends.
- **Deadline sweeper (NEW-5)**: an active daemon timer evaluates `deadline_at` (NOT lazy-on-send) so a stuck/idle thread actually closes and notifies the user — this is the safety net for the accepted at-most-once limitation (finding 9), which only works if something fires it.
- Async responses: replies are normal `send_to_session` calls tagged with `thread_id`; originator never blocks.

## 5. Non-authoritative posture (labeling + documentation only)

- Label the injected note/`additionalContext` as relayed + advisory, naming the origin session. This is the ONLY enforceable control: cluihud cannot attribute B's later autonomous gate request to a message B read earlier (unobservable reasoning), so a `relayed` provenance flag would be vacuous/over-trigger — it is NOT implemented (round-1 finding 1).
- Document: cluihud's own gates (plan-review FIFO, change-3 worktree gate) are human-decided by construction; cluihud has no central permission auto-approver (delegated to `--permission-mode`, `models.rs:104`) and cannot intercept it.
- Security reviewer still warranted (PTY injection surface), but the spine is the kill-switch + labeling, not a provenance interceptor.

## 6. Frontend — history panel + badge

- `src/stores/crossSession.ts`: `threadsAtom`, `humanUnseenMapAtom`; subscribe to Tauri events `crossmsg:new` / `crossmsg:agent-consumed` / `crossmsg:human-seen`.
- Right-panel "Cross-session" view (mirror the Activities/Tasks panel pattern): thread list + thread detail (sender, workspace, per-message hop indicator, timestamp, status). Persistent + navigable. Opening a thread sets `human_seen_at` only.
- TopBar icon + shortcut — **verify `src/stores/shortcuts.ts` for collisions** (`event.code`, WebKitGTK convention).
- `SessionRow` unread badge: reuse `cluihud-ask-pending`; clears on `human_seen_at`, never touches `agent_consumed_at`.

## Per-phase risk

- **Phase 2 (delivery)**: PTY injection into a non-idle or aux shell corrupts terminal state. Mitigate: strict `Idle` guard + never target aux shells (`pty.rs:104`); sanitize embedded strings. Liveness: always drain pending on working→idle for ALL agents, else messages strand (finding 3).
- **Phase 4 (router)**: dedup against the wrong set causes loops or premature drops. Dedup on `dedup_key` within the thread. `depth` per-message, not thread-global (finding 8).
- **Phase 5 (authority)**: honest scope — labeling + docs; no provenance gate. Adversarial test confirms labeling is present and the kill-switch halts delivery.

## Verification

`cd src-tauri && cargo clippy -- -D warnings && cargo test && cargo fmt --check` · `npx tsc --noEmit` · automated tests for hop-cap/dedup-status/budget/read-state-separation/idle-drain · manual A↔B (idle + working + send-just-after-Stop), A→B→C per-message hop cap, inactive search refusal, panel badge clears without cancelling delivery (see proposal Build contract).
