# linear-writeback

## Why

`linear-mirror` reads Linear into Nergal; `linear-agent-integration` feeds issues into the agent and binds a session to its issue. The loop is still half-open: Nergal can read and act, but every edit (move a workflow state, leave a comment, (un)assign yourself) still requires switching to the Linear web app. This change closes the loop — bidirectional editing from Nergal, reflected in Linear — and the integration payoff: when the work of a session bound to an issue wraps (a successful ship, or the explicit "Close out issue" action), Nergal offers to move the state (picked from the issue's team workflow states) and optionally post a comment, so finishing in Nergal updates the issue without leaving the app.

This is the third and final change of the Linear staging, mirroring the archived `clickup-writeback` adapted to Linear's GraphQL API. Delivers value alone: edit Linear issues from Nergal (state, assignee, comments) reflected upstream, plus an explicit write-back-on-done closure for bound sessions.

## What Changes

- **Write surface**: change issue workflow state, post comments, and set/clear the assignee (assign-to-me / unassign). Description, priority, and labels are explicitly OUT of scope this change (deferred — see design).
- **Optimistic UI (overlay, not durable)**: an edit updates a frontend overlay for instant feedback but does NOT touch the durable mirror until the API acknowledges (so a crash can't leave the mirror holding an unsent value); on failure the overlay reverts + toast.
- **Echo dedup by value comparison, evaluated before notification**: a successful write returns on the next poll — match it by comparing the server's current field value to what we wrote (not by keying on the issue's coarse `updatedAt`). The comparison runs in the poller **run loop** (which holds the registry + app handle + fetched payloads), between the pure `reconcile` and the assignment notification. The own-echo filter removes an own "assign to me" from the new-assignment set **before** `notify_assignments` so it never self-fires `linear:assigned`.
- **Scalar conflict handling (server-wins + warn-on-divergence)**: state and assignee are both scalar single-value fields. Because optimism lives only in the frontend overlay, the durable mirror always blind-upserts server truth — there is no `updatedAt` comparison and no local-wins path. On a real remote supersede (server value ≠ what we wrote ≠ pre-write), a NEW `linear:write-conflict` event warns the user. Never silently clobber. (Linear has no additive/set fields in this change's write surface, so there is no merge path — a divergence from ClickUp's assignees/checklist.)
- **Comments are a separate post-once model**: not field-keyed, not optimistically rolled back (append-only); ambiguous failures re-check via a live comment re-fetch before any retry to avoid duplicate team-visible comments.
- **Structural write boundary**: the closure + comment writes execute only via a backend-issued confirmation token (not UI-only), so a renderer bug can't auto-fire writes against the full-access personal key.
- **Write-back closure**: for a session with an `active_linear_issue_id`, surface an **explicit** closure prompt — manually via "Close out issue" in the issue detail (the primary path), and automatically after a successful ship (PR created; comment prefilled with the PR link). Its core act is marking the issue done locally — unbind + a durable "worked & closed" marker independent of Linear's state, surfaced as a panel badge. Layered on top: two independent OPTIONAL halves — a state move (picked from the issue's team workflow states) and a comment (empty posts nothing; state-only is the common case). Never automatic writes — moving a Linear state is outward-facing and requires confirmation. No agent hooks, no user-settings writes, no agent hardcoding.

## Impact

- **Affected capabilities**: `linear-writeback` (ADDED). Builds on `linear-mirror` (mirror + GraphQL client + poller) and `linear-agent-integration` (the binding that scopes the closure).
- **Affected code**:
  - Rust: extend `linear/client.rs` with GraphQL mutations (`issueUpdate` for state/assignee, `commentCreate`); new `linear/writeback.rs` (in-memory recent-writes registry + echo/conflict comparison); new `linear/closure.rs` (confirmation-token store + token-gated execution + worked-and-closed marker writes); echo hook in `linear/poller.rs` reconcile (before the new-assignment set-diff); a NEW read command for a team's workflow states; migration `026_linear_closed_out.sql`; Tauri commands for each write + the token-gated closure.
  - React: write controls in the floating detail (state picker, comment composer, assign-to-me/unassign); the closure prompt UI (`LinearClosureDialog`); the closed-out panel badge; pending/conflict toasts; a `ShipDialog` Linear closure hook.
- **Depends on**: `linear-mirror` + `linear-agent-integration`. Independent of the context-bridge / MCP changes.

## Build contract

### Qué construyo

1. GraphQL mutations on `linear/client.rs` via the existing `execute::<T>` helper (auth + rate-limit reused): `issue_update(issue_id, {state_id?, assignee_id?})` (Linear `issueUpdate`), `comment_create(issue_id, body)` (Linear `commentCreate`, returns the created comment id). No new auth/rate-limit code.
2. `linear/writeback.rs`: an in-memory `WritebackRegistry` (`(issue_id, field) → {written_value, pre_write_value, at}`, TTL ≥ 2× poll) for echo/conflict comparison; optimism lives in a **frontend overlay** (the durable mirror is written only on ack — Decision 1). Fields = `State`, `Assignee` (both scalar).
3. Echo dedup + conflict in the poller reconcile by **value comparison** (echo check runs before the new-assignment set-diff); scalar LWW+warn for state/assignee. Comments handled by the separate post-once path.
4. Comments (separate path): post-once, insert into `linear_comments` only after `commentCreate` returns the id, ambiguous-failure re-fetch (live `issue_detail`) before any retry, echo-match by author+body+timestamp.
5. Structural boundary + closure: `linear/closure.rs` issues a short-lived confirmation token for a specific `(issue, state?, comment?)` on user confirm; the closure + comment writes execute only via the token-requiring command. Closure core = unbind + durable worked-and-closed marker (migration 026); optional independent state + comment halves. Trigger = ship-success (PR-link prefill, one per ship) + manual "Close out issue" verb (`KeyC`).
6. NEW read command `linear_read_team_states(team_id)` — mirror read over `linear_workflow_states WHERE team_id = ?` ordered by `position`; consumed by the detail state picker AND the closure prompt; the server-side state validation reads the same table.
7. Frontend: optimistic-overlay write controls in the floating detail; `LinearClosureDialog`; closed-out panel badge; echo-silent / scalar-conflict-warn toasts; `ShipDialog` Linear closure offer.

### Cómo verifico

- `cd src-tauri && cargo clippy -- -D warnings && cargo test && cargo fmt --check`
- `npx tsc --noEmit`
- Tests: each mutation builds the correct GraphQL request shape; invalid-state (not in the issue's team) rejected at the command boundary; optimistic overlay reverts on API failure and never persists to the mirror; echo dedup (our state/assignee write does not re-notify); **regression: own "assign to me" write does not self-fire `linear:assigned` (echo-before-set-diff ordering)**; scalar conflict (remote supersedes a pending write → LWW + warning); comment post-once + ambiguous-failure re-fetch; no closure/comment write without a valid backend token (single-use, scoped); partial failure reported honestly.
- Walk: from the floating detail change a state → see it in Linear; (un)assign yourself → see it; post a comment (routes through the token confirm) → see it; "Close out issue" on a bound session → unbind + badge + state-only closure lands in Linear without posting a comment; ship via the modal → offer appears with the PR link prefilled.

### Criterio de done

- State, assignee, and comment writes succeed against a real issue and reflect in Linear.
- A successful write does not produce a spurious "remote change" notification (echo deduped); an own "assign to me" never self-fires `linear:assigned`.
- A genuine remote conflict is resolved last-writer-wins and the user is warned, never silently clobbered.
- The write-back closure is offered only for bound sessions (ship-success or manual verb), always closes out locally (unbind + durable marker) even with neither half selected, executes only the selected optional halves, and writes to Linear only on explicit confirmation (never auto).
- No `unwrap()`/`expect()` outside tests; no TODO/FIXME; comments WHY-only.

### Estimated scope

- files_estimate: 13
- risk_tier: critical
- tags: [security, feature]
- visibility: private
- spec_target: linear-writeback
