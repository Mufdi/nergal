# Design — clickup-writeback

## Context

The mirror (`clickup-sync`) is a durable local copy kept fresh by a poller; the binding (`clickup-task-integration`) ties a session to its task. This change adds the write direction. The hard parts are not the API calls — they are (a) keeping optimistic UI, the durable mirror, and polling from corrupting each other, (b) the fact that comments are a fundamentally different write than fields, and (c) making outward-facing writes against a full-access personal token structurally safe, not UI-guarded.

## Decision 1 — Optimistic UI is a frontend overlay; the durable mirror holds only server-acked truth

**Decision** (resolves review #1 split-brain): an edit updates an **in-memory frontend overlay** (a pending-edit atom) for instant UI, but **does NOT mutate the durable SQLite mirror until the API acknowledges**. The mirror always holds server truth. On API success, the mirror is updated by the normal reconcile and the overlay clears; on failure, the overlay reverts + toast.

**Why**: if the optimistic value were written to the durable mirror before the API ack, a daemon crash in that window would leave the mirror holding a value never sent upstream, and the next poll would silently overwrite it with the stale server value — violating "never silently clobber" with no registry left to detect it. Keeping optimism in the volatile UI layer means a crash loses at most an un-acked edit (the user re-does it) and the durable mirror is never wrong.

**Alternatives considered**:
- *Optimistic write to the durable mirror + in-memory rollback registry*: the split-brain above. Rejected.
- *Durable `pending_writes` table + dirty flag on mirror rows*: correct but adds a migration and crash-reconcile logic for a single-user tool; the frontend-overlay model achieves the same safety more simply. Rejected as over-built for MVP.
- *Write-through (no optimism)*: laggy UX. Rejected.

## Decision 2 — Echo dedup and conflict by value comparison, not `date_updated` keying

**Decision** (resolves review #4, #5): `date_updated` is a millisecond timestamp on the **task**, not per-field, not predictable pre-call, and can collide for two same-millisecond edits — so it is used only as a coarse "this task changed, re-examine it" trigger, never as the echo key or per-field order. Echo/conflict are decided by **value comparison against the fetched task payload** (which carries all current field values):

- After a successful write of `(task, field, value)`, record it in an in-memory `recent_writes` map.
- On the next poll, for a task that changed, compare the **server's current value** of that field to: (a) our written `value` → match ⇒ it's our **echo**, reconcile silently, no notification; (b) neither our `value` nor the pre-write value ⇒ someone else changed it ⇒ **conflict** (Decision 3).

This needs no per-field delta from `clickup-sync`'s poller (which diffs at task granularity) — the comparison reads field values straight from the fetched payload. The cross-change dependency is therefore just "the poller hands the writeback layer the fetched task payloads," a verified precondition, not an assumed per-field diff capability.

## Decision 3 — Field-class-specific conflict resolution

**Decision** (resolves review #6): resolution depends on the field's nature:

- **Scalar fields** (status, due date, description, single-select custom fields): last-writer-wins by `date_updated`; if the remote value supersedes a just-applied local edit, overwrite the mirror and **warn** (toast naming the field). Never silently clobber.
- **Additive/set fields** (assignees, checklist item resolution, labels/multi-select): **merge**, no LWW-with-warning. If remote adds user X and local adds user Y, ClickUp merges both server-side; treating that as "your edit superseded" is a false alarm that erodes trust in the warning. The mirror reconciles to the merged server state without a conflict warning.

## Decision 4 — Comments are a separate post-once model, not part of the optimistic/rollback pipeline

**Decision** (resolves review #2, #9): a comment is not a task field — it lives on a separate resource, posting it generally does **not** bump the task's `date_updated`, it is **append-only (cannot be rolled back)**, and a timeout-but-succeeded `POST` retried (or the closure firing twice) posts a **duplicate team-visible comment**. So comments get their own path:

- **No optimistic mirror insert**: the comment is inserted into the mirror only after the `POST` returns the created comment id.
- **Post-once with ambiguity handling**: on a clear success, store it. On an ambiguous failure (timeout, no response), **do not auto-retry**; mark the send "uncertain" and, before any retry, re-fetch the task's comments and check whether it landed (best-effort idempotency, since ClickUp has no comment idempotency key) — surface the uncertain state to the user rather than risk a duplicate.
- **Echo**: a posted comment appears in the next comment fetch; match it to the local pending comment by author+text+timestamp to avoid re-rendering it as "new."

## Decision 5 — Structural write boundary: confirm → token → execute, plus server-side validation

**Decision** (resolves review #3 — the security gap): the safety of an outward-facing write against a full-access personal token SHALL NOT rest on a React-side confirmation. Per-write Tauri commands registered in the invoke handler are directly callable from the webview, so a renderer bug or injected content could invoke them.

- **The closure and comment writes** (auto-fireable / irreversible / team-visible — the sharp edges) are **structurally gated**: the sole execution entry is a command that requires a short-lived **confirmation token** issued by the backend only when the user confirms the prompt for a specific `(task, action, value)`. A renderer cannot fabricate a valid token without going through the confirm step, so it cannot auto-fire the closure or post a comment by calling a command.
- **All write commands validate inputs server-side**: computed custom fields (`automatic_progress`) rejected at the command boundary (not just hidden in UI); a status write checks the value is a real status of the task's List (`clickup_statuses`); custom-field writes type-check (Decision 7). This is defense-in-depth atop the sanitized rendering `clickup-sync` already mandates (which closes the XSS vector that would let untrusted content drive the webview).

**Alternatives considered**:
- *UI-only confirmation* (the round-1 design): rejected — exactly the gap iprev forced closed on `agent-spawned-worktrees`.
- *Token handshake on every field edit*: rejected as disproportionate — routine user-initiated status/checklist edits are reversible and low-blast-radius; the token gate is reserved for the auto-fireable closure and irreversible comments, with server-side validation covering the rest.

## Decision 6 — Write-back-on-done: edge-triggered, idempotent, explicit

**Decision** (resolves review #7): the closure observes the **edge** `prev != Completed && new == Completed` (not the level — `SessionStatus` can be written `Completed` repeatedly; `lib.rs:702` already branches on `Completed`). The offer is idempotent per completion (one prompt per transition, not re-fired on every `Completed` write). The prompt is explicit; writes occur only on confirmation (via Decision 5's token); the status target is chosen from the task's List statuses (`clickup_statuses`), never hardcoded. Offered only for sessions with an `active_clickup_task_id`.

## Decision 7 — Type-correct custom-field writes

**Decision** (resolves review #11): writable custom fields span serialization shapes (drop-down option UUID, labels array, number, date ms, url, text…). `set_custom_field` SHALL serialize per the field's `type` (from `clickup_custom_field_defs`), and SHALL reject computed types (`automatic_progress`) and any type not in the supported writable set (rather than sending an untyped value that 400s or silently no-ops). The supported writable types are enumerated in tasks.md.

## Risks

- **[Risk] Partial-closure irreversibility** (review #9): status + comment are independent writes; if the comment posts but the status write fails, the comment cannot be un-posted. Do NOT present per-write rollback as clean — surface "comment posted; status change failed — retry status?" and let the user reconcile. The comment (Decision 4) is never optimistically rolled back.
- **[Risk] Pending-write / recent-write TTL** (review #8): entries expire ≥ ~2× the poll interval (derived from the cadence, not a magic constant) so the echo poll always arrives before expiry, and a silently-failed write's stale entry never suppresses a real remote change for an unbounded window.
- **[Risk] Reconcile ordering across changes** (review #10): the echo check MUST run before `clickup-task-integration`/`clickup-sync`'s new-assignment detection so an own write never self-notifies. Pin this with a regression test (own assignment-write does not self-notify), since the assignment-detection code lives in another change and a future edit could silently break the order.
- **[Risk] Closure comment prefill provenance** (review #12): the prefill source is defined — user-authored by default; if seeded from agent output / transcript it is clearly marked editable and **sanitized** before posting (strip/escape ClickUp mention `@` and task-reference syntax so an injected summary can't ping the team or link arbitrarily). The user always reviews it (Decision 5 confirm) before it posts.
- **[Risk] Computed-field write attempt** → rejected at the command boundary (Decision 5), not just in UI.
- **[Risk, benign] `recent_writes` crash-loss** → if the daemon crashes after a successful write but before the echo poll, the in-memory entry is gone and the next poll attributes the user's own edit to a remote change → one spurious "remote change" toast. No data is wrong (the value is what the user wanted). Benign one-shot; does not justify a durable table. Noted so a tester doesn't file it as a bug.
- **[Risk, accepted] Value-equality echo masks a coincidental same-value remote write** → if a remote independently sets the same value and the user's write actually failed, the failure is masked. Both states reconcile to the user-intended value; low blast radius for a single-user tool. Accepted tradeoff of value-comparison echo.
- **[Risk, accepted] Routine reversible writes are not token-gated** → only the closure + comments carry the confirmation token; routine status/checklist/field edits rely on command-boundary validation + sanitized rendering. A renderer bug could fire a *reversible* change against the full-access token — a conscious boundary (token-on-every-edit rejected as disproportionate), flagged for the build-time security reviewer as "reversible ≠ harmless against a full-access token."
- **[Risk, cosmetic] Comment echo-match collapse** → two genuinely-identical user comments (same author+text+~timestamp) could dedup one away in the mirror render; rare, recoverable on the next full fetch.
