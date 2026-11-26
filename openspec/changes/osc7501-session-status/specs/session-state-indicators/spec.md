## MODIFIED Requirements

### Requirement: Polymorphic indicator resolves visual state from session data
The system SHALL render a polymorphic indicator for each session that derives its visual state from the session's reported root status (OSC 7501, see `program-status-protocol`), its `modeMap` value, `planReviewStatus`, `askUser` state, and pending `Notification` attention. Attention SHALL take priority over every other state; attention is pending when a plan review is `pending_review`, an ask-user question is pending, notification attention is pending, or the root status is `blocked`. Completed SHALL come next. When the session has a root status, the remaining state SHALL come from it: `error` → error, `done` → done, `idle` → idle, and `working` → working when the mode is a tool name, thinking when the mode is `"active"`, and working otherwise (for example after `Stop` while background work runs). When the session has no root status, the remaining resolution SHALL be: idle (mode `"idle"`) > thinking (mode `"active"`) > working (mode is a tool name). Notification attention SHALL be set by the `permission_prompt` and `agent_needs_input` notification types. The `agent_completed` notification type SHALL NOT set attention; it SHALL clear pending notification attention for its session only when that attention came from `agent_needs_input`, because a background agent is answered in agent view and fires no other clearing event. Attention from `agent_needs_input` SHALL survive PostToolUse and Stop of the foreground session, and SHALL clear on `agent_completed`, UserPromptSubmit, and SessionEnd. Other notification attention SHALL clear on PostToolUse, Stop, UserPromptSubmit, and SessionEnd.

#### Scenario: Idle session shows static circle
- **WHEN** a session's mode is `"idle"`, it has no root status, and it has no pending plan review, ask-user, or notification attention
- **THEN** the indicator renders as a static circle with muted gray color and no animation

#### Scenario: Thinking session shows blue circle
- **WHEN** a session's mode is `"active"` (between tool calls) and it has no root status
- **THEN** the indicator renders as a static circle with sky-blue color and no animation

#### Scenario: Working session shows circle with orbital ring
- **WHEN** a session's mode is a tool name (e.g., `"Read"`, `"Edit"`, `"Bash"`) and it has no root status
- **THEN** the indicator renders as a green circle with a rotating orbital ring (0.8s cycle)

#### Scenario: Attention session shows circle with ripple ping
- **WHEN** a session has `planReviewStatus === "pending_review"` OR has a pending `askUser` question OR has pending notification attention
- **THEN** the indicator renders as an orange circle with expanding ripple/ping animation (1.2s cycle)
- **AND** the attention state takes priority over any mode-derived or status-derived state

#### Scenario: Blocked root status shows attention
- **WHEN** session 4 has root status `blocked` with no `kind` (for example the folder-trust prompt) and no hook attention
- **THEN** the indicator renders the attention state

#### Scenario: Working root status after Stop shows working
- **WHEN** session 4's mode is `"idle"` after a `Stop` hook and its root status is `working`
- **THEN** the indicator renders the working state

#### Scenario: Working root status between tools shows thinking
- **WHEN** session 4's mode is `"active"` and its root status is `working`
- **THEN** the indicator renders the thinking state

#### Scenario: Done root status shows done
- **WHEN** session 4's root status is `done`, its mode is `"Read"` from a late PostToolUse, and it has no attention
- **THEN** the indicator renders the done state: a static green ring with a transparent center and no animation

#### Scenario: Error root status shows error
- **WHEN** session 4's root status is `error` and it has no attention
- **THEN** the indicator renders the error state: a static red circle with no animation

#### Scenario: Idle root status shows idle
- **WHEN** session 4's root status is `idle` and its mode is `"active"`
- **THEN** the indicator renders the idle state

#### Scenario: Removed root status falls back to hooks
- **WHEN** session 4's root status is removed and its mode is `"Bash"`
- **THEN** the indicator renders the working state

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

#### Scenario: Foreground activity keeps a background agent wait
- **WHEN** `agent_needs_input` attention is pending for session 4, then PreToolUse, PostToolUse and Stop arrive for session 4
- **THEN** session 4 SHALL keep its pending notification attention
- **AND** the MCP descriptor SHALL still report session 4 as `needs_attention` with a `waiting_for` that starts with `background agent`

#### Scenario: Prompt submit clears a background agent wait
- **WHEN** `agent_needs_input` attention is pending for session 4 and the user submits a prompt in session 4
- **THEN** session 4 SHALL have no pending notification attention

#### Scenario: Background agent completion keeps a main-session permission wait
- **WHEN** `permission_prompt` attention is pending for session 4, then `agent_completed` arrives for session 4
- **THEN** session 4 SHALL keep its pending notification attention
- **AND** the MCP descriptor SHALL still report session 4 as `needs_attention` with the permission `waiting_for`

#### Scenario: Completed session shows check icon
- **WHEN** a session's status is `"completed"`
- **THEN** the indicator renders as a static check icon with dim color and no animation
