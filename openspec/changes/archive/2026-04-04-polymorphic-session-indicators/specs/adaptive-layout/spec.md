## MODIFIED Requirements

### Requirement: Layout presets define panel proportions
The system SHALL support three layout presets: `terminal-focus` (sidebar 15%, center 85%, right collapsed to 40px), `doc-review` (sidebar 15%, center 50%, right 35%), and `tool-workspace` (sidebar auto-collapsed to 32px, center 30%, right 55%).

The collapsed sidebar width SHALL be 32px (previously 40px icon strip) to accommodate polymorphic session indicators at legible size while minimizing horizontal space usage.

#### Scenario: Terminal-focus when no panel is open
- **WHEN** no right panel is open (all panels closed)
- **THEN** center column SHALL occupy 85% of available width and right panel SHALL collapse to 40px

#### Scenario: Doc-review when opening a document viewer
- **WHEN** user opens a document panel (plan, spec, transcript, or file)
- **THEN** center column SHALL resize to 50% and right panel SHALL expand to 35%

#### Scenario: Tool-workspace when opening a tool panel
- **WHEN** user opens a tool panel (git or diff)
- **THEN** center column SHALL resize to 30%, right panel SHALL expand to 55%, and left sidebar SHALL auto-collapse to 32px with polymorphic session indicators visible

#### Scenario: Collapsed sidebar shows session indicators
- **WHEN** sidebar is collapsed (manually via Ctrl+B or auto-collapsed in tool-workspace)
- **THEN** the sidebar renders at 32px width with polymorphic session indicators for each active session
- **AND** each indicator is 10px with sufficient padding for animation overshoot
