# clickup-writeback Specification

## Purpose
TBD - created by archiving change clickup-writeback. Update Purpose after archive.
## Requirements
### Requirement: Bidirectional task editing with server-side validation

The system SHALL let the user edit a task from Nergal — change status, post comments, toggle checklist items, and edit description, assignees, and due date — and reflect each change in ClickUp via the REST API. Assignee edits SHALL be modeled as add/remove diffs, not a full-set replace; the UI surface is remove-only (the mirror holds no workspace member directory to pick additions from — adding assignees from Nergal would require a live member-list call, deferred). Custom-field writes SHALL serialize per the field's type and SHALL reject computed fields (e.g. `automatic_progress`) and unsupported types **at the command boundary**, not only in the UI. Status writes SHALL validate the value is a real status of the task's List. Status options SHALL be the task's List statuses (per-List custom, never hardcoded). Because a List that inherits its Space/Folder workflow returns an empty `statuses[]` from the poll's folder/folderless endpoints, the option set and the write-validation source SHALL be resolved live from `GET /list/{id}` (the fully-resolved set ClickUp accepts) and cached into `clickup_statuses`; a network failure SHALL degrade to whatever the mirror already holds.

#### Scenario: Status options resolve the List's true workflow

- **WHEN** the user opens the status picker for a task whose List inherits the Space workflow
- **THEN** the system SHALL resolve the List's statuses live and present the full set ClickUp accepts
- **AND** SHALL validate a status write against that resolved set

#### Scenario: Status change reflects upstream after validation

- **WHEN** the user changes a task's status to one of its List's statuses
- **THEN** the system SHALL validate the status belongs to the List
- **AND** call the API to set it
- **AND** the change SHALL be visible in ClickUp

#### Scenario: Computed or invalid field rejected at the boundary

- **WHEN** a write targets a computed custom field or an invalid status
- **THEN** the system SHALL reject it at the command boundary, not only in the UI

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

### Requirement: Echo dedup and field-class conflict resolution by value comparison

The system SHALL decide echo and conflict by comparing field values in the fetched task payload, not by keying on the task's `date_updated` (which is coarse, per-task, and not predictable pre-call). After a successful write, the system SHALL record it; on the next poll, if the server's current value of that field equals the written value the change SHALL be treated as the user's own echo — reconciled silently, not notified. The echo check SHALL run before new-assignment detection so an own write never self-notifies. Conflict resolution SHALL depend on field class: scalar fields (status, due, description, single-select) SHALL be last-writer-wins by `date_updated` and SHALL warn when a remote value supersedes a local edit; additive fields (assignees, checklist items, labels) SHALL merge to the server state without a false "superseded" warning.

#### Scenario: Own write is not re-notified

- **WHEN** a poll observes a task whose changed field value equals a recent local write
- **THEN** the system SHALL reconcile silently and SHALL NOT emit a notification

#### Scenario: Scalar conflict warns

- **WHEN** a remote change supersedes a local scalar edit (status/due/description)
- **THEN** the remote value SHALL overwrite the mirror
- **AND** the system SHALL warn that the local edit was superseded

#### Scenario: Additive merge does not false-warn

- **WHEN** a remote add and a local add to an additive field both apply server-side
- **THEN** the system SHALL reconcile to the merged server state
- **AND** SHALL NOT warn that the local edit was superseded

### Requirement: Comments are posted once, never optimistically rolled back

Because a comment is a separate append-only resource that cannot be un-posted and ClickUp has no comment idempotency key, the system SHALL NOT treat comments like field writes. A comment SHALL be inserted into the mirror only after the POST returns the created comment id. On an ambiguous failure the system SHALL NOT auto-retry; it SHALL mark the send uncertain and, before any retry, re-fetch the task's comments to check whether it landed, to avoid posting a duplicate team-visible comment.

#### Scenario: Comment posted once

- **WHEN** the user posts a comment
- **THEN** it SHALL be inserted into the mirror only after the POST returns its id

#### Scenario: Ambiguous failure does not duplicate

- **WHEN** a comment POST times out with no response
- **THEN** the system SHALL NOT auto-retry blindly
- **AND** SHALL re-fetch comments to check whether it landed before any retry

### Requirement: Structural confirmation boundary for outward-facing writes

The safety of outward-facing writes against a full-access personal token SHALL NOT rest on a frontend-only confirmation. The closure and comment writes SHALL execute solely via a command that requires a short-lived confirmation token issued by the backend only when the user confirms a specific action, so a renderer bug or injected content cannot auto-fire them by calling a command directly.

#### Scenario: Closure write requires a backend token

- **WHEN** code attempts the closure or comment write without a valid confirmation token
- **THEN** the system SHALL refuse the write

#### Scenario: Token issued only on user confirmation

- **WHEN** the user confirms a specific closure action
- **THEN** the backend SHALL issue a short-lived token scoped to that action
- **AND** the execution command SHALL accept only that token

### Requirement: Ship-anchored and manual write-back closure

The system SHALL offer the write-back closure at two agent-agnostic moments: on demand via an explicit "Close out task" action in the task detail (the primary path), and automatically after a successful ship (PR created via Nergal's own ship flow) of a session that has an `active_clickup_task_id`, with the comment prefilled (editable) with the PR link. Close-out's core act SHALL be marking the task done locally — it SHALL unbind the task from the session and persist a local "worked & closed" marker that is independent of the ClickUp status (the task keeps whatever status ClickUp holds) and SHALL surface as a panel badge. Layered on top, the prompt SHALL offer two independent, OPTIONAL halves: a status move chosen manually from the task's List statuses (per-List custom, never hardcoded) and a comment; confirm SHALL execute only the selected halves — a status-only closure SHALL be the frictionless path and an empty comment SHALL post nothing. Confirm SHALL always be armed: confirming with neither half selected SHALL still close the task out locally (unbind + marker) without any ClickUp write. One offer per successful ship; a push without a PR SHALL NOT trigger the offer. Writes SHALL occur only on explicit confirmation (via the token boundary). The trigger SHALL NOT rely on agent-specific hooks, SHALL NOT write to any agent's user settings, and SHALL NOT hardcode a specific agent. (Design Revision 1: the originally-specified `SessionStatus Completed` edge does not exist at runtime — nothing writes session status transitions.)

#### Scenario: Manual closure is the primary path

- **WHEN** the user invokes "Close out task" from the task detail
- **THEN** the system SHALL surface the closure prompt without a PR prefill
- **AND** the status picker SHALL list the statuses of that task's List

#### Scenario: Status-only closure posts no comment

- **WHEN** the user confirms a closure with a status selected and the comment empty
- **THEN** the system SHALL move the status
- **AND** SHALL NOT post any comment

#### Scenario: Close-out with no halves still marks the task done

- **WHEN** the user confirms a closure with neither a status nor a comment selected
- **THEN** the system SHALL NOT write anything to ClickUp
- **AND** SHALL unbind the task from the session
- **AND** SHALL persist the local "worked & closed" marker

#### Scenario: Worked-and-closed marker is independent of ClickUp status

- **WHEN** a task has been closed out from a session
- **THEN** the panel SHALL flag it with a worked-and-closed badge
- **AND** the badge SHALL persist across restarts regardless of the task's ClickUp status

#### Scenario: Closure offered after a successful ship of a bound session

- **WHEN** a ship completes successfully (PR created) for a session with an `active_clickup_task_id`
- **THEN** the system SHALL surface the closure prompt once for that ship
- **AND** the comment prefill SHALL include the PR link and SHALL remain user-editable

#### Scenario: No write without confirmation

- **WHEN** the closure prompt is dismissed
- **THEN** the system SHALL NOT write anything to ClickUp

#### Scenario: Unbound session offers no closure

- **WHEN** a ship completes for a session without an `active_clickup_task_id`
- **THEN** the system SHALL NOT surface a ClickUp closure prompt

#### Scenario: Partial closure reports the irreversible half

- **WHEN** a confirmed closure posts the comment but the status write fails
- **THEN** the system SHALL report that the comment posted and the status change failed
- **AND** SHALL offer to retry the status, without pretending the comment was rolled back

