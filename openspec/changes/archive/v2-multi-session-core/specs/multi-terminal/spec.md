## ADDED Requirements

### Requirement: Render multiple terminals simultaneously
The system SHALL render one xterm.js Terminal instance per active session. All terminals SHALL exist in the DOM simultaneously as hidden divs. Only the active session's terminal SHALL be visible.

#### Scenario: Two sessions active
- **WHEN** session A and session B both have PTYs running
- **THEN** both xterm.js instances exist in the DOM
- **AND** only the active session's terminal has `display: block`
- **AND** the other terminal has `display: none`

### Requirement: Instant terminal switching
Terminal switching SHALL complete in under 50ms by toggling CSS visibility rather than destroying/recreating Terminal instances.

#### Scenario: Switch between terminals
- **WHEN** user switches from session A to session B
- **THEN** session A's terminal div becomes `display: none`
- **AND** session B's terminal div becomes `display: block`
- **AND** session B's terminal calls `fitAddon.fit()` to adjust to current panel size
- **AND** scroll history and WebGL context are preserved

### Requirement: Lazy terminal creation
The system SHALL create a Terminal instance only when a session is first activated (not when the session is created). This limits memory usage.

#### Scenario: Session created but not yet activated
- **WHEN** a new session is created
- **THEN** no Terminal instance or PTY is created until the user switches to that session

### Requirement: Terminal cleanup on session delete
The system SHALL dispose the xterm.js Terminal instance and kill the PTY when a session is deleted.

#### Scenario: Delete session with active terminal
- **WHEN** user deletes a session that has a running terminal
- **THEN** system calls `terminal.dispose()` and `pty_kill(pty_id)`
- **AND** removes the terminal div from the DOM
