## ADDED Requirements

### Requirement: Resume approval refuses a session whose terminal is still open

The approval of a resume request SHALL fail when the target session still holds an agent PTY reservation while it is not live. A target that is live at approval SHALL keep failing with the existing "message it directly" reason, checked first. This happens when the agent PTY exited on its own while its tab stays open. The approval SHALL resolve the request as `failed` with a reason that tells the user to close the session's tab first, and SHALL queue no prompt for the session.

#### Scenario: Self-exited PTY in an open tab

- **GIVEN** session S is not live because its agent PTY exited on its own, and its tab is still open
- **WHEN** the user approves a `request_session_resume` request for S
- **THEN** the request SHALL resolve as `failed` with a reason that says to close the tab of S first, and no prompt SHALL be queued for S

#### Scenario: Closed tab resumes normally

- **GIVEN** session S was closed (its tab was closed and its PTY killed)
- **WHEN** the user approves a `request_session_resume` request for S
- **THEN** the approval SHALL proceed as before and return the session

#### Scenario: Live session at approval still answers message it directly

- **GIVEN** a resume request for session S is pending, and the user then opened S by hand so it is live
- **WHEN** the user approves the request
- **THEN** the request SHALL resolve as `failed` with the reason that S became live and should be messaged directly, not the close-the-tab reason

## MODIFIED Requirements

### Requirement: request_session_resume revives an existing inactive session through the same gate

The system SHALL expose a `request_session_resume(session_id, message?)` MCP tool that requests reviving an existing, currently-inactive session (the natural complement to `send_to_session` refusing an inactive target: that session exists and was worked before but is not live now). It SHALL go through the SAME human gate as `create_worktree_session`, be gated by `agent_spawned_worktrees_enabled`, and be non-blocking (return `{ pending_request_id }`). It SHALL validate that the session exists (`unknown_session` otherwise) and is not already live (`already_live`, pointing to `send_to_session`). A resume request SHALL carry NO agent-chosen escalation inputs — it reuses the target session's own launch options. On approval the system SHALL resume the session in its existing worktree ("continue" mode, no new worktree created) and, if a `message` was given, deliver it to the revived session as a labeled, advisory relayed first turn. To avoid pasting into a not-yet-ready prompt, the target SHALL be marked busy the instant it is approved, and delivery to it SHALL queue until its first idle from the moment its agent PTY is reserved. Between the approval and that reservation the session is not live, so `send_to_session` to it SHALL return `inactive_target`; the frontend reserves the PTY right after the approval returns, so this window is short.

#### Scenario: Resume requests through the gate, never creates directly

- **WHEN** an agent calls `request_session_resume` for an existing inactive session
- **THEN** the system SHALL enqueue a pending resume request behind the human gate and return `{ pending_request_id }`, reviving nothing until the user approves

#### Scenario: Unknown or already-live target

- **WHEN** the target session does not exist, OR is already live
- **THEN** the tool SHALL return `unknown_session` (does not exist) or `already_live` (use `send_to_session` instead), enqueuing nothing

#### Scenario: Approved resume revives in place and delivers the message

- **WHEN** the user approves a resume whose target still exists and is still not live
- **THEN** the session SHALL be resumed in its existing worktree (no new worktree), and any provided message SHALL be delivered to it as a labeled advisory relayed turn

#### Scenario: No new escalation surface

- **WHEN** the gate shows a resume request
- **THEN** it SHALL NOT present create-only escalation inputs (branch, agent, permission preset, startup_command) — a resume reuses the target's own launch options

#### Scenario: Send before the PTY reservation is refused, after it queues

- **GIVEN** the user approved a resume of session S and S is marked busy
- **WHEN** `send_to_session` targets S before its agent PTY is reserved, and again after the reservation
- **THEN** the first send SHALL return `inactive_target`, and the second SHALL return `queued`
