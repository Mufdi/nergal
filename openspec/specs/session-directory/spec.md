# session-directory Specification

## Purpose
TBD - created by archiving change nergal-mcp-server. Update Purpose after archive.
## Requirements
### Requirement: list_sessions tool

The system SHALL expose a `list_sessions` MCP tool returning a descriptor for every live session known to the daemon, plus every **recently-dead** session (one whose `last_stop_at` falls within the configured recency window), optionally filtered. Each descriptor SHALL carry `is_live` so a caller can distinguish a running session from a recalled one. The result SHALL be assembled from data nergal already holds, with no AI and no transcript parse on the hot path. `list_sessions` SHALL serve cached summaries only and SHALL NOT trigger summary generation for any session (only `get_session` does), so a directory listing never fans out into per-session inference.

#### Scenario: Enumerate sibling sessions across workspaces

- **WHEN** an agent in session A (workspace W1) calls `list_sessions`
- **THEN** the tool SHALL return descriptors for all live sessions including session B in workspace W2, each with current metadata and `is_live: true`

#### Scenario: Recently-dead sessions included

- **WHEN** a session ended within the recency window and `list_sessions` is called
- **THEN** its descriptor SHALL be returned with `is_live: false` and its last-persisted summary (or null), and no generation SHALL be triggered

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

A session descriptor SHALL include at minimum: session id, display name, workspace path/name, git branch, agent type, current mode (idle/active/tool name), `waitingFor` when blocked, last-activity timestamp, recently-touched files, and an `is_live` boolean marking whether the session is currently running. When available it SHALL additively include `background_tasks`, `session_crons`, and (when enabled) an AI `summary` with its own timestamp plus a `summary_stale` boolean indicating whether the served summary predates the session's latest activity (`last_stop_at > summary.updated_at`). Unknown fields SHALL be null, never fabricated. The descriptor schema SHALL document that cross-workspace fields (workspace paths, recently-touched file paths, summaries) are exposed over MCP to **any same-uid caller** (the directory is global-read within the uid; identity is not an access gate), and that this exposure now includes recently-dead sessions, not only live ones.

#### Scenario: Cheap fields always present

- **WHEN** a descriptor is returned
- **THEN** id, name, workspace, agent, mode, and `is_live` SHALL always be populated from live or persisted state

#### Scenario: Stale summary flagged

- **WHEN** a descriptor carries a summary whose `updated_at` predates the session's `last_stop_at`
- **THEN** `summary_stale` SHALL be true so the caller knows newer activity is not yet reflected

#### Scenario: Dead-session descriptor is degraded, documented

- **WHEN** `get_session` returns a recently-dead session not seen since daemon start (the in-memory activity side-maps are empty)
- **THEN** `is_live` SHALL be false, `mode` SHALL reflect the frozen persisted value, the live-only activity fields (`recently_touched_files`, `background_tasks`, `session_crons`, `last_assistant_message`) SHALL be empty, and only `summary` (regenerable from the transcript) and `git_branch` (persisted) carry meaning — the descriptor schema SHALL document this so empty fields are not read as "no activity occurred"

#### Scenario: Optional fields null when absent

- **WHEN** a session has no background tasks, no crons, and AI summaries are disabled
- **THEN** `background_tasks`, `session_crons`, and `summary` SHALL be empty/null, not fabricated

### Requirement: get_session tool

The system SHALL expose a `get_session` MCP tool returning the full descriptor for one session by id, including recent activity and (when present) background tasks/crons and the AI summary. `get_session` SHALL resolve both live sessions and recently-dead sessions (within the recency window). As the sole intentional single-session read, `get_session` is the **only** tool that triggers lazy summary generation: when it resolves a dirty session (no summary, or `last_stop_at > summary.updated_at`) and no generation is in flight, it SHALL spawn generation detached and return the current (stale or null) summary without blocking.

#### Scenario: Inspect a specific session

- **WHEN** an agent calls `get_session` with a known live session id
- **THEN** the tool SHALL return that session's full descriptor

#### Scenario: Inspect a recently-dead session

- **WHEN** an agent calls `get_session` with the id of a session that ended within the recency window
- **THEN** the tool SHALL return its descriptor with `is_live: false`, and if dirty SHALL trigger on-demand generation from the persisted transcript without blocking the read

#### Scenario: Unknown or expired session id

- **WHEN** an agent calls `get_session` with an id that is neither live nor a recently-dead session within the window
- **THEN** the tool SHALL return a structured "session not found" error

### Requirement: Background tasks and crons captured from Stop hooks

The system SHALL extend the `Stop` and `SubagentStop` hook event schema with optional `background_tasks` and `session_crons` fields (additive, `#[serde(default)]`), capture them into session state, and surface them through the directory. Older hook payloads without these fields SHALL remain valid.

#### Scenario: Background tasks surfaced after Stop

- **WHEN** a Stop hook payload includes one or more `background_tasks`
- **THEN** those tasks SHALL appear in the session's `get_session` descriptor

#### Scenario: Legacy payload still parses

- **WHEN** a Stop hook payload omits `background_tasks` and `session_crons`
- **THEN** the event SHALL deserialize successfully with those fields empty

