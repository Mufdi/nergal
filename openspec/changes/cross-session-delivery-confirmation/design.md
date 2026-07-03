## Context

The wake path is deliberately two-phase (paste, then a lone `\r` after ~250ms) because a same-burst Enter races the TUI's bracketed-paste exit (spec "Wake submit timing"). The bug is bookkeeping, not timing: `drain_idle` (`delivery.rs:144-152`) records `agent_consumed_at` when phase 1 succeeds, but "landed" only happens at phase 2, which runs detached with its error discarded (`delivery.rs:60-64`). Callers of `drain_idle` are synchronous contexts: the hook server's `Stop` processing (`hooks/server.rs:767`) and the messaging daemon handlers (`messaging.rs:226` inside a spawned task, `:232` sync).

## Goals / Non-Goals

**Goals:**
- `agent_consumed_at` is written only after the submit `\r` was actually delivered to the PTY writer.
- Submit failure → messages remain unconsumed → next idle flip retries (existing spec promise, now honored end-to-end).
- No change to paste/settle timing or the one-paste-per-idle rule.

**Non-Goals:**
- Verifying the agent *processed* the note (no protocol exists for that; PTY-write success is the strongest observable signal); async-trait refactor of `SessionDelivery`; sender-side delivery receipts UI.

## Decisions

### D1: completion callback over async trait

`wake_idle(&self, session_id, note, on_settled: Box<dyn FnOnce(bool) + Send + 'static>)`. The spawned settle task calls `on_settled(submit_ok)`. `drain_idle` builds the callback owning the message ids + a `SharedDb` clone + the emit handle, and performs mark-consumed + `crossmsg:agent-consumed` inside it on `true`.

**Alternatives considered:**
- *Make `wake_idle`/`drain_idle` async and await the settle*: cleanest types, but `drain_idle` is called from the hook server's synchronous event path (`hooks/server.rs:767`) — an idle wake would block hook processing for 250ms+ per drain, or force an async refactor of the hook pipeline. Rejected.
- *Optimistic consume + compensating un-consume on failure*: keeps today's flow, but the crash window (consumed-but-unsent survives a process kill between paste and compensation) is exactly the stranded state the spec forbids; also un-consume after a partial paste risks double-pasting the note text into the prompt on retry. Rejected.
- *Blocking sleep inside `wake_idle`*: blocks the hook thread; rejected.

### D2: in-flight guard keyed by session id

During the 250ms window the messages are (correctly) still unconsumed, so a concurrent `drain_idle` — e.g. send-path immediate wake racing a `Stop` drain — would paste the same bodies again. A `Mutex<HashSet<String>>` of in-flight session ids (owned by the delivery module, checked at `drain_idle` entry, removed inside `on_settled` before the DB write) closes this. This also strengthens the existing "at most one paste per idle transition" rule across *concurrent* drains, which today relies on call-ordering.

### D3: failure semantics = leave unconsumed, log at debug

On `on_settled(false)`: remove in-flight marker, write nothing, `tracing::debug!` (same level as today's paste-failure path at `delivery.rs:145-147`). The note text may sit un-submitted in the target prompt if the paste landed but the submit failed with the PTY alive — rare (submit failure normally means the PTY is gone); the retry pastes a fresh note listing the still-pending messages, which is the spec's designed recovery.

### D4: `NoopDelivery` calls `on_settled(false)` synchronously

Headless/daemon contexts without an app handle never deliver; invoking the callback with `false` keeps messages pending for a live-app drain (today it returns `Err`, and that classification stays: `Err` from `wake_idle` short-circuits before any callback).

## Risks / Trade-offs

- [Callback holds a `SharedDb` clone across 250ms] → it locks only inside the callback for one UPDATE; no guard is held while waiting.
- [In-flight marker leaks if the spawned task panics before calling the callback] → wrap the settle task body so the marker removal is unconditional (drop-guard pattern or `finally`-style block).
- [Sender UX: `send_to_session` may now report "queued"/pasted while confirmation is pending] → the tool's return value already doesn't promise consumption ("delivered" refers to the wake attempt); unchanged surface, documented in the spec delta.

## Open Questions

- None blocking.
