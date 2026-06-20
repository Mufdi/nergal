# cross-session-history-ui Specification

## Purpose
TBD - created by archiving change cross-session-messaging. Update Purpose after archive.
## Requirements
### Requirement: Cross-session history right panel

The system SHALL provide a dedicated right-panel view ("Cross-session") that renders the persistent cross-session message history as a navigable thread list plus thread detail. It SHALL be reachable via a TopBar icon and a keyboard shortcut (bound respecting the `event.code` convention; verify `src/stores/shortcuts.ts` for collisions). The history SHALL be reviewable at any time, independent of whether the involved sessions are still active.

#### Scenario: Open the panel and read a past thread

- **WHEN** the user opens the Cross-session panel
- **THEN** it SHALL show the list of threads, and selecting one SHALL show its messages in order

#### Scenario: Thread detail shows provenance

- **WHEN** a thread is open
- **THEN** each message SHALL show its sender session, workspace, timestamp, and a hop indicator, and the thread SHALL show its status (active/closed)

#### Scenario: History survives session closure

- **WHEN** a session that participated in a thread is later closed
- **THEN** that thread SHALL remain fully viewable in the panel

### Requirement: SessionRow unread badge (human-seen, independent of agent delivery)

The system SHALL show a lightweight unread indicator on a `SessionRow` when that session has cross-session messages the **user** has not yet seen, reusing the existing pending-indicator pattern. The badge SHALL track `human_seen_at` only and SHALL clear when the user opens the thread. It SHALL NOT key off `agent_consumed_at`, and opening the thread SHALL NOT set `agent_consumed_at` — the UI must never cancel a pending agent delivery.

#### Scenario: Badge appears on new message

- **WHEN** a session receives a new cross-session message the user has not seen
- **THEN** its `SessionRow` SHALL show the unread badge

#### Scenario: Badge clears on human view without cancelling delivery

- **WHEN** the user opens the thread
- **THEN** the badge SHALL clear (setting `human_seen_at`), and any pending agent delivery SHALL remain active (its `agent_consumed_at` is untouched)

