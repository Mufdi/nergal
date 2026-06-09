## ADDED Requirements

### Requirement: list_sessions tool

The system SHALL expose a `list_sessions` MCP tool returning a descriptor for every live session known to the daemon, optionally filtered (e.g. by workspace or agent). The result SHALL be assembled from data cluihud already holds, with no AI and no transcript parse on the hot path.

#### Scenario: Enumerate sibling sessions across workspaces

- **WHEN** an agent in session A (workspace W1) calls `list_sessions`
- **THEN** the tool SHALL return descriptors for all live sessions including session B in workspace W2, each with current metadata

#### Scenario: Caller can exclude itself

- **WHEN** an agent calls `list_sessions` with a filter that excludes the current session
- **THEN** the caller's own session SHALL be omitted from the result

### Requirement: Session descriptor schema

A session descriptor SHALL include at minimum: session id, display name, workspace path/name, git branch, agent type, current mode (idle/active/tool name), `waitingFor` when blocked, last-activity timestamp, and recently-touched files. When available it SHALL additively include `background_tasks` and `session_crons`, and (when enabled) an AI `summary` with its own timestamp. Fields that are unknown SHALL be null rather than fabricated.

#### Scenario: Cheap fields always present

- **WHEN** a descriptor is returned
- **THEN** id, name, workspace, agent, and mode SHALL always be populated from live state

#### Scenario: Optional fields null when absent

- **WHEN** a session has no background tasks, no crons, and AI summaries are disabled
- **THEN** `background_tasks`, `session_crons`, and `summary` SHALL be null/empty, not fabricated

### Requirement: get_session tool

The system SHALL expose a `get_session` MCP tool that returns the full descriptor for one session by id, including recent activity and (when present) background tasks/crons and the AI summary.

#### Scenario: Inspect a specific session

- **WHEN** an agent calls `get_session` with a known session id
- **THEN** the tool SHALL return that session's full descriptor

#### Scenario: Unknown session id

- **WHEN** an agent calls `get_session` with an id not in the directory
- **THEN** the tool SHALL return a structured "session not found" error

### Requirement: Background tasks and crons captured from Stop hooks

The system SHALL extend the `Stop` and `SubagentStop` hook event schema with optional `background_tasks` and `session_crons` fields (additive, defaulted), capture them into session state, and surface them through the directory. Older hook payloads without these fields SHALL remain valid.

#### Scenario: Background tasks surfaced after Stop

- **WHEN** a Stop hook payload includes one or more `background_tasks`
- **THEN** those tasks SHALL appear in the session's `get_session` descriptor

#### Scenario: Legacy payload still parses

- **WHEN** a Stop hook payload omits `background_tasks` and `session_crons`
- **THEN** the event SHALL deserialize successfully with those fields empty
