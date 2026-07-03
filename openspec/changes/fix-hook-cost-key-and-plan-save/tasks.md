## 1. A6 — cost upsert session key

- [ ] 1.1 `hooks/server.rs` `HookEvent::Stop` (line ~910) — change `upsert_cost(session_id, &cost)` to key on `nergal_session_id.unwrap_or(session_id)`, matching the other 6 writes in the handler.
- [ ] 1.2 Replace the discarded `let _ = db_guard.upsert_cost(...)` with a `tracing::warn!` on `Err` (handler is not `Result`-returning; do not swallow silently).

## 2. A7 — plan-edit save error

- [ ] 2.1 `commands.rs` `submit_plan_decision` (line ~420) — replace `let _ = runtime.save_edits(plan.content.clone());` with error propagation (`.map_err(|e| e.to_string())?`), mirroring `save_plan` (`commands.rs:320-329`), so a failed save aborts the decision send.

## 3. Tests

- [ ] 3.1 A6: Stop-event test — assert the cost row is stored under the Nergal session id and is readable by it (not by the CC-internal id).
- [ ] 3.2 A7: `submit_plan_decision` test where `save_edits` returns `Err` — assert the decision is not written to the FIFO/pipe and the command returns an error.

## 4. Verification

- [ ] 4.1 `cd src-tauri && cargo clippy -- -D warnings && cargo test && cargo fmt --check`.
- [ ] 4.2 Manual: edit a plan then approve → confirm the CLI receives the edited content; simulate a save failure (e.g. read-only plan dir) → confirm the approval is blocked with an error toast, not silently approved.
