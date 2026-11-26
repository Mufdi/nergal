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

### Requirement: PTY child processes are reaped on session teardown

A PTY-backed session (agent session, aux shell, or quake shell) SHALL reap its child process on teardown so no defunct (zombie) process remains after the session exits or is killed. Reaping SHALL occur after the kill signal is delivered, and SHALL be bounded so a hung child cannot deadlock teardown.

#### Scenario: normal exit is reaped

- **GIVEN** a PTY session whose child exits on its own
- **WHEN** the session is torn down
- **THEN** the child is `wait()`ed and leaves no defunct process

#### Scenario: killed child is reaped

- **GIVEN** a PTY session with a still-running child
- **WHEN** the session is torn down (kill_tree sends SIGTERM)
- **THEN** the child is reaped after the signal and leaves no defunct process

#### Scenario: many sessions leave no zombie accumulation

- **WHEN** many sessions are opened and closed over the app's lifetime
- **THEN** the count of defunct child processes attributable to Nergal stays at zero

### Requirement: Session liveness tracks the agent PTY

A session SHALL be live exactly while this app process holds a running agent PTY for it. The system SHALL keep a live set that maps each live session id to the id of its agent PTY. Every liveness check SHALL read this set: `is_live` in `list_sessions` and `get_session`, the active-target check of `send_to_session`, and the `already_live` checks of `request_session_resume` and of its approval. Hook events and adapter-cache registration SHALL NOT mark a session live. The set SHALL start empty when the app starts.

#### Scenario: Closed tab is not live

- **GIVEN** session S has a running agent PTY `pty-S-1`, so `list_sessions` reports `is_live: true`, and S has a transcript marker inside the history window
- **WHEN** its tab closes and the soft-close window ends, so `kill_session_pty` runs for S
- **THEN** `get_session(S)` SHALL report `is_live: false`, and `request_session_resume(S)` SHALL return `pending`, not `already_live`

#### Scenario: Self-exited PTY is not live

- **GIVEN** session S is live under PTY `pty-S-1`
- **WHEN** the shell in `pty-S-1` exits on its own and its reader reaches end of file
- **THEN** S SHALL no longer be in the live set

#### Scenario: Late hook event does not revive a dead session

- **GIVEN** session S had its agent PTY killed and is not live
- **WHEN** a hook event that names S arrives afterwards (for example a `SessionEnd`)
- **THEN** S SHALL stay not live, and `send_to_session` targeting S SHALL return `inactive_target`

#### Scenario: Late exit of a replaced PTY keeps the new PTY live

- **GIVEN** session S was live under `pty-S-1`, that PTY was killed, and S was spawned again under `pty-S-2`
- **WHEN** the reader of `pty-S-1` reaches end of file after `pty-S-2` started
- **THEN** S SHALL stay live under `pty-S-2`

#### Scenario: Registering the adapter does not make a session live

- **GIVEN** session S has no agent PTY and has a transcript marker inside the history window
- **WHEN** the system registers S in the adapter cache (session creation, resume approval, or a hook event for an uncached session)
- **THEN** `list_sessions` SHALL report S with `is_live: false`

#### Scenario: Failed spawn is not live

- **GIVEN** session S has no agent PTY
- **WHEN** the PTY spawn step of `start_claude_session` for S fails, or S is killed while its PTY is starting
- **THEN** S SHALL not be in the live set

#### Scenario: A just-spawned session queues deliveries until its first idle

- **GIVEN** session S was not live and its last recorded mode is `idle`
- **WHEN** `start_claude_session` reserves a new agent PTY for S
- **THEN** S SHALL be live with mode `running`, so `send_to_session` to S SHALL queue until its next `Stop` instead of pasting into a shell that has no agent yet

#### Scenario: Releasing an older reservation keeps the newer one

- **GIVEN** S is live under `pty-S-2`, reserved after `pty-S-1` was killed
- **WHEN** the failed spawn of `pty-S-1` releases its reservation
- **THEN** the reservation and the liveness of `pty-S-2` SHALL stay in place

#### Scenario: Soft-closed tab stays live until the kill

- **GIVEN** session S is live and its tab was just closed
- **WHEN** an agent calls `get_session(S)` inside the 5 s undo window
- **THEN** it SHALL report `is_live: true`, because the PTY still runs

#### Scenario: App restart starts with no live session

- **GIVEN** the app just started, no agent PTY has been spawned yet, and S has a transcript marker inside the history window
- **WHEN** a hook event for session S arrives
- **THEN** `list_sessions` SHALL report S with `is_live: false`

#### Scenario: A killed session without a marker leaves the listing

- **GIVEN** session S was created and its PTY was killed before its first `Stop`, so S has no transcript marker
- **WHEN** an agent calls `list_sessions` and `get_session(S)`
- **THEN** S SHALL not appear in `list_sessions`, `get_session(S)` SHALL return not found, and `request_session_resume(S)` SHALL still accept it

#### Scenario: A not-live descriptor keeps past facts and drops present claims

- **GIVEN** session S was live in this app run, recorded mode `needs_attention` with a `waiting_for`, touched `a.rs`, recorded last activity at time T and a last assistant message M, reported a background task and a cron, and its PTY was then killed; S has a transcript marker inside the history window
- **WHEN** an agent calls `get_session(S)`
- **THEN** the descriptor SHALL have `is_live: false`, `mode` from the persisted row, `waiting_for` null, and empty `background_tasks` and `session_crons`, because those claim something about the present
- **AND** it SHALL keep `last_activity` T, `last_assistant_message` M and `recently_touched_files` with `a.rs`, because those describe what already happened

### Requirement: Descriptor mode follows the reported root status

For a live session that has a root status (`program-status-protocol`), the descriptor's `mode` and `waiting_for` SHALL combine it with the hook-derived activity, and the `mode` values SHALL stay `idle`, `running`, `needs_attention` and `completed`. `mode` SHALL be `needs_attention` when the root status is `blocked` or the hook-derived mode is `needs_attention`. Otherwise `mode` SHALL be `running` for root status `working`, and `idle` for `done`, `idle` and `error`, because a session in those states accepts a new prompt. When the root status is `blocked`, `waiting_for` SHALL be its `msg` cut to 200 characters, or, without a `msg`, `permission`, `question`, `login` or `dialog` for `kind` `permission`, `question`, `auth` or none. When only the hook-derived mode is `needs_attention`, `waiting_for` SHALL keep the hook value. A live session without a root status SHALL keep the hook-derived `mode` and `waiting_for`. `last_activity` SHALL be the later of the hook activity time and the time the root status was set; a root status SHALL NOT create or change the hook-derived activity entry.

#### Scenario: Working after Stop reports running

- **GIVEN** live session S got a `Stop` hook (hook-derived mode `idle`)
- **WHEN** S's root status is `working`
- **THEN** the descriptor SHALL report S with `mode` `running` and no `waiting_for`

#### Scenario: Folder-trust prompt reports needs_attention

- **WHEN** S's root status is `blocked` with no `kind` and no `msg`
- **THEN** the descriptor SHALL report S with `mode` `needs_attention` and `waiting_for` `dialog`

#### Scenario: Blocked msg becomes waiting_for

- **WHEN** S's root status is `blocked` with `kind=permission` and `msg="Allow Bash?"`
- **THEN** the descriptor SHALL report S with `mode` `needs_attention` and `waiting_for` `Allow Bash?`

#### Scenario: Error reports idle

- **WHEN** S's root status is `error`
- **THEN** the descriptor SHALL report S with `mode` `idle`

#### Scenario: Background agent wait wins over a done status

- **GIVEN** S's hook-derived mode is `needs_attention` with `waiting_for` starting with `background agent`
- **WHEN** S's root status is `done`
- **THEN** the descriptor SHALL report S with `mode` `needs_attention` and the hook `waiting_for`

#### Scenario: Root status overrides a running hook mode

- **GIVEN** live session S has hook-derived mode `running` set at time 900 (from the PTY reservation)
- **WHEN** S's root status `idle` was set at time 1000
- **THEN** the descriptor SHALL report S with `mode` `idle` and `last_activity` 1000
- **AND** the hook-derived activity of S SHALL still be `running` at time 900

#### Scenario: No root status keeps hook mode

- **GIVEN** S never sent an OSC 7501 report and its hook-derived mode is `running`
- **THEN** the descriptor SHALL report S with `mode` `running`
