## ADDED Requirements

### Requirement: Layout presets define panel proportions
The system SHALL support three layout presets: `terminal-focus` (sidebar 15%, center 85%, right collapsed to 40px), `doc-review` (sidebar 15%, center 50%, right 35%), and `tool-workspace` (sidebar auto-collapsed to 40px, center 30%, right 55%).

#### Scenario: Terminal-focus when no panel is open
- **WHEN** no right panel is open (all panels closed)
- **THEN** center column SHALL occupy 85% of available width and right panel SHALL collapse to 40px

#### Scenario: Doc-review when opening a document viewer
- **WHEN** user opens a document panel (plan, spec, transcript, or file)
- **THEN** center column SHALL resize to 50% and right panel SHALL expand to 35%

#### Scenario: Tool-workspace when opening a tool panel
- **WHEN** user opens a tool panel (git or diff)
- **THEN** center column SHALL resize to 30%, right panel SHALL expand to 55%, and left sidebar SHALL auto-collapse to 40px icon strip

### Requirement: Animated transitions between presets
The system SHALL animate panel size transitions with a 200-300ms ease curve when switching between layout presets.

#### Scenario: Smooth resize on panel open
- **WHEN** layout preset changes (e.g., terminal-focus → tool-workspace)
- **THEN** panel sizes SHALL transition smoothly over 200-300ms, not snap instantly

#### Scenario: No animation during manual drag
- **WHEN** user is actively dragging a resize handle
- **THEN** the system SHALL disable transition animations until the drag ends

### Requirement: Terminal minimum size guarantee
The system SHALL enforce a minimum size of 25% for the terminal center panel. The terminal SHALL never be fully collapsed or hidden in normal layout mode.

#### Scenario: Terminal visible in tool-workspace mode
- **WHEN** layout is in `tool-workspace` preset (center at 30%)
- **THEN** the terminal panel SHALL be visible with at least 25% of total width

#### Scenario: Manual resize respects minimum
- **WHEN** user drags the resize handle to shrink the center panel below 25%
- **THEN** the center panel SHALL stop at 25% minimum and not shrink further

### Requirement: Deterministic splits via shortcuts
The system SHALL provide keyboard shortcuts to switch between layout presets directly. Splits SHALL be preset-driven and predictable, not free-form drag-dependent.

#### Scenario: Shortcut cycles presets
- **WHEN** user presses the layout cycle shortcut
- **THEN** the system SHALL cycle through presets in order: terminal-focus → doc-review → tool-workspace → terminal-focus

#### Scenario: Preset shortcut overrides manual adjustments
- **WHEN** user has manually adjusted panel sizes and presses a preset shortcut
- **THEN** the layout SHALL snap to the target preset proportions

### Requirement: Manual override within presets
The system SHALL allow users to manually drag resize handles to adjust proportions after a preset is applied. The manual adjustment SHALL persist until a new preset is triggered by opening a different-category panel or using a preset shortcut.

#### Scenario: User adjusts after preset
- **WHEN** a preset has been applied and user drags a resize handle
- **THEN** the panel sizes SHALL follow the drag position (respecting min/max bounds)

#### Scenario: New panel category resets to preset
- **WHEN** user has manually adjusted sizes and opens a different-category panel
- **THEN** the layout SHALL snap to the new panel's category preset

### Requirement: Layout preset persisted per session
The system SHALL store the current layout preset for each session. When switching between sessions, the layout SHALL restore to the target session's last preset.

#### Scenario: Session switch restores layout
- **WHEN** user switches from session A (in tool-workspace) to session B (in doc-review)
- **THEN** the layout SHALL transition to doc-review proportions

#### Scenario: New session defaults to terminal-focus
- **WHEN** a new session is created with no panels open
- **THEN** the layout SHALL default to terminal-focus preset

### Requirement: Left sidebar auto-collapse in tool-workspace
The system SHALL auto-collapse the left sidebar to a 40px icon strip when entering `tool-workspace` preset. The sidebar SHALL restore to its previous size when leaving tool-workspace or when manually expanded via Ctrl+B.

#### Scenario: Sidebar collapses for tool panels
- **WHEN** layout enters `tool-workspace` preset
- **THEN** left sidebar SHALL collapse to 40px icon strip

#### Scenario: Sidebar restores on preset change
- **WHEN** layout transitions from `tool-workspace` to `doc-review` or `terminal-focus`
- **THEN** left sidebar SHALL restore to its previous expanded size (15%)

#### Scenario: Manual expand overrides auto-collapse
- **WHEN** sidebar is auto-collapsed in tool-workspace and user presses Ctrl+B
- **THEN** sidebar SHALL expand, and auto-collapse SHALL not re-trigger until next preset change

### Requirement: Layout mode indicator in status bar
The system SHALL display the current layout preset name in the status bar.

#### Scenario: Status bar shows current mode
- **WHEN** layout transitions to a new preset
- **THEN** the status bar SHALL display the preset name
