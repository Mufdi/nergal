## ADDED Requirements

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
