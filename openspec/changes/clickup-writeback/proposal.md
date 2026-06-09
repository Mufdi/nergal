# clickup-writeback

## Why

`clickup-sync` reads ClickUp into Nergal; `clickup-task-integration` feeds tasks into the agent and binds a session to its task. The loop is still half-open: Nergal can read and act, but every edit (move a status, leave a comment, check a checklist item) still requires switching to the ClickUp web app. This change closes the loop — full bidirectional editing from Nergal, reflected in ClickUp — and the integration payoff: when a session bound to a task wraps, Nergal offers to move the status and post a comment summarizing the work, so finishing in Nergal updates the task without leaving the app.

Delivers value alone: edit ClickUp tasks from Nergal (status, comments, checklists, fields) with the change reflected upstream, plus an explicit write-back-on-done closure for bound sessions.

## What Changes

- **Write surface** (the full set the user requested): change task status, post comments, toggle checklist items, edit description / assignees / due date.
- **Optimistic UI (overlay, not durable)**: an edit updates a frontend overlay for instant feedback but does NOT touch the durable mirror until the API acknowledges (so a crash can't leave the mirror holding an unsent value); on failure the overlay reverts + toast.
- **Echo dedup by value comparison**: a successful write returns on the next poll — match it by comparing the server's current field value to what we wrote (not by keying on the coarse per-task `date_updated`) so our own edit is not reported as a remote change.
- **Field-class conflict handling**: scalar fields (status/due/description) resolve last-writer-wins + warning; additive fields (assignees/checklist) merge without a false "superseded" warning. Never silently clobber.
- **Comments are a separate post-once model**: not field-keyed, not optimistically rolled back (append-only); ambiguous failures re-check before any retry to avoid duplicate team-visible comments.
- **Structural write boundary**: the closure + comment writes execute only via a backend-issued confirmation token (not UI-only), so a renderer bug can't auto-fire writes against the full-access token.
- **Write-back-on-done closure**: when a session with an `active_clickup_task_id` transitions to `Completed`, surface an **explicit** affordance to move the task's status and post a comment (e.g. the PR link / a work summary). Never automatic — changing a ClickUp status is an outward-facing write and requires confirmation.

## Impact

- **Affected capabilities**: `clickup-writeback` (ADDED). Builds on `clickup-mirror` (mirror + client) and `clickup-agent-integration` (the binding that scopes the closure).
- **Affected code**:
  - Rust: extend `clickup/client.rs` with write methods (`PUT /task/{id}`, `POST /task/{id}/comment`, checklist item update, custom-field set, assignee/due edits); `clickup/writeback.rs` (optimistic apply + pending-write tracking + conflict resolve); hook the `SessionStatus → Completed` transition (the mode-map writer, `models.rs:8`) for the closure offer; Tauri commands for each write + the closure.
  - React: write controls in the floating detail module (status picker, comment composer, checklist checkboxes, editable fields); the closure prompt UI; pending/conflict toasts.
- **Depends on**: `clickup-sync` + `clickup-task-integration`. Independent of the context-bridge changes.

## Build contract

### Qué construyo

1. Write methods on the REST client: status (`PUT /task/{id}` `{status}`), comment (`POST /task/{id}/comment`), checklist item toggle (`PUT /checklist_item/{id}` `{resolved}`), description/assignees/due (`PUT /task/{id}`), custom field set (`POST /task/{id}/field/{field_id}`) — excluding computed fields (`automatic_progress`).
2. `clickup/writeback.rs`: optimistic **frontend overlay** (durable mirror written only on ack) + an in-memory `recent_writes` map (value + pre-write value, TTL ≥ 2× poll) for echo/conflict comparison; on failure the overlay reverts + toast.
3. Echo dedup + conflict in the poller reconcile by **value comparison** (echo check before new-assignment detection); field-class resolution (scalar LWW+warn, additive merge). Comments handled by the separate post-once path.
4. Write-back-on-done: observe the **edge** `prev!=Completed && new==Completed` for a session with `active_clickup_task_id`; surface an explicit, offer-idempotent closure prompt; writes execute only via the confirmation-token boundary.
5. Frontend: write controls in the floating detail; closure prompt; pending/conflict toasts.

### Cómo verifico

- `cd src-tauri && cargo clippy -- -D warnings && cargo test && cargo fmt --check`
- `npx tsc --noEmit`
- Tests: optimistic apply + rollback on API failure; echo dedup (our write does not re-notify); conflict (remote change vs pending write → last-writer-wins + warning); closure offered only for a bound session reaching `Completed`, and only on explicit confirm does a write occur.
- Walk: from the floating detail change a status → see it in ClickUp; post a comment → see it; toggle a checklist item; edit a due date; let a bound session finish → confirm the closure prompt → status + comment land in ClickUp.

### Criterio de done

- All five write types succeed against a real task and reflect in ClickUp.
- A successful write does not produce a spurious "remote change" notification (echo deduped).
- A genuine remote conflict is resolved last-writer-wins and the user is warned, never silently clobbered.
- The write-back-on-done closure is offered only for bound sessions reaching `Completed`, and performs writes only on explicit confirmation (never auto).
- No `unwrap()`/`expect()` outside tests; no TODO/FIXME; comments WHY-only.

### Estimated scope

- files_estimate: 12
- risk_tier: critical
- tags: [security, feature]
- visibility: private
- spec_target: clickup-writeback
