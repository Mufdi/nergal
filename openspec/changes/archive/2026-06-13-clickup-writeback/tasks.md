# Tasks — clickup-writeback

> Depends on `clickup-sync` (mirror + client + poller) and `clickup-task-integration`
> (the binding that scopes the closure). No new migration (reuses existing tables);
> `recent_writes` is in-memory daemon state, and optimism lives in a frontend overlay
> (the durable mirror is never written before an API ack — Decision 1).

## 1. REST write methods

- [x] 1.1 Extend `clickup/client.rs`: `set_task_status(task_id, status_name)`, `add_comment(task_id, text)`, `set_checklist_item(item_id, resolved)`, `update_task(task_id, {description?, assignees_add?, assignees_rem?, due_date?})` (`PUT /task/{id}`; assignees as add/rem arrays, not replace), `set_custom_field(task_id, field_id, value)`. Reuse the auth + rate-limit machinery.
- [x] 1.2 `set_custom_field` serializes per the field `type` (from `clickup_custom_field_defs`): supported writable types = drop-down (option id), labels/multi-select (array), number, date (ms), url, text, checkbox. Reject computed (`automatic_progress`) and any unsupported type **at the command boundary**.
- [x] 1.3 Server-side validation at the command boundary: a status write checks the value is a real status of the task's List (`clickup_statuses`); computed fields rejected. Not UI-only.
- [x] 1.4 Tests: each method builds the correct request shape; computed-field + unsupported-type rejected pre-call; invalid-status rejected.
- [x] 1.5 **NEW read command** (iprev R1 finding #1 — `clickup_statuses` is written by the mirror but never read anywhere): `clickup_read_list_statuses(list_id)` — mirror read over `clickup_statuses WHERE list_id = ?` ordered by `orderindex`; register in `lib.rs`. Consumed by the §6.1 status picker AND the closure prompt; also the server-side status validation (1.3) reads the same table.

## 2. Optimistic overlay + recent-writes registry (no durable optimistic mutation)

- [x] 2.1 Optimism lives in a **frontend overlay atom** keyed by task+field; the durable mirror is updated only by the reconcile after an API ack (Decision 1). On API failure the overlay reverts + toast. The mirror is never written a value that was not server-acked.
- [x] 2.2 `clickup/writeback.rs`: on a successful write, record `(task_id, field, value, pre_write_value, at)` in an in-memory `recent_writes` map for echo/conflict comparison. Entries TTL out at ≥ ~2× the poll interval (derived from cadence).
- [x] 2.3 Tests: API failure reverts the overlay and never persists to the mirror; recent-writes TTL ≥ 2× poll.

## 3. Echo dedup + conflict resolution (value comparison)

- [x] 3.1 In the poller reconcile, before treating a task change as remote: for each `recent_writes` entry on that task, compare the **server's current field value** (from the fetched payload) to the written value → match ⇒ own echo, reconcile silently, clear the entry, do NOT notify. The echo check MUST run **before** the new-assignment detection (cross-change ordering).
- [x] 3.2 Conflict by field class (Decision 3): scalar fields (status/due/description/single-select) → last-writer-wins by `date_updated`, warn when remote supersedes a local edit. Additive fields (assignees/checklist/labels) → merge to server state, **no** false "superseded" warning.
- [x] 3.3 Tests: own write not re-notified (value match); scalar conflict remote-supersedes → warn; additive merge (remote+local both apply) → no warning; **regression test: own assignment-write does not self-notify (echo-before-assignment-detection ordering)**.

## 4. Comments — separate post-once model

- [x] 4.1 `add_comment`: no optimistic mirror insert; insert into the mirror only after `POST` returns the created comment id.
- [x] 4.2 Ambiguous failure (timeout/no response): do NOT auto-retry; mark "uncertain"; before any retry re-fetch task comments and check whether it landed (best-effort idempotency — ClickUp has no comment idempotency key); surface the uncertain state.
- [x] 4.3 Echo: match a fetched comment to a local pending one by author+text+timestamp so it isn't rendered as new.
- [x] 4.4 Tests: post→success inserts once; ambiguous failure does not duplicate on a re-fetch that finds it landed.

## 5. Structural write boundary + write-back-on-done closure

- [x] 5.1 Confirmation-token gate (Decision 5): the backend issues a short-lived token for a specific `(task, action, value)` only when the user confirms. The **closure and comment** writes execute solely via a command requiring that token — not directly-invokable per-write commands. A renderer cannot auto-fire them.
- [x] 5.2 Closure trigger **(design Revision 1 — read it first)**: (a) ship-success — `ShipDialog.tsx`'s `runCommitPushPr` success path holds `state.sessionId` + `ShipResult.pr_info` (`pr_info.url`); it does NOT hold binding state — resolve it by reading `workspacesAtom` + `clickupBindingMapAtom` and applying `resolveActiveClickUpTask` against the SHIPPED session's row (keyed by `state.sessionId`, never the active session; `findSession` in `stores/clickup.ts` is module-private → export a `resolveActiveClickUpTaskById` selector). Raise the offer before/alongside the success path's synchronous `close()`. One offer per successful ship; comment prefilled with `pr_info.url` (editable); push-only (`git_push` paths) does NOT offer. (b) Manual "Close out task" verb in the floating detail toolbar — a plain `ToolbarAction` button, **no single-letter contextual key** (S/W/P/B + A/C are taken); same prompt, no PR prefill. No hooks, no agent settings, no agent hardcoding.
- [x] 5.3 On confirm (token issued): perform ONLY the selected halves — status move and/or comment are independent, optional writes (status-only is the expected common case; an empty comment posts nothing; neither selected → confirm disabled). Report per-write outcome; the comment is never optimistically rolled back. If comment posts but status fails, surface "comment posted; status change failed — retry status?" (Decision 4 / Risks).
- [x] 5.4 Tests (Rust — the offer raise is frontend-only): no write without a valid token; token is single-use, short-lived, and scoped to its `(task, action, value)`; partial-failure result reports the irreversible half correctly.

## 6. Frontend write controls

- [x] 6.1 Floating detail write controls: status picker (from the task's List statuses), comment composer, checklist checkboxes, editable description / assignees / due date. Optimistic overlay with pending/failed states (Decision 1).
- [x] 6.2 Closure prompt UI: status picker (from the task's List statuses) + comment composer, BOTH optional (status-only must be the frictionless path; empty comment posts nothing; neither → confirm disabled). Prefill: user-authored by default; ship-triggered offers prefill the PR link, marked editable and **sanitized** (strip/escape `@` mentions + task-ref syntax) before posting; the user reviews before confirm.
- [x] 6.3 Toasts: echo silent; scalar conflict warns; additive merge silent.
- [x] 6.4 Commands registered in the invoke handler (`lib.rs`); the token-gated execution command is the sole entry for closure + comment writes.

## 7. Verification

- [x] 7.1 `cd src-tauri && cargo clippy -- -D warnings && cargo test && cargo fmt --check`
- [x] 7.2 `npx tsc --noEmit`
- [x] 7.3 Manual walk per proposal § Cómo verifico.
