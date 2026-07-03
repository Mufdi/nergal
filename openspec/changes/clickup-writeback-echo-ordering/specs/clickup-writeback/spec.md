# clickup-writeback

Own-echo suppression records provisionally before the API call, closing the poll race window.

## MODIFIED Requirements

### Requirement: Echo dedup and field-class conflict resolution by value comparison

The system SHALL decide echo and conflict by comparing field values in the fetched task payload, not by keying on the task's `date_updated` (which is coarse, per-task, and not predictable pre-call). The system SHALL record each write **provisionally before issuing the API call** and SHALL clear the provisional entry if the call fails, so no poll — however timed — can observe the server's post-write value without a matching registry entry. On the next poll, if the server's current value of that field equals the written value the change SHALL be treated as the user's own echo — reconciled silently, not notified. The echo check SHALL run before new-assignment detection so an own write never self-notifies. Conflict resolution SHALL depend on field class: scalar fields (status, due, description, single-select) SHALL be last-writer-wins by `date_updated` and SHALL warn when a remote value supersedes a local edit; additive fields (assignees, checklist items, labels) SHALL merge to the server state without a false "superseded" warning.

#### Scenario: Own write is not re-notified

- **WHEN** a poll observes a task whose changed field value equals a recent local write
- **THEN** the system SHALL reconcile silently and SHALL NOT emit a notification

#### Scenario: Poll racing the write command still sees the record

- **GIVEN** the user assigns a task to themselves and the API call has been sent but the command has not yet resumed
- **WHEN** the 45s poll reconciles during that window
- **THEN** the provisional registry entry matches the server value and no self-notification is emitted

#### Scenario: Failed write does not suppress a real remote change

- **GIVEN** a write's API call fails and its provisional entry is cleared
- **WHEN** a later poll observes a remote change to that field (even to the same value)
- **THEN** normal change handling applies (no echo suppression from the failed write)

#### Scenario: Scalar conflict warns

- **WHEN** a remote change supersedes a local scalar edit (status/due/description)
- **THEN** the remote value SHALL overwrite the mirror
- **AND** the system SHALL warn that the local edit was superseded

#### Scenario: Additive merge does not false-warn

- **WHEN** a remote add and a local add to an additive field both apply server-side
- **THEN** the system SHALL reconcile to the merged server state
- **AND** SHALL NOT warn that the local edit was superseded
