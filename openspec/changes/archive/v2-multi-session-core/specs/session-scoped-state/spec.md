## ADDED Requirements

### Requirement: Plan state scoped per session
Each session SHALL have independent plan state: content, original, path, mode, diff. Switching sessions SHALL swap the plan panel to show the active session's plan.

#### Scenario: Session A has plan, session B does not
- **WHEN** user switches from session A (which has a plan loaded) to session B (no plan)
- **THEN** plan panel shows "No plan yet" for session B
- **AND** switching back to session A restores its plan content

### Requirement: Task state scoped per session
Each session SHALL have independent task lists. The backend SHALL route `tasks:update` events to the correct session's TaskStore by `session_id`.

#### Scenario: Tasks from different sessions
- **WHEN** session A creates 3 tasks and session B creates 2 tasks
- **THEN** switching to session A shows 3 tasks
- **AND** switching to session B shows 2 tasks
- **AND** tasks never cross-contaminate between sessions

### Requirement: Activity log scoped per session
Each session SHALL have its own activity log. Hook events SHALL be appended to the correct session's log by `session_id`.

#### Scenario: Activity from concurrent sessions
- **WHEN** session A and session B are both running
- **THEN** session A's activity log only shows events from session A
- **AND** session B's activity log only shows events from session B

### Requirement: Cost tracking scoped per session
Each session SHALL track its own token usage and cost. The status bar SHALL display the active session's cost.

#### Scenario: Cost update for non-active session
- **WHEN** a `cost:update` event arrives for session B while session A is active
- **THEN** session B's cost is updated in the background
- **AND** the status bar continues showing session A's cost

### Requirement: Modified files scoped per session
Each session SHALL track its own list of modified files. This is already partially implemented via `fileMapAtom[session_id]`.

#### Scenario: Files modified in parallel sessions
- **WHEN** session A modifies `src/App.tsx` and session B modifies `src/main.ts`
- **THEN** switching to session A shows only `src/App.tsx` in modified files
- **AND** switching to session B shows only `src/main.ts`

### Requirement: Hook event routing by session_id
The backend hook server SHALL route all events to the correct session's state manager. Events with an unknown session_id SHALL create a transient session entry.

#### Scenario: Hook event arrives for known session
- **WHEN** a `pre_tool_use` event arrives with session_id matching session A
- **THEN** session A's mode is updated to the tool name
- **AND** session A's activity log receives the event
- **AND** no other session's state is affected
