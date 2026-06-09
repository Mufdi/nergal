# clickup-writeback

## ADDED Requirements

### Requirement: Bidirectional task editing with server-side validation

The system SHALL let the user edit a task from Nergal — change status, post comments, toggle checklist items, and edit description, assignees, and due date — and reflect each change in ClickUp via the REST API. Assignee edits SHALL be modeled as add/remove diffs, not a full-set replace. Custom-field writes SHALL serialize per the field's type and SHALL reject computed fields (e.g. `automatic_progress`) and unsupported types **at the command boundary**, not only in the UI. Status writes SHALL validate the value is a real status of the task's List. Status options SHALL be drawn from the task's List statuses (`clickup_statuses`), never hardcoded.

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

### Requirement: Edge-triggered, idempotent write-back-on-done closure

The system SHALL offer the closure on the transition edge `prev != Completed && new == Completed` for a session with an active bound task — not on every `Completed` write — and SHALL be idempotent per completion (one prompt per transition). The prompt SHALL offer to move the task's status (from its List statuses) and post a comment, and SHALL write only on explicit confirmation (via the token boundary). It SHALL be offered only for sessions that have an `active_clickup_task_id`.

#### Scenario: Closure offered on the completion edge only

- **WHEN** a session with an `active_clickup_task_id` transitions from not-Completed to Completed
- **THEN** the system SHALL surface the closure prompt once
- **AND** SHALL NOT re-fire it on a subsequent repeated `Completed` write

#### Scenario: No write without confirmation

- **WHEN** the closure prompt is dismissed
- **THEN** the system SHALL NOT write anything to ClickUp

#### Scenario: Unbound session offers no closure

- **WHEN** a session without an `active_clickup_task_id` reaches Completed
- **THEN** the system SHALL NOT surface a ClickUp closure prompt

#### Scenario: Partial closure reports the irreversible half

- **WHEN** a confirmed closure posts the comment but the status write fails
- **THEN** the system SHALL report that the comment posted and the status change failed
- **AND** SHALL offer to retry the status, without pretending the comment was rolled back
