---
status: archived
implemented: 2026-03-25
archived: 2026-04-04
files:
  - src/components/activity/ActivityLog.tsx
  - src/components/activity/ActivityDrawer.tsx
  - src/components/activity/DagGraph.tsx
  - src/stores/activity.ts
  - src/components/layout/StatusBar.tsx
---

## Purpose

Provide a status bar activity summary, expandable timeline drawer, and DAG graph visualization for agent tool-call events.

## Implementation Notes

All requirements implemented. DAG graph uses React Flow with interactive expandable nodes. Terminal occupies full center column height (no activity log split). ActivityLog.tsx is a simpler non-interactive fallback view distinct from the drawer.
## Requirements
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

### Requirement: Subagent lifecycle entries

The system SHALL record a `subagent` activity entry when a `SubagentStart` hook arrives with a non-empty `agent_type`. The entry SHALL read `Subagent: <agent_type>`, carry the `agent_id`, and start in `running` status. A `SubagentStop` with the same `agent_id` SHALL mark that entry `done`, stamp its duration, and set its detail to the subagent's `last_assistant_message`. When the `SubagentStop` payload carries `effort.level`, the entry message SHALL append `· <level>`. Events with an empty or missing `agent_type` (CC internal agents such as prompt suggestions) SHALL be dropped. Entries SHALL be flat: hook payloads carry no parent agent id, so nesting is not rendered. On `SubagentStart`, the newest running `tool_use` entry for the `Agent` or `Task` tool that has no agent id SHALL become the subagent entry (type, label, agent id) instead of a second row. A PostToolUse of a different tool SHALL never close a running `Agent`/`Task` row through the any-name fallback. A `Stop` SHALL mark every running `Agent`/`Task` tool row without an agent id as `done`. `PreToolUse` rows created from a payload carrying `agent_id` SHALL be tagged with that id. A `SubagentStop` SHALL close every `running` tool row tagged with its `agent_id` (and no other agent's). The stopped `subagent` entry's detail SHALL be the `tool_input.message` of that agent's `SubagentHandback` call when present, else the non-empty message forwarded on the stop event (the payload's `last_assistant_message`, or the last assistant text of the subagent transcript when the payload message is empty), else `Stopped before reporting`. A `subagent` entry with a detail SHALL be expandable regardless of the detail length. While a subagent entry is `running` it SHALL stay at the head of the timeline, above newer rows, until it stops. The stopped entry SHALL settle at the head stamped with the stop time (below any subagent still running) while its duration keeps measuring from the start time. A `tool_use` row created from a payload carrying `agent_id` and a non-empty `agent_type` SHALL show that type beside its message in secondary text. The settled entry SHALL show the token count forwarded on the stop event beside its duration, formatted like other token counts (`48.5K tokens`), and omit it when none was forwarded. A `Stop` SHALL also mark every `running` tool row without an `ownerAgentId` as `done` without setting a duration.

#### Scenario: Start and stop pair by agent id
- **WHEN** `SubagentStart {agent_id: "a1", agent_type: "Explore"}` arrives, then `SubagentStop {agent_id: "a1", agent_type: "Explore", last_assistant_message: "Found 3 files", effort: {level: "medium"}}`
- **THEN** Activities SHALL hold one `subagent` entry `Subagent: Explore · medium` with status `done`, a duration, and detail `Found 3 files`

#### Scenario: Agent tool row becomes the subagent row
- **WHEN** a `PreToolUse {tool_name: "Agent"}` adds a running `Agent` tool row, then `SubagentStart {agent_id: "a1", agent_type: "Explore"}` arrives
- **THEN** Activities SHALL hold exactly one row for that call: a running `subagent` entry `Subagent: Explore` with agent id `a1`
- **AND** a later PostToolUse of another tool SHALL NOT mark that entry done

#### Scenario: Parallel tool completion does not close the Agent row
- **WHEN** a running `Agent` tool row exists and a `PostToolUse {tool_name: "Bash"}` arrives with no running `Bash` row
- **THEN** the `Agent` row SHALL stay `running`

#### Scenario: Stop closes an Agent row no subagent claimed
- **WHEN** a running `Agent` tool row has no agent id and a `Stop` arrives for the session
- **THEN** the row SHALL be `done` with a duration
- **AND** a running `subagent` entry SHALL stay `running`

#### Scenario: Stop closes only that agent's tagged rows
- **WHEN** running `Bash` rows tagged `a1` and `a2` exist and `SubagentStop {agent_id: "a1"}` arrives
- **THEN** the `a1` row SHALL be `done` with a duration and the `a2` row SHALL stay `running`

#### Scenario: Handback report becomes the detail
- **WHEN** `PreToolUse {tool_name: "SubagentHandback", agent_id: "a1", tool_input: {message: "Report"}}` arrives, then `SubagentStop {agent_id: "a1", last_assistant_message: ""}`
- **THEN** the `a1` `subagent` entry SHALL have detail `Report` and the `SubagentHandback` row SHALL be `done`

#### Scenario: Stopped subagent surfaces at the head
- **WHEN** a `subagent` entry started at t=1000 is followed by newer rows and `SubagentStop` arrives at t=9000
- **THEN** the entry SHALL be the newest row, with timestamp 9000 and duration 8000 ms

#### Scenario: Running subagent stays pinned at the head
- **WHEN** a `subagent` entry is `running` and a `Read` tool row arrives afterwards
- **THEN** the `subagent` entry SHALL remain the newest row and the `Read` row SHALL sit directly below it

#### Scenario: Subagent tool rows carry the agent type
- **WHEN** `PreToolUse {tool_name: "Read", agent_id: "a1", agent_type: "reader-1"}` arrives
- **THEN** the `Read` row SHALL show `reader-1` beside its message, and a `Read` row without `agent_id` SHALL show no label

#### Scenario: Subagent stopped without reporting
- **WHEN** `SubagentStop {agent_id: "a1"}` arrives with an empty message and no `SubagentHandback` call
- **THEN** the entry detail SHALL be `Stopped before reporting` and the row SHALL be expandable

#### Scenario: Subagent tokens beside the duration
- **WHEN** `SubagentStop {agent_id: "a1"}` is forwarded with `agent_tokens: 48500`
- **THEN** the settled entry SHALL show `48.5K tokens` beside its duration

#### Scenario: Stop closes main-agent rows outside the matcher
- **WHEN** running `ToolSearch` and `TaskStop` rows without `ownerAgentId` exist and `Stop` arrives
- **THEN** both rows SHALL be `done` with no duration, and a running row tagged with an `ownerAgentId` SHALL stay `running`

#### Scenario: Stop without a matching start
- **WHEN** `SubagentStop {agent_id: "a9", agent_type: "Plan"}` arrives with no running entry for `a9`
- **THEN** Activities SHALL add a standalone `done` entry `Subagent: Plan`

#### Scenario: Internal agent is dropped
- **WHEN** `SubagentStop {agent_id: "x", agent_type: ""}` arrives
- **THEN** no activity entry SHALL be added

#### Scenario: Concurrent subagents pair independently
- **WHEN** starts arrive for `a1` (Explore) and `a2` (Explore), then a stop arrives for `a1`
- **THEN** only the `a1` entry SHALL be `done` and the `a2` entry SHALL stay `running`

### Requirement: Background session notification entries

The system SHALL add a `subagent` activity entry and a desktop notification for `Notification` hooks of type `agent_completed`, with message `Background agent finished` and the notification `message` as detail. For type `agent_needs_input`, the attention entry SHALL read `Background agent needs input`.

#### Scenario: Background session completes
- **WHEN** `Notification {notification_type: "agent_completed", message: "Refactor done"}` arrives for session 2
- **THEN** session 2 Activities SHALL gain the entry `Background agent finished` with detail `Refactor done`

#### Scenario: Background session waits for input
- **WHEN** `Notification {notification_type: "agent_needs_input", message: "Agent X is waiting"}` arrives for session 2
- **THEN** session 2 Activities SHALL gain the entry `Background agent needs input` with detail `Agent X is waiting`

### Requirement: Keyboard expansion of detail rows

In the Activities drawer, Enter and Space on the cursor row SHALL expand or collapse that row's detail (the subagent report or long thinking text) when the row has one, using the same expanded state as the mouse toggle. Precedence per row kind: a group header toggles its group on Enter and Space; Enter on a row with a file opens the file even when the row also has a detail; Space on such a row, and Enter or Space on a detail row without a file, toggles the detail; a plain row does nothing.

#### Scenario: Enter expands a finished subagent report
- **WHEN** the cursor is on a `Subagent: …` row with a report and the user presses Enter
- **THEN** the report SHALL expand, and pressing Enter again SHALL collapse it

#### Scenario: Enter on a file row still opens the file
- **WHEN** the cursor is on a row with a file and the user presses Enter
- **THEN** the file SHALL open in a tab

