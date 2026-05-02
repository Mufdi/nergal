## ADDED Requirements

### Requirement: Scratchpad toggle shortcut

The system SHALL bind `Ctrl+Alt+L` to toggle the scratchpad floating panel. `Ctrl+L` SHALL remain reserved for the terminal's `clear-screen` sequence and SHALL NOT be intercepted by the application.

#### Scenario: Toggle scratchpad from any zone

- **WHEN** focus is anywhere outside the terminal and the user presses `Ctrl+Alt+L`
- **THEN** the scratchpad floating panel SHALL open or close

#### Scenario: Ctrl+L preserved for terminal

- **WHEN** focus is in the terminal and the user presses `Ctrl+L`
- **THEN** the keystroke SHALL reach the PTY and trigger the shell's `clear-screen` behavior

### Requirement: Context-scoped tab shortcuts inside scratchpad

When the scratchpad panel is open AND focus is contained within the panel subtree, the system SHALL hijack tab-management shortcuts so they operate on scratchpad tabs instead of session or right-panel tabs. The hijack SHALL be scoped via DOM containment (target inside `[data-floating-panel-id="scratchpad"]`).

#### Scenario: Ctrl+Tab cycles scratchpad tabs forward

- **WHEN** the scratchpad is focused and the user presses `Ctrl+Tab`
- **THEN** the next scratchpad tab SHALL become active (with wrap-around)
- **AND** the session and right-panel tab cycles SHALL NOT advance

#### Scenario: Ctrl+Shift+Tab cycles scratchpad tabs backward

- **WHEN** the scratchpad is focused and the user presses `Ctrl+Shift+Tab`
- **THEN** the previous scratchpad tab SHALL become active (with wrap-around)

#### Scenario: Ctrl+T creates a new scratchpad tab

- **WHEN** the scratchpad is focused and the user presses `Ctrl+T`
- **THEN** a new scratch tab SHALL be created and focused

#### Scenario: Ctrl+W closes the active scratchpad tab

- **WHEN** the scratchpad is focused and the user presses `Ctrl+W`
- **THEN** the active scratchpad tab SHALL be soft-deleted to `.trash/`
- **AND** the session and right-panel tab close handlers SHALL NOT fire

#### Scenario: Ctrl+Shift+T restores the last closed scratchpad tab

- **WHEN** the scratchpad is focused and the user presses `Ctrl+Shift+T`
- **AND** the in-memory close stack contains at least one tab id
- **THEN** the most recently closed scratchpad tab SHALL be restored from `.trash/`

#### Scenario: Other shortcuts remain global while scratchpad is focused

- **WHEN** the scratchpad is focused and the user presses `Ctrl+B`, `Ctrl+1..9`, or `Ctrl+Alt+L`
- **THEN** those shortcuts SHALL fire with their global behavior (sidebar toggle, session switch, panel toggle)
