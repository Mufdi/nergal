## ADDED Requirements

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

### Requirement: SessionRow unread badge

The system SHALL show a lightweight unread indicator on a `SessionRow` when that session has unread cross-session messages, reusing the existing pending-indicator pattern. The badge SHALL clear when the messages are read.

#### Scenario: Badge appears on new message

- **WHEN** a session receives a new cross-session message it has not read
- **THEN** its `SessionRow` SHALL show the unread badge

#### Scenario: Badge clears on read

- **WHEN** the session's unread messages are read (the agent calls `read_messages` or the user opens the thread)
- **THEN** the unread badge SHALL clear
