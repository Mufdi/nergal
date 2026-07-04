## 1. Trait + bridge

- [x] 1.1 `src-tauri/src/mcp/delivery.rs` — extend `SessionDelivery::wake_idle` with `on_settled: Box<dyn FnOnce(bool) + Send + 'static>` (design D1). `AppBridge` invokes it in the spawned settle task with the real `submit_to_session` outcome; wrap the task body so the callback fires even on early bail (design Risks: drop-guard).
- [x] 1.2 `NoopDelivery`: keep returning `Err` (unchanged short-circuit; callback never fires on the `Err` path — design D4).

## 2. drain_idle bookkeeping

- [x] 2.1 Move mark-consumed (`mark_cross_session_agent_consumed`) + `crossmsg:agent-consumed` emit (delivery.rs:149-160) into the `on_settled(true)` callback (owned ids + `SharedDb` clone + emit handle).
- [x] 2.2 In-flight guard (design D2): module-level `Mutex<HashSet<String>>` of session ids; check-and-insert at `drain_idle` entry (skip if present), remove inside `on_settled` before any DB write.

## 3. Tests

- [x] 3.1 Extend the existing delivery tests with a mock `SessionDelivery` capturing the callback: success → consumed set once, emit fired; failure → nothing consumed, next `drain_idle` retries the same ids; concurrent second drain during in-flight → 0 pasted.

## 4. Verification

- [x] 4.1 `cd src-tauri && cargo clippy -- -D warnings && cargo test && cargo fmt --check`
- [ ] 4.2 Manual walk: send a message to an idle session → note pastes, submits, message marked consumed; send to an idle session and close its tab within the settle window → message stays pending and delivers on the session's next idle (reopen).
