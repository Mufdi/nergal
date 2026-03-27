## ADDED Requirements

### Requirement: Status bar activity line
The system SHALL display a single-line activity summary in the status bar showing the last tool action, total action count, and elapsed session time.

#### Scenario: Status bar shows last action
- **WHEN** the agent executes a tool call
- **THEN** the status bar SHALL update to show the tool name and target (e.g., "⚡ Write src/auth.ts │ 12 actions │ 2m 34s")

#### Scenario: Status bar clickable to expand drawer
- **WHEN** user clicks the activity section of the status bar
- **THEN** the activity timeline drawer SHALL expand from the bottom

### Requirement: Activity timeline drawer
The system SHALL provide an expandable drawer (~30% viewport height) that slides up from the status bar, containing a visual timeline strip and an event list with expandable thinking blocks.

#### Scenario: Drawer expands on click or shortcut
- **WHEN** user clicks the status bar activity section or presses the timeline shortcut
- **THEN** a drawer SHALL slide up from the bottom occupying ~30% of viewport height

#### Scenario: Visual timeline strip
- **WHEN** the drawer is expanded
- **THEN** the drawer SHALL display a horizontal timeline with dots representing events, scrubable by click/drag

#### Scenario: Event list with thinking blocks
- **WHEN** the drawer is expanded
- **THEN** each event SHALL show timestamp, tool name, and target, with an expandable `[thinking ▾]` section for agent reasoning blocks

#### Scenario: Thinking blocks from transcript
- **WHEN** an event has associated thinking/reasoning content in the transcript
- **THEN** expanding `[thinking ▾]` SHALL display the agent's reasoning text for that action

#### Scenario: Open as full tab
- **WHEN** user clicks the `[↗ Tab]` button in the drawer
- **THEN** the activity view SHALL open as a tab in the right panel with the DAG graph visualization

### Requirement: DAG graph visualization
The system SHALL provide a directed acyclic graph (DAG) view of tool-call chains as a tab in the right panel, using React Flow for node rendering.

#### Scenario: DAG tab shows tool-call chain
- **WHEN** user opens the DAG tab (via drawer button or panel shortcut)
- **THEN** the right panel SHALL display a React Flow graph where nodes represent tool calls and edges represent execution sequence

#### Scenario: Node details expandable
- **WHEN** user clicks a node in the DAG graph
- **THEN** the node SHALL expand to show tool name, target, duration, exit status, and thinking block if available

#### Scenario: DAG updates in real-time
- **WHEN** the agent executes new tool calls during an active session
- **THEN** the DAG SHALL append new nodes and edges in real-time

### Requirement: Activity log panel removed from center column
The system SHALL remove the permanent activity log panel from the center column vertical split. The terminal SHALL occupy the full center column height.

#### Scenario: Terminal occupies full center height
- **WHEN** no drawer is expanded
- **THEN** the terminal SHALL occupy 100% of the center column height (no activity log split)

#### Scenario: Existing activity data preserved
- **WHEN** the layout change is applied
- **THEN** all activity event data SHALL remain accessible via the status bar drawer and DAG tab
