# Implementation — clickup-writeback

Detailed plan mapped against the current codebase. No code — guides Mode B.

## Codebase anchors (validated)

- **Client**: `clickup/client.rs` from `clickup-sync` — extend with write methods, reusing its auth header + `429`/`Retry-After` backoff.
- **Poller**: `clickup/poller.rs` reconcile from `clickup-sync` — the echo/conflict logic hooks in here. It hands the writeback layer the **fetched task payloads** (whole-task); the field-value comparison reads field values from those payloads (no per-field poller delta required — review #4).
- **SessionStatus**: enum at `models.rs:8` (`Idle`, `Running`, `NeedsAttention`, `Completed`); `lib.rs:702` already branches on `Completed`. The mode-map writer (the single status-writing path) is where the **edge** `prev!=Completed && new==Completed` is detected for the closure (review #7).
- **Binding**: `active_clickup_task_id` on `sessions` from `clickup-task-integration` — scopes the closure.
- **`date_updated`**: present on task payloads — used only as a coarse "this task changed" trigger, NOT the echo key (review #5).

## Migrations

**None.** No tables/columns added. `recent_writes` is in-memory daemon state; optimism lives in a frontend overlay atom. The durable mirror is never written a non-acked value (Decision 1), so no durable pending-write table is needed.

## Execution order

1. **Write methods** on `client.rs` (status/comment/checklist/update-task/custom-field) + type-correct custom-field serialization + **command-boundary validation** (computed-field reject, status-valid-for-list). Unit-test request shapes + rejections.
2. **`clickup/writeback.rs`**: the `recent_writes` map (value + pre-write value + TTL ≥ 2× poll). The optimistic mutation is **frontend-only** — the Rust side writes the mirror only on ack via reconcile. No durable optimistic write.
3. **Echo + conflict in the poller reconcile** (value comparison): echo check runs **before** new-assignment detection (cross-change ordering — pin with a regression test). Field-class resolution: scalar = LWW+warn, additive = merge-no-warn.
4. **Comments** (separate path): post-once, insert-after-id, ambiguous-failure re-fetch-before-retry, echo-match by author+text+timestamp.
5. **Structural boundary + closure**: backend issues a confirmation token for a specific `(task, action, value)` on user confirm; closure + comment writes execute only via the token-requiring command. Closure trigger = the Completed **edge**, offer-idempotent. On confirm, independent writes with per-write outcome.
6. **Frontend**: optimistic-overlay write controls (pending/failed states); closure prompt; echo-silent / scalar-conflict-warn / additive-silent toasts.

## Reuse, don't reinvent

- Auth + rate-limit: the `clickup-sync` client already has them.
- Notification/toast: reuse `toastsAtom` + the notification plugin.
- Status list for the closure picker: read `clickup_statuses` for the task's List (mirrored) — no live call.
- The token-gate pattern echoes the structural human gate iprev forced onto `agent-spawned-worktrees` (sole entry = one command requiring a backend-issued token), scoped here to the closure + comment writes.

## Edge cases

- **Echo ordering**: the value-comparison echo check MUST run before new-assignment detection (which lives in `clickup-sync`/`clickup-task-integration`), or an own assignment-write self-notifies. Regression test required.
- **Same-millisecond / two-field writes**: handled by value comparison per field, not `date_updated` ordering.
- **Partial closure**: comment posted + status failed → comment is NOT rolled back (append-only); surface "comment posted; status failed — retry?".
- **Assignee/checklist additive**: merge semantics, no false superseded-warning.
- **Comment idempotency**: no ClickUp key → ambiguous-failure path re-fetches comments before any retry; the closure's offer-idempotency prevents a double-post from a re-fired prompt.
- **Closure level vs edge**: `Completed` can be written repeatedly; only the not-Completed→Completed edge offers, once.
- **Comment prefill provenance**: user-authored default; agent-seeded text is marked editable and sanitized (strip/escape `@` mentions + task refs) and always user-reviewed before posting.
- **TTL vs poll**: `recent_writes` TTL ≥ ~2× poll interval so the echo poll arrives before expiry and a silently-failed write doesn't suppress a real remote change unbounded.

## Security note

Every write is outward-facing (visible to the user's team) against a full-access personal token. The closure + comment writes are structurally gated by a backend confirmation token (not UI-only); all writes validate inputs at the command boundary; rendering of untrusted ClickUp text is sanitized by `clickup-sync`. No write path is auto-triggered. Security reviewer focus at build: the token gate is genuinely un-bypassable, and echo-before-assignment ordering holds.
