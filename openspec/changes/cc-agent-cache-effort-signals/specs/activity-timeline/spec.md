## ADDED Requirements

### Requirement: Subagent lifecycle entries

The system SHALL record a `subagent` activity entry when a `SubagentStart` hook arrives with a non-empty `agent_type`. The entry SHALL read `Subagent: <agent_type>`, carry the `agent_id`, and start in `running` status. A `SubagentStop` with the same `agent_id` SHALL mark that entry `done`, stamp its duration, and set its detail to the subagent's `last_assistant_message`. When the `SubagentStop` payload carries `effort.level`, the entry message SHALL append `· <level>`. Events with an empty or missing `agent_type` (CC internal agents such as prompt suggestions) SHALL be dropped. Entries SHALL be flat: hook payloads carry no parent agent id, so nesting is not rendered. On `SubagentStart`, the newest running `tool_use` entry for the `Agent` or `Task` tool that has no agent id SHALL become the subagent entry (type, label, agent id) instead of a second row. A PostToolUse of a different tool SHALL never close a running `Agent`/`Task` row through the any-name fallback.

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
