## ADDED Requirements

### Requirement: list_sessions tool

The system SHALL expose a `list_sessions` MCP tool returning a descriptor for every live session known to the daemon, optionally filtered. The result SHALL be assembled from data cluihud already holds, with no AI and no transcript parse on the hot path.

#### Scenario: Enumerate sibling sessions across workspaces

- **WHEN** an agent in session A (workspace W1) calls `list_sessions`
- **THEN** the tool SHALL return descriptors for all live sessions including session B in workspace W2, each with current metadata

#### Scenario: Caller can exclude itself

- **WHEN** an agent calls `list_sessions` with a filter that excludes the current session
- **THEN** the caller's own session SHALL be omitted from the result

### Requirement: Snapshot-then-release assembly (no reactor stalls)

Descriptor assembly SHALL acquire the `AgentRuntimeState` mutex only long enough to copy the cheap in-memory fields, then release it before performing any git, filesystem, or subprocess work. No blocking call SHALL execute while the mutex is held. `claude agents --json` enrichment SHALL be served from an out-of-band cache, never spawned synchronously on a directory read.

#### Scenario: No blocking work under the lock

- **WHEN** `list_sessions` assembles descriptors
- **THEN** git metadata and any subprocess enrichment SHALL be read outside the held mutex, so concurrent reads do not serialize on slow I/O

#### Scenario: Enrichment from cache

- **WHEN** a descriptor includes CC `waitingFor`/`state` enrichment
- **THEN** it SHALL come from a cache refreshed out-of-band, not from a subprocess spawned during the read

### Requirement: Session descriptor schema

A session descriptor SHALL include at minimum: session id, display name, workspace path/name, git branch, agent type, current mode (idle/active/tool name), `waitingFor` when blocked, last-activity timestamp, and recently-touched files. When available it SHALL additively include `background_tasks`, `session_crons`, and (when enabled) an AI `summary` with its own timestamp. Unknown fields SHALL be null, never fabricated. The descriptor schema SHALL document that cross-workspace fields (workspace paths, recently-touched file paths, summaries) are exposed over MCP to **any same-uid caller** (the directory is global-read within the uid; identity is not an access gate).

#### Scenario: Cheap fields always present

- **WHEN** a descriptor is returned
- **THEN** id, name, workspace, agent, and mode SHALL always be populated from live state

#### Scenario: Optional fields null when absent

- **WHEN** a session has no background tasks, no crons, and AI summaries are disabled
- **THEN** `background_tasks`, `session_crons`, and `summary` SHALL be empty/null, not fabricated

### Requirement: get_session tool

The system SHALL expose a `get_session` MCP tool returning the full descriptor for one session by id, including recent activity and (when present) background tasks/crons and the AI summary.

#### Scenario: Inspect a specific session

- **WHEN** an agent calls `get_session` with a known session id
- **THEN** the tool SHALL return that session's full descriptor

#### Scenario: Unknown session id

- **WHEN** an agent calls `get_session` with an id not in the directory
- **THEN** the tool SHALL return a structured "session not found" error

### Requirement: Background tasks and crons captured from Stop hooks

The system SHALL extend the `Stop` and `SubagentStop` hook event schema with optional `background_tasks` and `session_crons` fields (additive, `#[serde(default)]`), capture them into session state, and surface them through the directory. Older hook payloads without these fields SHALL remain valid.

#### Scenario: Background tasks surfaced after Stop

- **WHEN** a Stop hook payload includes one or more `background_tasks`
- **THEN** those tasks SHALL appear in the session's `get_session` descriptor

#### Scenario: Legacy payload still parses

- **WHEN** a Stop hook payload omits `background_tasks` and `session_crons`
- **THEN** the event SHALL deserialize successfully with those fields empty
