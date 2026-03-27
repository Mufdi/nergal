## Purpose

Provide a git panel with commit history graph, staged/unstaged/stashed file sidebar, and commit/push/PR actions.

## Requirements

### Requirement: Git panel two-area layout
The system SHALL render the git panel with a main content area (history/timeline), a sidebar (staged/unstaged/stashed files), and a fixed commit bar at the bottom.

#### Scenario: Git panel layout renders correctly
- **WHEN** user opens the git panel
- **THEN** the panel SHALL show history/timeline in the main area, file sections in the sidebar, and commit controls fixed at the bottom

### Requirement: Session commit history with graph
The main area of the git panel SHALL display the session's commit history with two toggleable view modes: graph view (visual commit graph with branches/merges) and list view (simple commit list with message + timestamp).

#### Scenario: Graph view shows visual commit tree
- **WHEN** graph view is active
- **THEN** the main area SHALL render a visual commit graph showing branches, merges, and commit messages for the session

#### Scenario: List view shows simple history
- **WHEN** list view is active
- **THEN** the main area SHALL render a chronological list of commits with message, timestamp, and short hash

#### Scenario: Toggle between views
- **WHEN** user clicks the view toggle button in the history header
- **THEN** the view SHALL switch between graph and list modes

#### Scenario: Commit click opens Zen Mode
- **WHEN** user clicks a commit in either view mode
- **THEN** Zen Mode SHALL activate showing that commit's diff

### Requirement: Git sidebar with staged/unstaged/stashed sections
The git sidebar SHALL display three collapsible sections: Staged, Unstaged, and Stashed. Each section SHALL list files with stage/unstage actions.

#### Scenario: Staged files section
- **WHEN** there are staged files
- **THEN** the Staged section SHALL list each file with an option to unstage

#### Scenario: Unstaged files section
- **WHEN** there are unstaged modified files
- **THEN** the Unstaged section SHALL list each file with an option to stage

#### Scenario: Stashed section
- **WHEN** there are stashed changes
- **THEN** the Stashed section SHALL list stash entries with apply/drop options

#### Scenario: File click opens Zen Mode diff
- **WHEN** user clicks a file in any sidebar section
- **THEN** Zen Mode SHALL activate showing that file's diff

### Requirement: Fixed commit bar
The git panel SHALL render a fixed commit bar at the bottom with a commit message input and action buttons.

#### Scenario: Commit action
- **WHEN** user types a commit message and clicks Commit
- **THEN** the system SHALL create a git commit with the staged files and provided message

#### Scenario: Push action
- **WHEN** user clicks Push
- **THEN** the system SHALL push the current branch to the remote

#### Scenario: Create PR action
- **WHEN** user clicks PR
- **THEN** the system SHALL initiate a pull request creation flow

### Requirement: Git sidebar persists in Zen Mode
When Zen Mode is activated from the git panel, the git sidebar SHALL remain visible alongside the diff overlay for continuous file navigation.

#### Scenario: Sidebar visible during Zen Mode
- **WHEN** Zen Mode is activated from a git panel file click
- **THEN** the git sidebar SHALL remain visible to the right of the diff overlay

#### Scenario: File navigation updates diff
- **WHEN** user clicks a different file in the git sidebar during Zen Mode
- **THEN** the diff overlay content SHALL update to the newly selected file
