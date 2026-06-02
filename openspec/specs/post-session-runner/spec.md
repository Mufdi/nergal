# post-session-runner Specification

## Purpose
TBD - created by archiving change obsidian-bridge. Update Purpose after archive.
## Requirements
### Requirement: New CLI subcommand `cluihud post-session`
The system SHALL add a new top-level subcommand to the `cluihud` CLI binary: `cluihud post-session`. The subcommand SHALL acquire a global advisory file lock at `~/.config/cluihud/post-session.lock` using `fs2::FileExt::try_lock_exclusive`, drain pending markers from `~/.config/cluihud/pending-mocs/`, and exit. If the lock cannot be acquired (sibling already running), the subcommand SHALL exit 0 immediately with no work.

The subcommand SHALL be invoked exclusively as a detached child via `obsidian::post_session::spawn_runner_detached`, never blockingly from the GUI process.

#### Scenario: First runner starts work

- **WHEN** `cluihud post-session` is invoked
- **AND** no other runner holds the global lock
- **THEN** the subcommand SHALL acquire the lock
- **AND** process every marker file in `~/.config/cluihud/pending-mocs/`
- **AND** exit 0 after releasing the lock

#### Scenario: Concurrent runner sees the lock

- **WHEN** a runner is already mid-flight
- **AND** a second `cluihud post-session` is invoked
- **THEN** the second invocation SHALL see the lock as held
- **AND** exit 0 immediately without touching any markers

### Requirement: Marker file lifecycle
The system SHALL drop a marker file at `~/.config/cluihud/pending-mocs/<session_id>.json` on every event that should produce a session MOC. Triggers MUST be:

- `HookEvent::SessionEnd` arriving on the hook socket.
- Explicit user-driven session deletion via `delete_session`.
- Workspace deletion via `delete_workspace` (one marker per session in the deleted workspace).
- `WindowEvent::CloseRequested` (one marker per active session).
- PTY-side session termination (worker thread observes EOF on the PTY).

Each marker MUST contain JSON with at minimum: `session_id`, `workspace_id`, `agent_id`, `trigger` (one of the above), `created_at` (unix ms).

The system SHALL write markers atomically (tmp file + rename). The system SHALL delete a marker only after its MOC + backlink propagation completes successfully.

#### Scenario: SessionEnd hook drops a marker

- **WHEN** the hook server receives a `SessionEnd` event for session S
- **AND** the workspace owning S has a non-null `vault_root`
- **THEN** the dispatcher SHALL write `~/.config/cluihud/pending-mocs/S.json`
- **AND** spawn `cluihud post-session` detached

#### Scenario: App close drops markers for all active sessions

- **WHEN** the user closes Nergal
- **AND** sessions S1 and S2 are active in workspaces with configured vaults
- **THEN** the close handler SHALL write `S1.json` and `S2.json` markers
- **AND** spawn the runner detached
- **AND** `app.exit()` SHALL be called immediately afterwards

#### Scenario: Workspace without vault skips marker

- **WHEN** SessionEnd fires for a session whose workspace has `vault_root = null`
- **THEN** no marker SHALL be written (no work to do)

### Requirement: Detached spawn on Linux
The runner spawn helper SHALL use `std::process::Command::new("cluihud")` with `pre_exec` invoking `libc::setsid()` to detach from the controlling terminal, AND SHALL null stdin/stdout/stderr so the bg process does not inherit fds the parent will close. The helper SHALL be `#[cfg(target_os = "linux")]` only — Nergal is Linux-only.

The helper SHALL NOT wait for the child. It SHALL return immediately after spawn.

#### Scenario: Bg process survives GUI exit

- **WHEN** the runner is spawned right before `app.exit()`
- **AND** the GUI process exits
- **THEN** the bg process SHALL continue running until its work completes
- **AND** SHALL NOT be killed by the parent's exit

### Requirement: Recovery on next launch
On Nergal startup, after `reconcile_worktrees` runs, the system SHALL list pending markers and count those older than 10 minutes. If any are present, the system SHALL spawn the runner detached and emit a Sileo toast to the frontend "Caught up on N pending session snapshots" once the recovery completes.

If the recovery run fails (logged in `~/.config/cluihud/logs/post-session.log`), the system SHALL emit a louder toast pointing at the log path.

#### Scenario: Stale markers on launch

- **WHEN** Nergal starts
- **AND** `~/.config/cluihud/pending-mocs/` contains a marker file with `created_at` older than 10 minutes
- **THEN** the system SHALL spawn the runner detached during startup
- **AND** once the runner completes, a toast SHALL surface noting the recovered session count

#### Scenario: No stale markers

- **WHEN** the pending-mocs directory is empty (or all markers are < 10 min old)
- **THEN** no recovery spawn SHALL happen
- **AND** no toast SHALL surface

### Requirement: Runner failure surfaces to user
The runner SHALL log every step to `~/.config/cluihud/logs/post-session.log` (rotative: 5 MB max, 3 generations retained). Failures (filesystem errors, panic, invalid markers) SHALL be logged with stack traces.

When Nergal next launches, the system SHALL inspect the tail of the log. If the most recent run ended with `ERROR`, the system SHALL surface a non-dismissive Sileo toast "Last session snapshot failed — see <log path>".

#### Scenario: Successful runner is silent

- **WHEN** the runner completes normally
- **AND** the log's last entry is `INFO drained N markers`
- **THEN** Nergal's next launch SHALL show no error toast

#### Scenario: Runner panic surfaces on next launch

- **WHEN** the runner panics during marker N
- **AND** the log's last entry contains `ERROR`
- **THEN** the next Nergal launch SHALL surface the error toast
- **AND** the markers that did not complete SHALL be retried on the next runner invocation

