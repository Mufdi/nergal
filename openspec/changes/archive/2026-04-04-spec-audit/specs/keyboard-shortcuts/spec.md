---
status: archived
implemented: 2026-03-25
archived: 2026-04-04
files:
  - src/stores/shortcuts.ts
  - src/hooks/useKeyboardShortcuts.ts
  - src/stores/layout.ts
---

## Purpose

Define focus zone tracking, terminal bypass, and a central shortcut registry for navigation, session switching, tab management, and panel actions.

## Implementation Notes

All requirements implemented. Known deviations:
- **Terminal bypass**: Shortcuts still fire when terminal is focused (no focusZone gate in handler). Only Ctrl+Ñ/Ctrl+; are explicitly handled via `attachCustomKeyEventHandler`. Practical impact minimal since xterm.js `attachCustomKeyEventHandler` returns false for cluihud shortcuts.
- **Ctrl+K in terminal**: No explicit terminal zone check — toggle happens regardless of focus zone.

## Requirements

### Requirement: Focus zone tracking
The system SHALL track the current focus zone as one of: sidebar, terminal, panel. A Jotai atom SHALL hold the current zone. Focus zone SHALL update when user clicks in a zone or uses zone-switching shortcuts.

#### Scenario: Click in terminal sets zone
- **WHEN** user clicks inside the terminal area
- **THEN** focusZoneAtom is set to "terminal"

#### Scenario: Click in right panel sets zone
- **WHEN** user clicks inside the right panel
- **THEN** focusZoneAtom is set to "panel"

### Requirement: Terminal focus bypass
When focus zone is "terminal", ALL keyboard events SHALL pass through to the PTY via xterm.js. No cluihud shortcuts SHALL be intercepted in terminal zone EXCEPT the terminal escape shortcut (Ctrl+Ñ / Ctrl+; on US layout).

#### Scenario: Ctrl+Shift+P in terminal passes to PTY
- **WHEN** focus is in terminal and user presses Ctrl+Shift+P
- **THEN** the keystroke is sent to the PTY, command palette does NOT open

#### Scenario: Ctrl+Ñ escapes terminal
- **WHEN** focus is in terminal and user presses Ctrl+Ñ (Ctrl+; on US layout)
- **THEN** focus moves to the last active non-terminal zone

### Requirement: Navigation shortcuts
The system SHALL provide shortcuts for navigating between zones and toggling panels.

#### Scenario: Toggle sidebar
- **WHEN** user presses Ctrl+B (outside terminal)
- **THEN** the left sidebar toggles between collapsed and expanded

#### Scenario: Toggle right panel
- **WHEN** user presses Ctrl+Shift+B (outside terminal)
- **THEN** the right panel toggles between collapsed and expanded

#### Scenario: Move focus between zones
- **WHEN** user presses Alt+Left or Alt+Right (outside terminal)
- **THEN** focus moves to the adjacent zone (sidebar ↔ terminal ↔ panel)

#### Scenario: Focus terminal
- **WHEN** user presses Ctrl+Ñ from any zone
- **THEN** focus moves to terminal and xterm.js receives focus

### Requirement: Session switching shortcuts
The system SHALL provide Ctrl+1 through Ctrl+9 to switch to session N of the active workspace (by order in the session list).

#### Scenario: Switch to session 2
- **WHEN** workspace has 3 sessions and user presses Ctrl+2 (outside terminal)
- **THEN** session 2 becomes active, terminal and panels update to that session

#### Scenario: No-op for non-existent session
- **WHEN** workspace has 2 sessions and user presses Ctrl+5
- **THEN** nothing happens

### Requirement: Tab navigation shortcuts
The system SHALL provide shortcuts for navigating and managing tabs in the right panel.

#### Scenario: Next tab
- **WHEN** user presses Ctrl+Tab (outside terminal)
- **THEN** the next tab in the tab bar becomes active

#### Scenario: Previous tab
- **WHEN** user presses Ctrl+Shift+Tab (outside terminal)
- **THEN** the previous tab in the tab bar becomes active

#### Scenario: Close active tab
- **WHEN** user presses Ctrl+W (outside terminal)
- **THEN** the active tab is closed (with unsaved confirmation if needed)

#### Scenario: Reopen closed tab
- **WHEN** user presses Ctrl+Shift+T (outside terminal)
- **THEN** the most recently closed tab reopens

### Requirement: Panel opening shortcuts
The system SHALL provide Ctrl+Shift+{letter} shortcuts to open or focus specific panel types.

#### Scenario: Open plan panel
- **WHEN** user presses Ctrl+Shift+P (outside terminal)
- **THEN** Plan tab opens or is focused in right panel

#### Scenario: Open files panel
- **WHEN** user presses Ctrl+Shift+F (outside terminal)
- **THEN** Files tab opens or is focused in right panel

#### Scenario: Open diff panel
- **WHEN** user presses Ctrl+Shift+D (outside terminal)
- **THEN** Diff tab opens or is focused in right panel

#### Scenario: Open spec panel
- **WHEN** user presses Ctrl+Shift+S (outside terminal)
- **THEN** Spec tab opens or is focused in right panel

#### Scenario: Open git panel
- **WHEN** user presses Ctrl+Shift+G (outside terminal)
- **THEN** Git tab opens or is focused in right panel

#### Scenario: Open tasks panel
- **WHEN** user presses Ctrl+Shift+K (outside terminal)
- **THEN** Tasks tab opens or is focused in right panel

#### Scenario: Toggle activity log
- **WHEN** user presses Ctrl+Shift+L (outside terminal)
- **THEN** the activity log panel toggles between collapsed and expanded

### Requirement: Action shortcuts
The system SHALL provide shortcuts for common actions.

#### Scenario: Open in IDE
- **WHEN** user presses Ctrl+Shift+E (outside terminal)
- **THEN** the active session's working directory opens in the configured IDE

#### Scenario: Merge session
- **WHEN** user presses Ctrl+Shift+M (outside terminal)
- **THEN** the merge modal opens for the active session

#### Scenario: Commit session
- **WHEN** user presses Ctrl+Shift+C (outside terminal)
- **THEN** the commit modal opens for the active session

### Requirement: Shortcut registry
All shortcuts SHALL be defined in a central registry (`stores/shortcuts.ts`) as an array of action objects containing: id, label, shortcut key combination, category, handler function, and optional keywords for command palette search.

#### Scenario: Registry contains all shortcuts
- **WHEN** the app initializes
- **THEN** the shortcut registry contains entries for all defined shortcuts with their key combinations and handlers
