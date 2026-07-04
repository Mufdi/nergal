# clickup-task-panel

Scopes the task-detail view's async results and write lock to the currently displayed task.

## ADDED Requirements

### Requirement: Detail results are scoped to the displayed task

Async task-detail results (initial load, post-write confirm polling, comment/assignee refreshes) SHALL be applied to the view only if the task they were requested for is still the displayed task; results for a previously displayed task MUST be discarded.

#### Scenario: write confirm resolves after switching tasks

- **GIVEN** the user edits task A (status/description/checklist/due-date/assignee/comment) and the post-write confirm poll is still in flight
- **WHEN** the user opens task B before the poll resolves
- **THEN** the poll's result for A is not applied to the view, and B's detail renders B's data

#### Scenario: same-task confirm still lands

- **WHEN** a post-write confirm poll resolves while its task is still displayed
- **THEN** the refreshed detail is applied and the optimistic overlay entry is cleared, as today

### Requirement: Write lock resets on task switch

The single-flight write lock (`busy`) SHALL reset when the displayed task changes, and a stale write completing for a previous task MUST NOT clear the lock of a newer in-flight write.

#### Scenario: navigating mid-write does not lock the next task

- **GIVEN** a write on task A is in flight (lock held)
- **WHEN** the user opens task B
- **THEN** B's write controls are immediately usable

#### Scenario: stale completion does not unlock a fresh write

- **GIVEN** the user switched from task A (write in flight) to task B and started a write on B
- **WHEN** A's write completes
- **THEN** B's lock remains held until B's own write resolves
