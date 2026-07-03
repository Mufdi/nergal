## 1. Reorder records (mirror Linear)

- [ ] 1.1 `src-tauri/src/clickup/mod.rs` `clickup_set_task_status`: move `registry.record` (538) above `cl.set_task_status(...).await` (535); on `Err`, `registry.clear_entry(&task_id, &WriteField::Status)` before returning. Add Linear's "Provisional record BEFORE the API call" comment.
- [ ] 1.2 Same for `clickup_set_checklist_item` (record at 560).
- [ ] 1.3 `clickup_update_task`: move the three conditional records (602 description, 620 assignees, 628 due) above the single `cl.update_task(...).await`; on `Err`, clear exactly the entries that were recorded.
- [ ] 1.4 `clickup_set_custom_field` (record at 679): same reorder + clear-on-failure.
- [ ] 1.5 `src-tauri/src/clickup/closure.rs`: move the status record (313) above `client.set_task_status(...).await`; clear on the `Err` arm (outcome stays `StatusOutcome::Failed`).

## 2. Tests

- [ ] 2.1 Registry-level test: record → clear_entry → `recent_writes` shows no entry; record → (no clear) → entry present. Plus a test asserting each command's failure path clears what it recorded (mock/client-error injection if the command layer permits; otherwise unit-test the extracted record/clear helper).

## 3. Verification

- [ ] 3.1 `cd src-tauri && cargo clippy -- -D warnings && cargo test && cargo fmt --check`
- [ ] 3.2 Manual: with ClickUp connected, "assign to me" and immediately let a poll cycle run → no self-notification; force an API failure (bad token) → the failed write doesn't suppress the next real remote change.
