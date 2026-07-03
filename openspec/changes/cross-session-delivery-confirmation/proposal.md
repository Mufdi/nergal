## Why

A cross-session message can be marked consumed while its text never reached the target agent. `AppBridge::wake_idle` (`src-tauri/src/mcp/delivery.rs:46-67`) returns `Ok(())` right after the PTY paste; the actual Enter/submit runs 250ms later in a detached `tauri::async_runtime::spawn` whose error is discarded (`let _ = crate::pty::submit_to_session(...)`, line 63). `drain_idle` (`delivery.rs:144-152`) treats that `Ok` as "delivered" and immediately sets `agent_consumed_at` in the DB. If the target PTY closes or switches within the 250ms window, the submit fails silently: the note sits unsent (or is lost with the PTY), the message is already consumed and never retried — while the sender saw "delivered". This violates the existing spec sentence "If the wake fails to land the messages SHALL be left unconsumed so the next idle flip retries — never stranded" (`openspec/specs/cross-session-messaging/spec.md:76`): today only the *paste* failure path honors it, not the *submit* failure path.

## What Changes

- **Consumption moves after the confirmed submit**: `wake_idle` keeps its paste-then-settled-submit shape (the timing is walk-hardened and spec-mandated) but reports completion — the settle + `submit_to_session` outcome — back to the caller instead of discarding it. `drain_idle` marks `agent_consumed_at` (and emits `crossmsg:agent-consumed`) only in that completion path, on success.
- **In-flight guard**: while a wake's submit is pending, a per-session in-flight marker prevents a concurrent drain from re-pasting the same messages (they are still unconsumed in the DB during the window). Cleared on completion either way.
- **Failure leaves messages unconsumed** (no DB write), so the next working→idle transition retries per the existing spec contract.
- Trait shape: `SessionDelivery::wake_idle` grows a completion callback (`on_settled: Box<dyn FnOnce(bool) + Send>`) rather than becoming async — both call contexts (`hooks/server.rs:767` hook processing, `mcp/messaging.rs:226/232`) are sync; design.md D1 weighs this against an async trait.

## Capabilities

### New Capabilities

_None._

### Modified Capabilities

- `cross-session-messaging`: the "Hybrid state-aware delivery" requirement's definition of a landed wake is tightened — consumption is recorded only after the deferred submit is confirmed, and an in-flight wake is not double-pasted.

## Impact

- **`src-tauri/src/mcp/delivery.rs`**: `SessionDelivery` trait (25-32), `AppBridge::wake_idle` (46-67), `NoopDelivery` impl (77+), `drain_idle` (123-160) — consumption/emit move into the completion callback; in-flight set (e.g. `Mutex<HashSet<String>>` keyed by session) lives beside the bridge or in `drain_idle`'s state.
- **`src-tauri/src/mcp/messaging.rs` / `src-tauri/src/hooks/server.rs`**: call sites unchanged in shape (`drain_idle` signature keeps returning the pasted count; the DB write just happens later).
- **Risk**: MEDIUM — delivery is walk-hardened territory (paste/submit races documented in the spec); mitigations: timing unchanged, only the bookkeeping moves; in-flight guard scoped per session id.
- **Out of scope**: the `additionalContext` working-target path (no PTY involved); retry-with-backoff beyond the existing next-idle retry; sender-side UI for "pending vs delivered" states.
