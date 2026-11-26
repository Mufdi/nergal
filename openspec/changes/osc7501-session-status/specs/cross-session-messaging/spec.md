## ADDED Requirements

### Requirement: Delivery follows the root status when there is one

Every PTY delivery path (the send-path immediate wake, the just-idled deferred wake, the `Stop` drain, the worktree-outcome drain, and the immediate worktree-outcome wake) SHALL decide whether a target is busy with one rule. When the target has a root status (`program-status-protocol`), it SHALL be busy exactly when the root status is `blocked`, or when it is `working` and the hook-derived mode is `running` or `needs_attention`. When the target has no root status, today's rule SHALL apply: busy when the hook-derived mode is `running` or `needs_attention`. A busy target SHALL never receive a paste, so a wake note and its Enter never land inside a Claude Code dialog or a running turn.

#### Scenario: Folder-trust prompt queues the message

- **GIVEN** target S has hook-derived mode `running` (set when its PTY was reserved) and root status `blocked` with no `kind`
- **WHEN** a message is sent to S
- **THEN** the delivery status SHALL be `queued` and nothing SHALL be written to S's PTY

#### Scenario: Background work after Stop does not hold delivery

- **GIVEN** target S got a `Stop` (hook-derived mode `idle`) past the settle window and its root status is `working`
- **WHEN** a message is sent to S
- **THEN** the delivery status SHALL be `delivered`

#### Scenario: A done status ignores a stale running hook mode

- **GIVEN** target S has hook-derived mode `running` from `SessionStart` and root status `idle`, past the settle window
- **WHEN** a message is sent to S
- **THEN** the delivery status SHALL be `delivered`

#### Scenario: A worktree outcome is not pasted into a dialog

- **GIVEN** requesting session S has hook-derived mode `idle` and root status `blocked`
- **WHEN** a worktree request of S reaches an outcome
- **THEN** the outcome note SHALL be queued, not pasted

#### Scenario: The Stop drain skips a blocked target

- **GIVEN** a message for S is queued and S's root status is `blocked`
- **WHEN** a `Stop` hook arrives for S
- **THEN** the `Stop` drain SHALL NOT paste into S's PTY

### Requirement: A target that starts accepting a prompt is drained

When a root status change, or a hook event other than `Stop` and `SessionEnd`, makes a target go from busy to not busy (by the rule above), the system SHALL run the ready drain for that target once, after the just-idled settle window, unless the target is busy again by then, because a dialog that closes without a turn sends no `Stop`. The ready drain SHALL do what the `Stop` drain does: the cross-session drain, then the worktree-outcome drain only when the first pasted nothing (at most one paste per transition). The `Stop` drain SHALL keep running as today for targets that are not busy. A message SHALL therefore never stay queued while its target accepts a prompt and has had a busy-to-not-busy transition after the message was queued.

#### Scenario: Answering the folder-trust prompt delivers the queued message

- **GIVEN** a message for S is queued while S's root status is `blocked` and its hook-derived mode is `running`
- **WHEN** S's root status changes to `idle`, a `SessionStart` hook sets the hook-derived mode to `running`, and the settle window passes
- **THEN** the idle drain SHALL run for S once and paste the wake note

#### Scenario: The ready drain delivers a queued worktree outcome

- **GIVEN** a worktree outcome for S is queued, no cross-session message is pending, and S's root status is `blocked`
- **WHEN** S's root status changes to `idle` and the settle window passes
- **THEN** the worktree-outcome drain SHALL paste the outcome note

#### Scenario: A background agent completing drains the queue

- **GIVEN** S's root status is `working`, its hook-derived mode is `needs_attention` for a background agent wait, and a message for S is queued
- **WHEN** the `agent_completed` notification moves S's hook-derived mode to `idle` and the settle window passes
- **THEN** the ready drain SHALL run for S once

#### Scenario: A busy target at the end of the settle window is not pasted

- **GIVEN** S's root status changed from `blocked` to `idle` with a message queued
- **WHEN** S's root status becomes `working` with hook-derived mode `running` before the settle window ends
- **THEN** the drain SHALL NOT paste, and the message SHALL wait for S's next busy-to-not-busy transition

#### Scenario: Every combination keeps the invariant

- **WHEN** the busy rule is evaluated for every combination of root status (none, `working`, `blocked`, `done`, `idle`, `error`) and hook-derived mode (none, `running`, `idle`, `needs_attention`)
- **THEN** a combination with root `blocked` SHALL be busy
- **AND** a combination with root `done`, `idle` or `error` SHALL NOT be busy
- **AND** a combination with no root status SHALL match today's rule
