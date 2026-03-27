## ADDED Requirements

### Requirement: Panel type classification
The system SHALL classify each panel type into exactly one category: `document` (plan, spec, transcript, file) or `tool` (git, diff).

#### Scenario: Document panels classified correctly
- **WHEN** user opens a plan, spec, transcript, or file panel
- **THEN** the system SHALL identify it as category `document`

#### Scenario: Tool panels classified correctly
- **WHEN** user opens a git or diff panel
- **THEN** the system SHALL identify it as category `tool`

### Requirement: Category determines layout preset
The system SHALL use the active panel's category to select the appropriate layout preset: `document` → `doc-review`, `tool` → `tool-workspace`, no panel → `terminal-focus`.

#### Scenario: Opening git panel triggers tool-workspace
- **WHEN** user opens the git panel (category: tool)
- **THEN** the system SHALL apply the `tool-workspace` layout preset

#### Scenario: Switching from git to plan changes preset
- **WHEN** user switches from git panel (tool) to plan panel (document)
- **THEN** the system SHALL transition from `tool-workspace` to `doc-review` preset

#### Scenario: Closing all panels triggers terminal-focus
- **WHEN** user closes the last open panel
- **THEN** the system SHALL apply the `terminal-focus` layout preset

### Requirement: Category metadata on panel types
The system SHALL store a `category` field on each panel/tab type definition. The category SHALL be used by the layout system to derive the target preset.

#### Scenario: Tab state includes category
- **WHEN** a new tab or panel view is created
- **THEN** the tab/panel metadata SHALL include a `category` field set to `"document"` or `"tool"`

### Requirement: Contextual right sidebar navigation
The right sidebar SHALL display navigation content relevant to the active panel type. The sidebar SHALL be collapsible via keyboard shortcut.

#### Scenario: Git panel shows staged/unstaged files
- **WHEN** git panel is active
- **THEN** right sidebar SHALL show staged, unstaged, and stashed file sections

#### Scenario: File browser shows project tree
- **WHEN** file browser panel is active
- **THEN** right sidebar SHALL show the project directory tree

#### Scenario: Plan panel shows plan files
- **WHEN** plan panel is active
- **THEN** right sidebar SHALL show the list of plan files (existing behavior)

#### Scenario: Sidebar collapsible via shortcut
- **WHEN** user presses the sidebar collapse shortcut
- **THEN** right sidebar SHALL toggle between collapsed and expanded states
