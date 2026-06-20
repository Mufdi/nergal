# Tasks — cross-session-messaging

> Redaction only. Depends on `cluihud-mcp-server`. Do NOT start implementation until the three-change set is approved.

## 1. Message + thread store (backend)

- [x] 1.1 Migration `src-tauri/migrations/015_cross_session.sql` (per config.yaml: SQLite schema as a migration, not ad-hoc; **depends on change 1 landing `014_session_summaries` — if change 1 ships a different number, renumber to next free** — round-1 finding 13); register `include_str!` in `db.rs:132`. Schema:
  - `cross_session_threads(id TEXT PRIMARY KEY, originator_session TEXT NOT NULL, participants TEXT NOT NULL, status TEXT NOT NULL DEFAULT 'active', max_hops INTEGER NOT NULL, msg_count INTEGER NOT NULL DEFAULT 0, msg_budget INTEGER, deadline_at INTEGER, created_at INTEGER NOT NULL)`. (Budget is **message-count + wall-clock deadline**, NOT tokens — cluihud cannot measure tokens spent inside agent turns; round-1 finding 6.)
  - `cross_session_messages(id TEXT PRIMARY KEY, thread_id TEXT NOT NULL REFERENCES cross_session_threads(id), from_session TEXT NOT NULL, to_session TEXT NOT NULL, body TEXT NOT NULL, depth INTEGER NOT NULL, dedup_key TEXT NOT NULL, agent_consumed_at INTEGER, human_seen_at INTEGER, created_at INTEGER NOT NULL)`. `depth` is **per-message reach**, computed per §3.1 (`sender_message_depth + (target is a NEW participant ? 1 : 0)`), not a thread scalar (round-1 finding 8) and not an unconditional `+1` (round-2 finding 3). `agent_consumed_at` (set by `read_messages`) and `human_seen_at` (set by UI) are **separate** — the UI must never cancel agent delivery (round-1 finding 2). Index `(to_session, agent_consumed_at)`.
- [x] 1.2 Store API: create/join thread, append message (computes `depth`, `dedup_key`), mark-agent-consumed (take-on-read), mark-human-seen (UI), list threads for a session, query agent-undelivered for a session.

## 2. Messaging MCP tools

- [x] 2.1 `send_to_session(to, message, thread_id?)`: validate target is active (else structured error → worktree-spawn path); record message; join/create thread; trigger delivery; return thread id + status.
- [x] 2.2 `read_messages(thread_id?)`: return caller's unread (minimal payload), mark read.
- [x] 2.3 `list_threads()`: return caller's threads with status/participants.
- [x] 2.4 `search_sessions(query)`: read-only over active + inactive (name/summary/transcript), inactive marked not-messageable.

## 3. Thread router (caps)

- [x] 3.1 **Reach** hop cap: `depth = sender_message_depth + (to_session is a NEW thread participant ? 1 : 0)`; reject when reach would exceed `max_hops` with `hop_limit_reached`. A reply between existing participants does NOT increment (two-party dialogue is bounded by `msg_budget`, not the hop cap — round-2 finding 3). Per-message/per-branch (round-1 finding 8).
- [x] 3.2 Dedup: `dedup_key = hash(from, to, normalize(body))` where `normalize` = trim + collapse internal whitespace (conservative exact-match; documented so callers know reworded follow-ups are NOT deduped). A duplicate returns a **distinct** status `duplicate_suppressed` so the caller doesn't wait for a phantom reply (finding 5).
- [x] 3.3 Budget = `msg_count` cap + wall-clock `deadline_at` (NOT tokens). **Active daemon timer sweeps `deadline_at`** (not lazy-on-send — round-2 finding 5) so stuck/idle threads close. On exhaustion: close thread + notify participants **via `SessionDelivery`** (respects kill-switch + guards — round-2 finding 6) + refuse further sends.
- [x] 3.4 Async response tagging (replies carry `thread_id` + "response to your query"); originator never blocks.

## 4. SessionDelivery abstraction (state-aware)

- [x] 4.1 Define `SessionDelivery` trait (multiplatform-ready); implement the unix path. Delivery keys off `agent_consumed_at IS NULL` (NOT `human_seen_at`) so the UI never cancels delivery (finding 2).
- [x] 4.2 Idle target → PTY stdin wake prompt via `write_to_session_pty` (`pty.rs`), `\r` submit, only when `mode == idle`, only the owning agent PTY (never aux/quake shells, `pty.rs:104`). **Sanitize** the embedded session name / any relayed string before it lands on stdin (finding 15).
- [ ] 4.3 **DEFERRED — documented (build-logs/impl-2026-06-19.md).** The idle-transition drain (4.4) is the universal delivery and fully meets every done-criterion (idle / working / send-just-after-Stop, no stranding); `additionalContext` only changes *when* delivery lands (same turn vs next idle), never *whether*. Naively running both conflicts: cluihud sees the same `Stop` event and can't tell whether CC continued (additionalContext emitted) or idled, so a PTY wake could land mid-turn (the corruption the design forbids); clean coexistence needs a per-Stop FIFO "keep-running" coordination on a hot path — a latency optimization the design itself frames as "best-effort … never instead of the idle-transition drain". Surfaced at review. Original: Working target → queue; extend the `cluihud hook stop` CLI command to query the daemon for pending deliveries (request/response over a FIFO, mirroring `plan-review` `cli.rs:123-188`) and print `{"hookSpecificOutput":{"additionalContext": "<wake note>"}}` to stdout (CC v2.1.163), same stdout-JSON pattern as `inject_edits` (`cli.rs:64-106`). The socket stays fire-and-forget; the CLI is where stdout is emitted. Requires a `Stop` hook command registered in the agent hook config.
- [x] 4.4 **Liveness (finding 3): every working→idle transition with a non-empty pending queue MUST PTY-wake, for ALL agents** (not just non-`additionalContext` ones). If a send arrives and the target is already idle, wake immediately rather than queueing for a `Stop` that may never fire. The `additionalContext` path is a best-effort fast path layered on top of, never instead of, the idle-transition drain. Mode-observation reliability per adapter is the load-bearing assumption for non-CC agents (finding 12) — document it.

## 5. Non-authoritative posture (security — labeling + documentation only)

- [x] 5.1 Label injected wake/`additionalContext` as relayed/advisory, naming the origin session. This is the ONLY enforceable control (round-1 finding 1: cluihud cannot attribute a downstream autonomous action of B to a relayed message B read earlier — the agent's reasoning is unobservable, so a `relayed_origin` gate flag would be vacuous or over-trigger; it is dropped).
- [x] 5.2 Document that cluihud's own gates (plan-review FIFO `cli.rs:123-188`, change-3 worktree gate) are human-decided **by construction** (a relayed message cannot auto-satisfy them because a human resolves them) — no new flag needed.
- [x] 5.3 Document the limitation: cluihud cannot override the target agent's `--permission-mode` (`models.rs:104`); a bypass-preset target acting on relayed text is the user's chosen posture, not something cluihud can intercept.

## 6. Config kill-switch + frontend

- [x] 6.1 Config `cross_session_messaging_enabled` (default off) in `config.rs` gating ALL delivery (PTY wake + Stop-hook emit) — a halt switch for a critical-tier autonomous PTY-injecting router (finding 10). Plus `cross_session_max_hops` (default 4), `cross_session_msg_budget` (default ~30 messages), `cross_session_deadline` (default ~30 min) — all with explicit defaults (round-2 finding 6).
- [x] 6.2 `src/stores/crossSession.ts`: threads atom, unread map atom (keyed on `agent_consumed_at`-independent UI state), Tauri event subscriptions (new-message, agent-consumed, human-seen).
- [x] 6.3 Right-panel "Cross-session" view: thread list + thread detail (sender, workspace, per-message hop indicator, timestamp, status). Persistent + navigable. Opening a thread sets `human_seen_at` only (never `agent_consumed_at`).
- [x] 6.4 TopBar icon + keyboard shortcut (verify `src/stores/shortcuts.ts` collisions).
- [x] 6.5 `SessionRow` unread badge (reuse `cluihud-ask-pending` pattern); clears on `human_seen_at`.

## 7. Verification

- [x] 7.1 **Automated `cargo test`** (not manual — finding 11) for the security spine: per-message hop-cap rejection, dedup → `duplicate_suppressed`, msg-count/deadline budget exhaustion → thread close, `agent_consumed_at` vs `human_seen_at` separation (UI seen does not cancel delivery), and the idle-transition drain (queued message delivered on working→idle).
- [x] 7.2 `cd src-tauri && cargo clippy -- -D warnings && cargo test && cargo fmt --check` + `npx tsc --noEmit`.
- [x] 7.3 **Live walk PASSED 2026-06-20.** The only defect surfaced was keyboard focus orphaning to `<body>` on the thread detail view (Backspace/arrows dropped); fixed by making the detail root focusable (`tabIndex={-1}`, commit on `main`) and re-verified. Manual: A↔B autonomous turn in both idle and working states (incl. send-just-after-Stop → delivered on next idle, not stranded); A→B→C bounded by per-message hop cap; relayed context is labeled advisory and cluihud's gates stay human-decided; `search_sessions` finds an inactive session read-only (via the `transcripts_dir` scope of `search/mod.rs`) and refuses to message it; UI badge clears on view without cancelling a pending agent delivery.
