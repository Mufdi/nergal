# Implementation — linear-writeback

Detailed plan mapped against the current codebase. No code — guides Mode B.

## Codebase anchors (validated via Explore, 2026-06-18)

- **Client**: `linear/client.rs` — generic `async fn execute<T: DeserializeOwned>(&self, query, variables) -> Result<T>` (`client.rs:100`) already handles auth header + rate-limit (HTTP 400 / `RATELIMITED` / `X-RateLimit-*-Reset`, `client.rs:471`). **Zero mutations exist** — add `issue_update` + `comment_create` through `execute`. All current public methods are queries.
- **Active key (one-shot writes)**: `load_active_stored_key(&db) -> Result<auth::StoredKey, String>` (`mod.rs:857`) — same helper `linear_issue_detail`/`linear_fetch_image` use. No per-command org plumbing.
- **States source**: `linear_workflow_states` table (`023_linear_mirror.sql:41`, cols `id, team_id, name, type, color, position, synthetic`). **No "read states for a team" fn exists** — add `mirror::read_team_states(conn, team_id)` + command `linear_read_team_states`. Issue → team is a direct FK (`linear_issues.team_id`). Status picker reads the mirror; NO live-resolve step (Linear states are fully materialized per team — divergence from ClickUp's per-List inheritance).
- **Issue row**: `IssueView` (`mirror.rs:537`) carries `state_id`, `assignee_id`, `updated_at`, `team_id`. Assignee is **single scalar** (`assignee_id: Option<String>`).
- **Comments**: `linear_comments` table exists (`023_linear_mirror.sql:118`) but the poller never populates it. Detail fetches comments **live** via `client.issue_detail` (`client.rs:324`). Post-success: insert into `linear_comments`; ambiguous-failure re-check: live `issue_detail` re-fetch. Author id is a **String** UUID.
- **Poller reconcile**: `poller.rs:159 reconcile` does blind upserts; new-assignment is a **set-diff** `mine_before` vs `mine_now` (`poller.rs:262`) → `newly_assigned_ids` (`poller.rs:111`). Generation/epoch guard at `poller.rs:166`. The echo hook goes **before** the set-diff. Emit lives at `mod.rs:1033` (`notify_assignments` → `linear:assigned` at `mod.rs:1111`).
- **Binding**: `active_linear_issue_id` + `pinned_linear_issue_ids` columns on `sessions` (`024_linear_session_binding.sql`); DB fns `set_active_linear_issue` (`db.rs:851`), pins at `db.rs:859-884`. Unbind = `linear_unbind_issue` (`mod.rs:355`).
- **Viewer id (assign-to-me)**: `mirror::get_sync_state(conn)?.viewer_id: Option<String>` (`mirror.rs:336`).
- **Worked-closed marker pattern**: `019_clickup_closed_out.sql` (`task_id PK, closed_at`) + `clickup_mark_closed_out`/`clickup_read_closed_out` (`clickup/mod.rs:337/351`), read every mirror refresh into `clickupClosedOutAtom`. Mirror for Linear.

## Reference (mirror these files)

- `clickup/writeback.rs`: `WritebackRegistry { entries: Mutex<HashMap<(String,WriteField),WriteEntry>> }`, `WriteEntry { written_value, pre_write_value, at }`, `WriteField::field_class()`, `check_echo(entry, server_value) -> EchoCheckResult`, `WRITE_TTL = poll*2`, `record/entries_for_task/clear_entry/purge_expired`, `CommentOutcome` enum, `post_comment`, `verify_comment_landed`, `classify_comment_error`.
- `clickup/closure.rs`: `ClosureToken { task_id, status, comment, issued_at }`, `ClosureTokenStore { Mutex<HashMap<String,ClosureToken>> }` `issue()`/`take()`, `TOKEN_TTL=30s`, `ClosureResult { status: StatusOutcome, comment: CommentOutcome }`, `sanitize_comment_text`, 3 commands (request_token / execute_closure / verify_comment_landed).
- Frontend: `stores/clickup.ts` `clickupOverlayAtom`/`clickupClosureOfferAtom`/`resolveActiveClickUpTaskById` (`:318`), `ClickUpClosureDialog.tsx`, `ShipDialog.tsx:309` clickup offer hook, `LinearVerbToolbar`/`ToolbarAction` (`LinearTaskView.tsx:904/942`, writeback slot documented absent at `:939`), `VERB_KEYS` (`LinearTaskDetail.tsx:57`), `LinearConfirmDialogs.tsx` (`LinearSendConfirmDialog` pattern).

## Migrations

`026_linear_closed_out.sql`: `CREATE TABLE IF NOT EXISTS linear_closed_out (issue_id TEXT PRIMARY KEY, closed_at INTEGER NOT NULL);`. No other schema (recent-writes is in-memory; optimism is a frontend overlay; the durable mirror is never written a non-acked value — Decision 1). Register the migration in the migrations list.

## Execution order

1. **Mutations** on `client.rs` (`issue_update {stateId?, assigneeId?}` → `issueUpdate(id,input)` selecting `success issue{state{id} assignee{id}}`, `comment_create(issue_id, body)` → `commentCreate(input:{issueId,body})` selecting `success comment{id}`) via `execute::<T>`. **`success != true` ⇒ failure** (review #2). serde-rename to camelCase. Unit-test request shapes + the success-false→failure path. Direct write commands `linear_set_issue_state`/`linear_set_assignee` (the reversible half).
2. **`mirror::read_team_states`** (`WHERE team_id = ? AND synthetic = 0`) + command `linear_read_team_states` + server-side state-validation helper (state id ∈ team **non-synthetic** states — review #3). Unit-test invalid-state + synthetic-placeholder reject.
3. **`linear/writeback.rs`**: `WritebackRegistry` (fields `State`, `Assignee`, both scalar; TTL ≥ 2× poll). **Provisional record before the API call, clear on failure** (review #7). Optimism is frontend-only — Rust writes the mirror only on ack via reconcile.
4. **Echo + conflict in the RUN LOOP** (`mod.rs:1033`, after `run_cycle` returns, before `notify_assignments` — NOT inside the pure `reconcile`/`run_cycle`, reviews #6, N2): `run_cycle` drops the `FetchedCycle` and returns only `ReconcileOutcome` (no per-issue values), so read each registry entry's current `state_id`/`assignee_id` from the **post-reconcile mirror** (server truth via blind upsert) and `check_echo` against written/pre; filter own-echo issues out of `outcome.newly_assigned` before `notify_assignments` (the assignment-self-notify pin — regression test); on a divergent value emit a NEW `linear:write-conflict` event (server-wins + warn, NOT `updatedAt` LWW — review #5). Reach `WritebackRegistry` from the spawned poller task via `app.state::<WritebackRegistry>()` (review N3 — not `tauri::State`). Comments excluded from this path.
5. **Comments** (separate path in `writeback.rs`, reached ONLY via the token gate — review #1): post-once, insert into `linear_comments` after the id returns, ambiguous-failure re-fetch via live `issue_detail` before retry, echo-match by author+body+timestamp. No standalone `linear_post_comment` command. The detail composer uses `request_comment_token` (`close_out=false`) so posting a comment does NOT close the issue out (review N1).
6. **`linear/closure.rs`**: confirmation-token store (token carries `close_out: bool`) + token-gated `execute_gated_write` (state via `issue_update`, comment via the post-once path, independent halves; unbind + worked-closed marker **only when `close_out=true`**) + mark-closed/read-closed commands. TWO request commands: `request_comment_token` (`close_out=false`, comment only) and `request_closure_token` (`close_out=true`), both validating the state against the issue's non-synthetic team states and sanitizing the comment at issuance (reviews #1, N1).
7. **Frontend**: `linearOverlayAtom` write controls in the detail (state picker, comment composer, assign-to-me/unassign) with pending/failed states; `LinearClosureDialog` + `linearClosureOfferAtom`; closed-out panel badge (`linearClosedOutAtom`); `KeyC` close-out verb; `resolveActiveLinearIssueById` + `ShipDialog` Linear offer hook; echo-silent / scalar-conflict-warn toasts. Register all commands in `lib.rs`; the token-gated command is the sole entry for closure + comment writes; register `WritebackRegistry` + `ClosureTokenStore` as `tauri::State` via `manage()`.

## Edge cases

- **Echo ordering (corrected placement)**: the own-echo filter MUST run in the run loop on `outcome.newly_assigned` before `notify_assignments` (`mod.rs:1033`) — NOT inside `reconcile` (pure, no registry/AppHandle in scope). Or an own "assign to me" self-fires `linear:assigned`. Regression test required.
- **TOCTOU**: provisional registry record before the API call (cleared on failure) closes the window where a poll between write-land and command-resume mis-fires (review #7).
- **Closure order**: state write → comment post → local close-out (always). Retry surface for a failed state carries the `issue_id` explicitly (binding already unbound — review #8).
- **Partial closure**: comment posted + state failed → comment is NOT rolled back; surface "state failed — retry?". Local close-out (unbind+marker) still applied.
- **Comment idempotency**: no Linear key → ambiguous-failure path re-fetches comments (live detail) before any retry; the closure offer-idempotency (one per ship) prevents a double-post from a re-fired prompt.
- **TTL vs poll**: `recent_writes` TTL ≥ ~2× poll interval so the echo poll arrives before expiry.
- **Generation guard**: the writeback echo/conflict reconcile runs inside the same generation-guarded transaction (`poller.rs:166`) so an account swap mid-cycle discards cleanly.

## Security note

Every write is outward-facing (visible to the user's team) against a full-access personal key. The closure + comment writes are structurally gated by a backend confirmation token (not UI-only); state writes validate against the team's states at the command boundary; rendering of untrusted Linear text is sanitized by `linear-mirror`. No write path is auto-triggered. Security reviewer focus at build: the token gate is genuinely un-bypassable, and echo-before-assignment ordering holds.
