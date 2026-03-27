## ADDED Requirements

### Requirement: Create workspace from folder
The system SHALL allow users to create a workspace by selecting a directory that contains a git repository. The workspace name SHALL be the directory basename. The workspace id SHALL be a deterministic hash of the absolute path.

#### Scenario: User adds workspace via folder picker
- **WHEN** user clicks "+" button in workspaces sidebar header
- **THEN** system opens native OS folder picker dialog
- **AND** upon selection, creates a Workspace record with the selected path as `repo_path`
- **AND** the workspace appears in the sidebar

#### Scenario: Selected folder is not a git repo
- **WHEN** user selects a folder that does not contain a `.git` directory
- **THEN** system shows an error toast "Not a git repository"
- **AND** no workspace is created

#### Scenario: Workspace already exists for path
- **WHEN** user selects a folder that already has a workspace
- **THEN** system activates the existing workspace instead of creating a duplicate

### Requirement: Persist workspaces across restarts
The system SHALL persist all workspace and session metadata to `~/.config/cluihud/state.json`. On app startup, the system SHALL load this state and restore the sidebar.

#### Scenario: App restart preserves workspaces
- **WHEN** user restarts cluihud
- **THEN** all previously created workspaces appear in the sidebar with their sessions
- **AND** session status is set to `idle` (PTYs are not preserved across restarts)

### Requirement: List workspaces with sessions
The system SHALL display workspaces in the sidebar as an expandable tree. Each workspace shows its name and a chevron to expand/collapse its session list.

#### Scenario: Expanded workspace shows sessions
- **WHEN** user expands a workspace in the sidebar
- **THEN** system shows all sessions under that workspace with their names and relative age

### Requirement: Delete workspace
The system SHALL allow users to delete a workspace, which removes all its sessions (including worktrees) and the workspace record.

#### Scenario: Delete workspace with active sessions
- **WHEN** user deletes a workspace that has running sessions
- **THEN** system kills all PTYs, removes all worktrees, removes workspace from state
