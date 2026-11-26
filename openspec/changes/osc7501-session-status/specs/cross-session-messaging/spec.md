## ADDED Requirements

### Requirement: A blocked root status holds delivery

The send-path immediate wake and the just-idled deferred wake SHALL treat a target as busy when its root status (`program-status-protocol`) is `blocked`, in addition to today's hook-derived `running` and `needs_attention`, so a wake note and its Enter never land inside a Claude Code dialog such as the folder-trust prompt. A root status `working` SHALL NOT make the target busy for delivery, because after `Stop` the prompt accepts input while background work runs. When a target's root status changes from `blocked` to any other state, or is removed, the system SHALL run the idle drain for that target once, after the just-idled settle window, unless the target is busy by then, because no `Stop` follows a dialog that closes without a turn.

#### Scenario: Folder-trust prompt queues the message

- **GIVEN** target S has no hook-derived activity and root status `blocked` with no `kind`
- **WHEN** a message is sent to S
- **THEN** the delivery status SHALL be `queued` and nothing SHALL be written to S's PTY

#### Scenario: Closing the dialog delivers the queued message

- **GIVEN** a message for S is queued while S's root status is `blocked`
- **WHEN** S's root status changes to `idle` and the settle window passes with S not busy
- **THEN** the idle drain SHALL run for S once and paste the wake note

#### Scenario: Background work after Stop does not hold delivery

- **GIVEN** target S got a `Stop` (hook-derived mode `idle`) past the settle window and its root status is `working`
- **WHEN** a message is sent to S
- **THEN** the delivery status SHALL be `delivered`

#### Scenario: A busy target at the end of the settle window is not pasted

- **GIVEN** S's root status changed from `blocked` to `idle` with a message queued
- **WHEN** S's hook-derived mode becomes `running` before the settle window ends
- **THEN** the drain SHALL NOT paste, and the message SHALL wait for S's next `Stop`
