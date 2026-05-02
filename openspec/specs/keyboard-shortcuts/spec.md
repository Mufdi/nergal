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

#### Scenario: Ship session
- **WHEN** user presses Ctrl+Shift+Y (any focus zone)
- **THEN** the Ship preview dialog opens for the active session

#### Scenario: Push session
- **WHEN** user presses Ctrl+Alt+P (any focus zone)
- **THEN** Push is triggered for the active session if `ahead > 0`, otherwise a toast is shown

#### Scenario: Contextual revise/resolve
- **WHEN** user presses Ctrl+Shift+R and a conflict tab is active
- **THEN** the system emits `cluihud:resolve-conflict-active-tab`
- **WHEN** user presses Ctrl+Shift+R while the git panel is active and the session has conflicted files
- **THEN** the system opens or focuses the conflict tab for the first conflicted file
- **WHEN** user presses Ctrl+Shift+R while a plan/spec panel is active with a pending review
- **THEN** the system emits `cluihud:revise-plan` (prior behavior preserved)

### Requirement: Shortcut registry
All shortcuts SHALL be defined in a central registry (`stores/shortcuts.ts`) as an array of action objects containing: id, label, shortcut key combination, category, handler function, and optional keywords for command palette search.

#### Scenario: Registry contains all shortcuts
- **WHEN** the app initializes
- **THEN** the shortcut registry contains entries for all defined shortcuts with their key combinations and handlers

### Requirement: Ship action shortcut (global)
The system SHALL bind `Ctrl+Shift+Y` to the Ship action. From any focus zone (terminal, sidebar, panel), pressing this combination SHALL open the Ship preview dialog for the active session.

#### Scenario: Ship shortcut fires from terminal zone
- **WHEN** focus is in terminal and user presses `Ctrl+Shift+Y`
- **THEN** the shortcut is intercepted (via `attachCustomKeyEventHandler` returning false) and the Ship preview dialog opens

#### Scenario: Ship shortcut fires from panel zone
- **WHEN** focus is in the right panel and user presses `Ctrl+Shift+Y`
- **THEN** the Ship preview dialog opens

#### Scenario: Registry entry exists
- **WHEN** the shortcut registry is initialized
- **THEN** an entry exists with `id: "ship"`, `keys: "ctrl+shift+y"`, `category: "action"`, keywords including `["ship", "pr", "push", "commit", "deploy"]`

### Requirement: Push action shortcut (global)
The system SHALL bind `Ctrl+Alt+P` to the explicit Push action (push-only, no commit, no PR).

#### Scenario: Push shortcut pushes when ahead
- **WHEN** active session has `ahead > 0` and user presses `Ctrl+Alt+P`
- **THEN** the system triggers Push without opening a dialog

#### Scenario: Push shortcut toasts when nothing to push
- **WHEN** active session has `ahead === 0` and user presses `Ctrl+Alt+P`
- **THEN** a toast "Nothing to push" is shown

#### Scenario: Registry entry exists
- **WHEN** the shortcut registry is initialized
- **THEN** an entry exists with `id: "push"`, `keys: "ctrl+alt+p"`, `category: "action"`, keywords including `["push", "upload", "remote"]`

### Requirement: Ship-Enter contextual shortcut (git panel textarea)
Within the git panel commit textarea, `Ctrl+Shift+Enter` SHALL trigger Ship using the textarea contents. This binding is local to the textarea and does NOT appear in the global shortcut registry.

#### Scenario: Ship-Enter with message and staged
- **WHEN** commit textarea has a non-empty message, there are staged files, and user presses `Ctrl+Shift+Enter`
- **THEN** Ship proceeds (commit + push + PR preview) using the textarea message

#### Scenario: Ship-Enter with empty message opens dialog
- **WHEN** commit textarea is empty and user presses `Ctrl+Shift+Enter`
- **THEN** the global Ship preview dialog opens (same as `Ctrl+Shift+Y`)

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

