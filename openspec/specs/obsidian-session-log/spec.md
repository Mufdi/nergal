# obsidian-session-log Specification

## Purpose
TBD - created by archiving change obsidian-bridge. Update Purpose after archive.
## Requirements
### Requirement: Continuous append on hook events
The system SHALL append a one-line entry to the configured `session_log_path` for every relevant hook event that flows through `hooks/server.rs::process_event`. The write SHALL happen in-process, synchronously, on the dispatcher thread. The write SHALL be best-effort (failures are logged via `tracing` but MUST NOT break the hook pipeline).

The relevant events and their formats SHALL be:

- `SessionStart` — writes the session header block (see next requirement).
- `PreToolUse` — `- <ISO> · Tool start: <tool_name>`
- `PostToolUse` — `- <ISO> · Tool end: <tool_name>` (and for file-touching tools, the file path: `- <ISO> · Edit: <relative path>`)
- `TaskCreated` / `TaskCompleted` — `- <ISO> · Task <created|completed>: "<subject>"`
- `Stop` — `- <ISO> · Stop (reason: <stop_reason>)`
- `UserPromptSubmit` — `- <ISO> · User prompt submitted`
- `PermissionDenied` — `- <ISO> · Permission denied: <tool> — <reason>`
- `PlanReview` (PreToolUse[ExitPlanMode]) — `- <ISO> · Plan ready: <plan_path>`
- `SessionEnd` — writes the session footer block.

Writes SHALL use `O_APPEND` on the resolved `session_log_path`. The file SHALL be created if absent (along with intermediate directories).

#### Scenario: New session writes header

- **WHEN** `HookEvent::SessionStart` fires for a session in a workspace whose config has `session_log_path` set
- **THEN** the system SHALL append the header block (see next requirement) to the file
- **AND** subsequent events for that session SHALL append event lines after the header

#### Scenario: Workspace without session_log_path is silent

- **WHEN** a session in a workspace with `session_log_path = null` produces hook events
- **THEN** no writes SHALL be attempted
- **AND** no warnings SHALL be logged (this is the expected disabled state)

#### Scenario: File-touching tool gets an Edit line

- **WHEN** `PostToolUse` fires for `Edit` (or any non-read-only tool whose tool_input has `file_path`)
- **THEN** the system SHALL emit two lines: `Tool end: Edit` and `Edit: <relative path>`
- **AND** the path SHALL be relativized to the workspace's `repo_path` if it falls inside

### Requirement: Session header and footer
The session header block SHALL be a markdown fragment composed of, in order: an H2 line `Session "<name>" — <ISO start>` (rendered with the literal `##` prefix), four bullet lines for `Agent: <agent_id> (<model_name>)`, `Workspace: <workspace name>`, `Cwd: <cwd at start>`, and a blank line, followed by an H3 line `Activity` (literal `###` prefix). The session footer block (written on `SessionEnd`) SHALL be a markdown fragment composed of: a blank line, an H3 line `Session ended at <ISO end>` (literal `###` prefix), and three bullet lines: `Final cost: $<formatted USD>`, `Files touched: <count> (<comma-separated basenames, max 10>)`, `Tasks completed: <count>`.

The system SHALL fetch the cost from `db.get_cost(session_id)`, the file count from `fileMapAtom` (or the equivalent DB persistence), and the task count from `db.get_visible_tasks(session_id)`.

#### Scenario: Header captures agent metadata

- **WHEN** `SessionStart` fires for a Codex session named "auth-refactor" in workspace "nergal"
- **THEN** the header SHALL include `Agent: codex (gpt-5-codex)` (or whatever the resolved model_name is)
- **AND** the workspace name SHALL match the user-visible workspace label

#### Scenario: Footer captures final state

- **WHEN** `SessionEnd` fires after a 30-minute session with 4 file edits, 2 completed tasks, $0.84 cost
- **THEN** the footer SHALL include `Final cost: $0.84`, `Files touched: 4 (foo.rs, bar.ts, …)`, `Tasks completed: 2`

### Requirement: One log file per workspace
The system SHALL treat `session_log_path` as a path to a single file (not a directory). All sessions belonging to the same workspace SHALL write to the same file, delimited by their `## Session "<name>"` headers. Concurrent sessions in the same workspace MAY interleave at line boundaries but MUST NOT corrupt each other's bytes (POSIX `O_APPEND` guarantees this for writes under 4 KB).

#### Scenario: Two concurrent sessions share the file

- **WHEN** two sessions in workspace W run in parallel
- **AND** both produce events at the same time
- **THEN** their event lines SHALL appear in chronological order in the log file
- **AND** no line SHALL be truncated or fused with another

### Requirement: Tracing on write failures
Write failures SHALL be reported via `tracing::warn!` with the event type and the file path. The dispatcher SHALL continue processing the event (other side-effects like atom updates SHALL still happen).

#### Scenario: Filesystem permission error

- **WHEN** the session_log_path points at a directory the user cannot write to
- **AND** a SessionStart event fires
- **THEN** the writer SHALL log a warning with the path and the error
- **AND** the frontend SHALL still receive the `session:start` Tauri event normally

