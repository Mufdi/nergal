## Why

Two small, high-confidence correctness bugs in the hook/plan path, both **silent** (no crash, wrong or lost data):

**A6 — cost upsert keyed by the wrong session id.** In the `HookEvent::Stop` handler, `db_guard.upsert_cost(session_id, &cost)` (`hooks/server.rs:910`) uses the raw hook-payload `session_id` (Claude Code's own internal id) instead of `nergal_session_id.unwrap_or(session_id)`. Every other write/emit in the same handler (6 sites) uses the Nergal id, which is what keys `sessions.id` throughout the DB. So cost rows land under CC's internal id; any lookup by Nergal session id finds nothing, and a resumed session (new CC-internal id per resume) fragments its cost history across orphaned rows. The error is also discarded (`let _ = ...`), hiding write failures.

**A7 — plan-edit save error discarded right before approval is sent.** In `submit_plan_decision` (`commands.rs:420`), `let _ = runtime.save_edits(plan.content.clone());` discards the `Result`. The sibling `save_plan` (`commands.rs:320-329`) propagates the identical call with `.map_err(...)?`. If the user edited a plan and the disk write then fails (permissions, disk full), the approval is still sent to the blocked CLI as approved; the CLI re-reads `plan.path` from disk and proceeds with the **stale, unedited** plan while the UI believes the edit was saved.

Grouped because both are one- to two-line correctness fixes on the same hook/plan surface, share the same reviewer lens, and neither warrants its own ceremony.

## What Changes

- **A6**: change `upsert_cost(session_id, &cost)` to key on `nergal_session_id.unwrap_or(session_id)`, matching the other 6 call sites in the same handler. Surface the write error instead of discarding it — at minimum `tracing::warn!` on failure (the handler is not `Result`-returning; do not silently swallow).
- **A7**: propagate the `save_edits` error with `?` (or map to the command's `Result<(), String>`), exactly as `save_plan` already does. If the edited plan fails to save, the approval is **not** sent and the user gets an error, so they never believe a lost edit was applied.

## Capabilities

### Modified Capabilities

- `session-summary`: the per-session cost record is keyed by the Nergal session id (the DB's session key), consistent with every other Stop-handler write, so cost is attributable and survives resume. *(If cost is owned by a different spec than `session-summary`, retarget at scaffold — the fix is unchanged.)*
- `plan-panel-multi-agent`: submitting a plan decision after editing the plan SHALL fail the submission if the edit cannot be persisted, rather than approving with a silently-unsaved edit.

## Impact

- **`src-tauri/src/hooks/server.rs`**: `HookEvent::Stop` cost upsert keyed by `nergal_session_id.unwrap_or(session_id)`; write error logged, not discarded.
- **`src-tauri/src/commands.rs`**: `submit_plan_decision` propagates `save_edits` failure with `?` before the decision is written.
- **Tests**: A6 — a Stop-event test asserting the cost row is stored under the Nergal id and readable by it; A7 — a `submit_plan_decision` test where `save_edits` fails asserts the decision is not sent and an error is returned.
- **Risk**: LOW change size, MED value (silent data-integrity + silent edit loss).
- **Out of scope**: the broader "discarded `Result` on critical paths" sweep across the backend (a Band-C/D concern) — this change fixes these two confirmed instances only.
