## ADDED Requirements

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

## MODIFIED Requirements

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
