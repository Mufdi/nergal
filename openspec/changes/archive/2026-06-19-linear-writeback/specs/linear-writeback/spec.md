# linear-writeback

## ADDED Requirements

### Requirement: Bidirectional issue editing with server-side validation

The system SHALL let the user edit a Linear issue from Nergal — change its workflow state, post comments, and set or clear the assignee — and reflect each change in Linear via the GraphQL API (`issueUpdate` for state and assignee, `commentCreate` for comments). Each mutation SHALL select the response `success` boolean and SHALL treat `success != true` (even with no GraphQL `errors`) as a write failure, never a silent no-op. The assignee surface SHALL be set-to-me and clear (Linear assignee is a single scalar, not a set); a full team-member picker is deferred (the mirror holds no complete member directory). An assign-to-me SHALL error when the viewer id is unresolved rather than fall through to clearing the assignee. State writes SHALL validate the target state belongs to the issue's team workflow states at the command boundary, not only in the UI. State options SHALL be the issue's **non-synthetic** team workflow states (per-team, never hardcoded), read from the mirror (`linear_workflow_states WHERE team_id = ? AND synthetic = 0`) — synthetic placeholder rows SHALL be excluded from both the picker and the validation source; no live resolution step is needed because Linear states are fully materialized per team (a divergence from ClickUp's per-List inheritance). Comment writes (unlike the direct state/assignee edits) SHALL route through the confirmation-token boundary, not a standalone comment command. Description, priority, and label writes are out of scope for this change.

#### Scenario: State change reflects upstream after validation

- **WHEN** the user changes an issue's state to one of its team's workflow states
- **THEN** the system SHALL validate the state belongs to the issue's team
- **AND** call `issueUpdate` to set it
- **AND** the change SHALL be visible in Linear

#### Scenario: Invalid state rejected at the boundary

- **WHEN** a write targets a state id that is not in the issue's team workflow states
- **THEN** the system SHALL reject it at the command boundary, not only in the UI

#### Scenario: Assignee set-to-me and clear

- **WHEN** the user assigns the issue to themselves or clears the assignee
- **THEN** the system SHALL call `issueUpdate` with the viewer id or null respectively
- **AND** the change SHALL be visible in Linear

#### Scenario: Assign-to-me with no resolved viewer id errors

- **WHEN** the user invokes assign-to-me and the viewer id is unresolved
- **THEN** the system SHALL return an error
- **AND** SHALL NOT call `issueUpdate` with a null assignee

#### Scenario: Mutation reporting success=false is a failure

- **WHEN** a mutation returns `success: false` with no GraphQL errors
- **THEN** the system SHALL treat it as a write failure
- **AND** the frontend overlay SHALL revert

### Requirement: Cycle assignment write-back

The system SHALL let the user add an issue to a cycle, move it between cycles, or remove it from its cycle, reflected in Linear via `issueUpdate` (`cycleId`). The cycle picker SHALL list the team's cycles read from the mirror (`linear_cycles WHERE team_id = ?`) plus a "no cycle" option. Removal SHALL send `cycleId: null` (a `Some(None)` double-option so a state/assignee-only write never accidentally clears the cycle). Cycle writes SHALL reuse the same optimistic-overlay, provisional-registry, and echo-dedup machinery as assignee (a `Cycle` write-field), and SHALL NOT be token-gated (reversible, like state and assignee).

#### Scenario: Add to or move between cycles

- **WHEN** the user selects a cycle for the issue
- **THEN** the system SHALL call `issueUpdate` with `cycleId` set to the chosen cycle id
- **AND** the change SHALL be visible in Linear

#### Scenario: Remove from cycle

- **WHEN** the user selects "no cycle"
- **THEN** the system SHALL call `issueUpdate` with `cycleId: null`
- **AND** the issue SHALL no longer belong to a cycle in Linear

### Requirement: Optimistic UI without durable mirror corruption

The system SHALL apply an edit optimistically in a frontend overlay for instant feedback, but SHALL NOT write the optimistic value to the durable mirror until the API acknowledges. On success the mirror SHALL be updated by reconcile and the overlay cleared; on failure the overlay SHALL revert and the user SHALL be notified. A crash before acknowledgement SHALL at worst lose an un-acked edit and SHALL NOT leave the durable mirror holding a value that was never sent upstream.

#### Scenario: Un-acked edit never corrupts the mirror

- **WHEN** an optimistic edit is made and the daemon crashes before the API call resolves
- **THEN** the durable mirror SHALL NOT hold the optimistic value
- **AND** the next poll SHALL NOT silently overwrite a persisted-but-unsent edit

#### Scenario: Failed write reverts the overlay

- **WHEN** an optimistic edit's API call fails
- **THEN** the frontend overlay SHALL revert to the pre-edit value
- **AND** the user SHALL be notified

### Requirement: Echo dedup and scalar conflict by value comparison, evaluated before notification

The system SHALL decide echo and conflict by comparing field values, not by keying on the issue's `updatedAt` (which is coarse, per-issue, and not predictable pre-call). The system SHALL record a write provisionally before its API call and clear the record on API failure (closing the window where a concurrent poll sees the new server value with no record). The echo/conflict evaluation SHALL run in the poller run loop — which holds the registry and the app handle — between the pure `reconcile` and the assignment notification, NOT inside `reconcile` itself. Because the run-loop wrapper drops the fetched payload and returns only a reconcile outcome with no per-issue field values, the comparison SHALL read each written field's current value from the **post-reconcile mirror** (which holds server truth, since reconcile blind-upserts the fetched payload); an issue that fell out of the poll window (no mirror row) SHALL be skipped. Because optimism lives only in the frontend overlay, the durable mirror always blind-upserts server truth; there is no `updatedAt` comparison and no path where the local value wins. The resolution is therefore: the server value lands in the mirror, and the system warns (emitting a `linear:write-conflict` event with the issue, field, your value, and the remote value) IFF the server value diverges from what the user just wrote. An own echo (server equals the written value) SHALL be suppressed and its record cleared. For the assignee field specifically, the own-echo filter SHALL run before the new-assignment set-diff is turned into a notification, so an own "assign to me" never self-fires `linear:assigned`. The system SHALL never silently clobber a divergence.

#### Scenario: Own assign-to-me does not self-fire the assignment notification

- **WHEN** the user assigns an issue to themselves from Nergal and the next poll observes it
- **THEN** the own-echo filter SHALL remove that issue from the new-assignment set before notification
- **AND** the system SHALL NOT emit `linear:assigned` for that own write

#### Scenario: Own state write is not flagged as a conflict

- **WHEN** a poll observes an issue whose current state equals a recent local state write
- **THEN** the system SHALL clear the record and SHALL NOT emit `linear:write-conflict`

#### Scenario: Scalar conflict warns on divergence

- **WHEN** a remote change sets a scalar field (state or assignee) to a value that is neither the user's recent write nor its pre-write value
- **THEN** the server value SHALL land in the mirror via the blind upsert
- **AND** the system SHALL emit `linear:write-conflict` naming the field, the user's value, and the remote value

### Requirement: Comments are posted once, never optimistically rolled back

Because a comment is a separate append-only resource that cannot be un-posted and Linear has no comment idempotency key, the system SHALL NOT treat comments like field writes. A comment SHALL be inserted into the mirror (`linear_comments`) only after `commentCreate` returns the created comment id. On an ambiguous failure the system SHALL NOT auto-retry; it SHALL mark the send uncertain and, before any retry, re-fetch the issue's comments (live `issue_detail`) to check whether it landed, to avoid posting a duplicate team-visible comment.

#### Scenario: Comment posted once

- **WHEN** the user posts a comment
- **THEN** it SHALL be inserted into the mirror only after `commentCreate` returns its id

#### Scenario: Ambiguous failure does not duplicate

- **WHEN** a comment post times out with no response
- **THEN** the system SHALL NOT auto-retry blindly
- **AND** SHALL re-fetch the issue's comments to check whether it landed before any retry

### Requirement: Structural confirmation boundary for outward-facing writes

The safety of outward-facing writes against a full-access personal key SHALL NOT rest on a frontend-only confirmation. The closure and comment writes SHALL execute solely via a command that requires a short-lived confirmation token issued by the backend only when the user confirms a specific action, so a renderer bug or injected content cannot auto-fire them by calling a command directly.

#### Scenario: Closure write requires a backend token

- **WHEN** code attempts the closure or comment write without a valid confirmation token
- **THEN** the system SHALL refuse the write

#### Scenario: Posting a comment does not close the issue out

- **WHEN** the user posts a comment from the issue detail composer (a comment-only token, close_out=false)
- **THEN** the system SHALL post the comment
- **AND** SHALL NOT unbind the issue from the session
- **AND** SHALL NOT write the worked-and-closed marker

#### Scenario: Token issued only on user confirmation

- **WHEN** the user confirms a specific closure action
- **THEN** the backend SHALL issue a short-lived token scoped to that action
- **AND** the execution command SHALL accept only that token

### Requirement: Ship-anchored and manual write-back closure

The system SHALL offer the write-back closure at two agent-agnostic moments: on demand via an explicit "Close out issue" action in the issue detail (the primary path), and automatically after a successful ship (PR created via Nergal's own ship flow) of a session that has an `active_linear_issue_id`, with the comment prefilled (editable) with the PR link. Close-out's core act SHALL be marking the issue done locally — it SHALL unbind the issue from the session and persist a local "worked & closed" marker that is independent of the Linear state (the issue keeps whatever state Linear holds) and SHALL surface as a panel badge. Layered on top, the prompt SHALL offer two independent, OPTIONAL halves: a state move chosen manually from the issue's team workflow states (per-team, never hardcoded) and a comment; confirm SHALL execute only the selected halves — a state-only closure SHALL be the frictionless path and an empty comment SHALL post nothing. Confirm SHALL always be armed: confirming with neither half selected SHALL still close the issue out locally (unbind + marker) without any Linear write. One offer per successful ship; a push without a PR SHALL NOT trigger the offer. Writes SHALL occur only on explicit confirmation (via the token boundary). The trigger SHALL NOT rely on agent-specific hooks, SHALL NOT write to any agent's user settings, and SHALL NOT hardcode a specific agent.

#### Scenario: Manual closure is the primary path

- **WHEN** the user invokes "Close out issue" from the issue detail
- **THEN** the system SHALL surface the closure prompt without a PR prefill
- **AND** the state picker SHALL list the workflow states of that issue's team

#### Scenario: State-only closure posts no comment

- **WHEN** the user confirms a closure with a state selected and the comment empty
- **THEN** the system SHALL move the state
- **AND** SHALL NOT post any comment

#### Scenario: Close-out with no halves still marks the issue done

- **WHEN** the user confirms a closure with neither a state nor a comment selected
- **THEN** the system SHALL NOT write anything to Linear
- **AND** SHALL unbind the issue from the session
- **AND** SHALL persist the local "worked & closed" marker

#### Scenario: Worked-and-closed marker is independent of Linear state

- **WHEN** an issue has been closed out from a session
- **THEN** the panel SHALL flag it with a worked-and-closed badge
- **AND** the badge SHALL persist across restarts regardless of the issue's Linear state

#### Scenario: Closure offered after a successful ship of a bound session

- **WHEN** a ship completes successfully (PR created) for a session with an `active_linear_issue_id`
- **THEN** the system SHALL surface the closure prompt once for that ship
- **AND** the comment prefill SHALL include the PR link and SHALL remain user-editable

#### Scenario: No write without confirmation

- **WHEN** the closure prompt is dismissed
- **THEN** the system SHALL NOT write anything to Linear

#### Scenario: Unbound session offers no closure

- **WHEN** a ship completes for a session without an `active_linear_issue_id`
- **THEN** the system SHALL NOT surface a Linear closure prompt

#### Scenario: Partial closure reports the irreversible half

- **WHEN** a confirmed closure attempts the state write first and it fails, then posts the comment
- **THEN** the system SHALL report that the state change failed and the comment posted
- **AND** SHALL still close the issue out locally (unbind + marker)
- **AND** SHALL offer to retry the state carrying the issue id explicitly (the binding is already gone), without pretending the comment was rolled back
