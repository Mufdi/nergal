## MODIFIED Requirements

### Requirement: Session switching shortcuts
The system SHALL provide `Ctrl+1..9` to switch to session N (locked). The target workspace SHALL prefer the sidebar-focused workspace when the sidebar owns focus (existing `numericTargetWorkspace` behavior), falling back to the active session's workspace. A `Ctrl+digit` that switches to, or resumes, a session SHALL set the focus zone to `terminal`, as a click on the session does. The system SHALL provide `Ctrl+Shift+1..9` to jump to project/workspace N (was `Ctrl+Alt+1..9`); jumping SHALL leave focus in the sidebar so a following `Ctrl+digit` targets the jumped-to workspace. The `focused-session-*` entries (`Ctrl+Shift+1..9` old semantics) are removed as redundant.

#### Scenario: Jump to project then pick a session
- **WHEN** the user presses `Ctrl+Shift+3` (project 3) and then `Ctrl+2`
- **THEN** session 2 of project 3 becomes active

#### Scenario: Ctrl+digit from terminal focus
- **WHEN** focus is in the terminal and the user presses `Ctrl+2`
- **THEN** session 2 of the active workspace becomes active

#### Scenario: First Ctrl+digit of a run moves the halo to the terminal
- **GIVEN** Nergal just started, no session is active, and the focus zone is `sidebar`
- **WHEN** the user presses `Ctrl+1` and session 1 resumes
- **THEN** the focus zone is `terminal`

#### Scenario: Ctrl+digit to the session that is already active
- **GIVEN** session 2 is active with a live terminal and the focus zone is `sidebar`
- **WHEN** the user presses `Ctrl+2`
- **THEN** the focus zone is `terminal` and the terminal has DOM focus

#### Scenario: Ctrl+digit to a live session from the sidebar
- **GIVEN** session 2 has a live terminal and the focus zone is `sidebar`
- **WHEN** the user presses `Ctrl+2`
- **THEN** session 2 becomes active and the focus zone is `terminal`

## ADDED Requirements

### Requirement: One-shot shortcut signals
The new-session, add-workspace and jump-to-project signals SHALL be cleared by the component that consumes them, at the moment it acts. A component that unmounts and mounts again SHALL NOT act on a signal it already consumed. A signal whose consumer cannot act yet because no workspaces are loaded SHALL NOT be cleared by that attempt. Once workspaces are loaded, a jump to an index with no workspace SHALL be cleared without acting. A press made while the sidebar is collapsed SHALL act once, when the sidebar expands.

#### Scenario: Re-expanding the sidebar does not open the folder picker
- **GIVEN** the user pressed `leader shift+w` earlier in the run and the folder picker opened
- **WHEN** the user collapses and re-expands the sidebar with `leader shift+b`
- **THEN** the workspace folder picker does not open

#### Scenario: Re-expanding the sidebar does not repeat a jump
- **GIVEN** the user jumped to a project with `Ctrl+Shift+1` and then focused the terminal
- **WHEN** the user collapses and re-expands the sidebar
- **THEN** the focus stays in the terminal

#### Scenario: A jump past the last workspace is dropped
- **GIVEN** three workspaces are loaded
- **WHEN** the user presses `Ctrl+Shift+5`, then adds two workspaces and collapses and re-expands the sidebar
- **THEN** no jump happens

#### Scenario: A press while collapsed acts once on expand
- **GIVEN** the sidebar is collapsed
- **WHEN** the user presses `leader shift+w`, then re-expands the sidebar twice
- **THEN** the folder picker opens once

#### Scenario: A new press still fires
- **GIVEN** the add-workspace signal was consumed once
- **WHEN** the user presses `leader shift+w` again
- **THEN** the folder picker opens

### Requirement: Keys yielded to the agent reach the PTY
When `keyboard_ownership` is `"agent"` and the event target is inside the terminal or quake zone, the dispatcher SHALL yield an app-scope key to the agent CLI: it SHALL mark the event, stop scanning the registry, and leave the event unprevented. The terminal SHALL send a marked key to the PTY, even when the combination is on its list of keys it leaves to Nergal. Keys that are not marked SHALL keep the current terminal behavior.

#### Scenario: Ctrl+B in agent mode reaches Claude Code
- **GIVEN** `keyboard_ownership` is `"agent"` and the focus is in the terminal
- **WHEN** the user presses `Ctrl+B`
- **THEN** the PTY receives `Ctrl+B` and the sidebar does not toggle

#### Scenario: Ctrl+B in nergal mode toggles the sidebar
- **GIVEN** `keyboard_ownership` is `"nergal"` and the focus is in the terminal
- **WHEN** the user presses `Ctrl+B`
- **THEN** the sidebar toggles and the PTY does not receive the key

#### Scenario: A target outside every zone is not yielded
- **GIVEN** `keyboard_ownership` is `"agent"`, the stored focus zone is `terminal`, and DOM focus is on `<body>`
- **WHEN** the user presses `Ctrl+B`
- **THEN** the sidebar toggles

#### Scenario: The event target decides the zone
- **GIVEN** `keyboard_ownership` is `"agent"`, the stored focus zone is `terminal`, and the event target is inside the right panel
- **WHEN** the user presses `Ctrl+W`
- **THEN** the key is not yielded and the active panel tab closes
