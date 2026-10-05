## MODIFIED Requirements

### Requirement: Polymorphic indicator resolves visual state from session data
The system SHALL render a polymorphic indicator for each session that derives its visual state from the session's `modeMap` value, `planReviewStatus`, `askUser` state, and pending `Notification` attention. The resolution priority SHALL be: attention (plan review, ask user, or notification attention pending) > completed > idle > thinking (active between tools) > working (tool executing). Notification attention SHALL be set by the `permission_prompt` and `agent_needs_input` notification types. The `agent_completed` notification type SHALL NOT set attention; it SHALL clear pending notification attention for its session only when that attention came from `agent_needs_input`, because a background agent is answered in agent view and fires no other clearing event. Pending notification attention SHALL also clear on PostToolUse, Stop, UserPromptSubmit, and SessionEnd.

#### Scenario: Idle session shows static circle
- **WHEN** a session's mode is `"idle"` and it has no pending plan review, ask-user, or notification attention
- **THEN** the indicator renders as a static circle with muted gray color and no animation

#### Scenario: Thinking session shows blue circle
- **WHEN** a session's mode is `"active"` (between tool calls)
- **THEN** the indicator renders as a static circle with sky-blue color and no animation

#### Scenario: Working session shows circle with orbital ring
- **WHEN** a session's mode is a tool name (e.g., `"Read"`, `"Edit"`, `"Bash"`)
- **THEN** the indicator renders as a green circle with a rotating orbital ring (0.8s cycle)

#### Scenario: Attention session shows circle with ripple ping
- **WHEN** a session has `planReviewStatus === "pending_review"` OR has a pending `askUser` question OR has pending notification attention
- **THEN** the indicator renders as an orange circle with expanding ripple/ping animation (1.2s cycle)
- **AND** the attention state takes priority over any mode-derived state

#### Scenario: Background agent needing input sets attention
- **WHEN** a `Notification` hook with `notification_type: "agent_needs_input"` arrives for session 4
- **THEN** session 4 gets pending notification attention
- **AND** its session tab and sidebar row carry the same pending tint as an AskUserQuestion
- **AND** the MCP descriptor reports session 4 as `needs_attention`

#### Scenario: Background agent completion does not set attention
- **WHEN** a `Notification` hook with `notification_type: "agent_completed"` arrives for session 4, which has no pending attention and MCP mode `running`
- **THEN** session 4 SHALL NOT get pending notification attention
- **AND** the MCP descriptor mode of session 4 SHALL stay `running`

#### Scenario: Background agent completion clears a pending input wait
- **WHEN** `agent_needs_input` arrives for session 4, then `agent_completed` arrives for session 4
- **THEN** session 4 SHALL have no pending notification attention
- **AND** the MCP descriptor SHALL report session 4 as `idle` with no `waiting_for`

#### Scenario: Background agent completion keeps a main-session permission wait
- **WHEN** `permission_prompt` attention is pending for session 4, then `agent_completed` arrives for session 4
- **THEN** session 4 SHALL keep its pending notification attention
- **AND** the MCP descriptor SHALL still report session 4 as `needs_attention` with the permission `waiting_for`

#### Scenario: Completed session shows check icon
- **WHEN** a session's status is `"completed"`
- **THEN** the indicator renders as a static check icon with dim color and no animation
