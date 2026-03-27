## Purpose

Provide a full-viewport overlay for side-by-side diff review with backdrop blur, persistent git sidebar, and file navigation controls.

## Requirements

### Requirement: Zen Mode overlay for diff review
The system SHALL provide a Zen Mode that renders a full-viewport overlay with side-by-side diff content over the terminal area, applying a CSS backdrop-filter blur to the terminal underneath.

#### Scenario: Activate from git panel file click
- **WHEN** user clicks a file in the git panel sidebar
- **THEN** Zen Mode SHALL activate, showing the file's side-by-side diff in the overlay with the terminal blurred behind

#### Scenario: Activate from inline diff expand
- **WHEN** user clicks "expand" on an inline diff view
- **THEN** Zen Mode SHALL activate with the corresponding file's side-by-side diff

#### Scenario: Activate from commit graph
- **WHEN** user clicks a commit in the git history graph
- **THEN** Zen Mode SHALL activate showing that commit's changes

#### Scenario: Exit via Esc
- **WHEN** Zen Mode is active and user presses Esc
- **THEN** Zen Mode SHALL deactivate, removing the overlay and restoring the terminal from blur

### Requirement: Git sidebar visible in Zen Mode
The system SHALL keep the git sidebar (staged/unstaged/stashed files) visible alongside the diff overlay in Zen Mode, allowing file navigation without exiting.

#### Scenario: Navigate files within Zen Mode
- **WHEN** Zen Mode is active and user clicks a different file in the git sidebar
- **THEN** the diff content SHALL update to show the newly selected file without exiting Zen Mode

#### Scenario: Non-sequential file navigation
- **WHEN** user wants to review the 2nd file then the 8th file
- **THEN** user SHALL click each file directly in the git sidebar without needing to exit and re-enter Zen Mode

### Requirement: Zen Mode z-index layering
Zen Mode overlay SHALL render above the terminal and main content but below modals, command palette, and toast notifications.

#### Scenario: Command palette over Zen Mode
- **WHEN** Zen Mode is active and user opens the command palette (Ctrl+K)
- **THEN** the command palette SHALL render above the Zen Mode overlay

#### Scenario: Toast notifications over Zen Mode
- **WHEN** Zen Mode is active and a toast notification fires
- **THEN** the toast SHALL render above the Zen Mode overlay

### Requirement: Zen Mode file navigation controls
The system SHALL provide prev/next file navigation within Zen Mode via keyboard shortcuts and visible controls.

#### Scenario: Navigate to next file
- **WHEN** Zen Mode is active and user presses the next-file shortcut
- **THEN** the diff SHALL advance to the next file in the git sidebar list

#### Scenario: File position indicator
- **WHEN** Zen Mode is active
- **THEN** the overlay SHALL display the current file name and position (e.g., "auth.ts (2/5)")
