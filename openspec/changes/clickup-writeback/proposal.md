# clickup-writeback

## Why

`clickup-sync` reads ClickUp into Nergal; `clickup-task-integration` feeds tasks into the agent and binds a session to its task. The loop is still half-open: Nergal can read and act, but every edit (move a status, leave a comment, check a checklist item) still requires switching to the ClickUp web app. This change closes the loop — full bidirectional editing from Nergal, reflected in ClickUp — and the integration payoff: when the work of a session bound to a task wraps (a successful ship, or the explicit "Close out task" action — design Revision 1: the originally-planned `Completed` transition does not exist at runtime), Nergal offers to move the status (picked from the task's List statuses) and optionally post a comment, so finishing in Nergal updates the task without leaving the app.

Delivers value alone: edit ClickUp tasks from Nergal (status, comments, checklists, fields) with the change reflected upstream, plus an explicit write-back-on-done closure for bound sessions.

## What Changes

- **Write surface** (the full set the user requested): change task status, post comments, toggle checklist items, edit description / assignees / due date.
- **Optimistic UI (overlay, not durable)**: an edit updates a frontend overlay for instant feedback but does NOT touch the durable mirror until the API acknowledges (so a crash can't leave the mirror holding an unsent value); on failure the overlay reverts + toast.
- **Echo dedup by value comparison**: a successful write returns on the next poll — match it by comparing the server's current field value to what we wrote (not by keying on the coarse per-task `date_updated`) so our own edit is not reported as a remote change.
- **Field-class conflict handling**: scalar fields (status/due/description) resolve last-writer-wins + warning; additive fields (assignees/checklist) merge without a false "superseded" warning. Never silently clobber.
- **Comments are a separate post-once model**: not field-keyed, not optimistically rolled back (append-only); ambiguous failures re-check before any retry to avoid duplicate team-visible comments.
- **Structural write boundary**: the closure + comment writes execute only via a backend-issued confirmation token (not UI-only), so a renderer bug can't auto-fire writes against the full-access token.
- **Write-back closure (design Revision 1)**: for a session with an `active_clickup_task_id`, surface an **explicit** closure prompt — manually via "Close out task" in the task detail (the primary path), and automatically after a successful ship (PR created; comment prefilled with the PR link). Two independent OPTIONAL halves: status move (picked from the task's List statuses) and comment (empty posts nothing; status-only is the common case). Never automatic writes — changing a ClickUp status is outward-facing and requires confirmation. No agent hooks, no user-settings writes, no agent hardcoding.

## Impact

- **Affected capabilities**: `clickup-writeback` (ADDED). Builds on `clickup-mirror` (mirror + client) and `clickup-agent-integration` (the binding that scopes the closure).
- **Affected code**:
  - Rust: extend `clickup/client.rs` with write methods (`PUT /task/{id}`, `POST /task/{id}/comment`, checklist item update, custom-field set, assignee/due edits); `clickup/writeback.rs` (optimistic apply + pending-write tracking + conflict resolve); closure offer raised from Nergal's own surfaces (ship-flow success + the detail's "Close out task" — design Revision 1); Tauri commands for each write + the token-gated closure.
  - React: write controls in the floating detail module (status picker, comment composer, checklist checkboxes, editable fields); the closure prompt UI; pending/conflict toasts.
- **Depends on**: `clickup-sync` + `clickup-task-integration`. Independent of the context-bridge changes.

## Build contract

### Qué construyo

1. Write methods on the REST client: status (`PUT /task/{id}` `{status}`), comment (`POST /task/{id}/comment`), checklist item toggle (`PUT /checklist_item/{id}` `{resolved}`), description/assignees/due (`PUT /task/{id}`), custom field set (`POST /task/{id}/field/{field_id}`) — excluding computed fields (`automatic_progress`).
2. `clickup/writeback.rs`: optimistic **frontend overlay** (durable mirror written only on ack) + an in-memory `recent_writes` map (value + pre-write value, TTL ≥ 2× poll) for echo/conflict comparison; on failure the overlay reverts + toast.
3. Echo dedup + conflict in the poller reconcile by **value comparison** (echo check before new-assignment detection); field-class resolution (scalar LWW+warn, additive merge). Comments handled by the separate post-once path.
4. Write-back closure (Revision 1): offer raised on ship-success (one per ship, PR-link prefill) and via the manual "Close out task" verb, for sessions with `active_clickup_task_id`; status and comment are independent optional halves; writes execute only via the confirmation-token boundary.
5. Frontend: write controls in the floating detail; closure prompt; pending/conflict toasts.

### Cómo verifico

- `cd src-tauri && cargo clippy -- -D warnings && cargo test && cargo fmt --check`
- `npx tsc --noEmit`
- Tests: optimistic apply + rollback on API failure; echo dedup (our write does not re-notify); conflict (remote change vs pending write → last-writer-wins + warning); no closure/comment write without a valid backend token (single-use, scoped); partial failure reported honestly.
- Walk: from the floating detail change a status → see it in ClickUp; post a comment → see it; toggle a checklist item; edit a due date; "Close out task" on a bound session → status-only closure lands in ClickUp without posting a comment; ship via the modal → offer appears with the PR link prefilled.

### Criterio de done

- All five write types succeed against a real task and reflect in ClickUp.
- A successful write does not produce a spurious "remote change" notification (echo deduped).
- A genuine remote conflict is resolved last-writer-wins and the user is warned, never silently clobbered.
- The write-back closure is offered only for bound sessions (ship-success or manual verb), executes only the selected optional halves, and writes only on explicit confirmation (never auto).
- No `unwrap()`/`expect()` outside tests; no TODO/FIXME; comments WHY-only.

### Estimated scope

- files_estimate: 12
- risk_tier: critical
- tags: [security, feature]
- visibility: private
- spec_target: clickup-writeback
