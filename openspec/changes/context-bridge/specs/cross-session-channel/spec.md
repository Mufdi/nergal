## ADDED Requirements

### Requirement: Channel creation between two sessions
The system SHALL allow creating a bidirectional communication channel between exactly two active Claude Code sessions. A channel MUST have a topic, two participant session IDs, and a unique channel ID. The system SHALL create a markdown file at `.claude/crossmsg-{channel_id}.md` in the active workspace's project root with a header containing the topic, participant names, and creation timestamp.

#### Scenario: Create channel from command palette
- **WHEN** user opens command palette and selects "New Channel"
- **THEN** system shows a picker listing all active (non-completed) sessions excluding the current one
- **WHEN** user selects a target session and enters a topic
- **THEN** system creates the channel file, registers the channel in state, and injects the initial prompt into the first session's PTY stdin

#### Scenario: Create channel from sidebar context menu
- **WHEN** user right-clicks a session in the sidebar and selects "Start Channel"
- **THEN** system creates a channel between the current active session and the right-clicked session, prompting for a topic

#### Scenario: Reject channel with invalid participants
- **WHEN** user attempts to create a channel with a session that is completed or does not exist
- **THEN** system shows an error toast and does not create the channel

### Requirement: Autonomous message routing
The system SHALL automatically route messages between channel participants without user intervention. When a participant writes a message ending with `**CAMBIO**` to the channel file, the system SHALL detect the new content and inject a read-and-respond prompt into the other participant's PTY stdin.

#### Scenario: Route message from session A to idle session B
- **WHEN** session A writes a message ending with `**CAMBIO**` to the channel file
- **AND** session B's mode is `"idle"`
- **THEN** system injects a prompt into session B's PTY stdin instructing it to read the channel file and respond

#### Scenario: Queue message when target session is working
- **WHEN** session A writes a message ending with `**CAMBIO**` to the channel file
- **AND** session B's mode is not `"idle"` (e.g., a tool name or `"active"`)
- **THEN** system queues the injection
- **WHEN** session B's mode transitions to `"idle"` (via `Stop` event)
- **THEN** system dequeues and injects the prompt into session B's PTY stdin

#### Scenario: Replace queued message with newer one
- **WHEN** a message is already queued for session B
- **AND** session A writes another `**CAMBIO**` message before session B becomes idle
- **THEN** system replaces the queued injection (Claude will read the full file regardless)

### Requirement: Channel file format
The channel file SHALL use a structured markdown format where each message is a level-2 heading with the session name and ISO timestamp, followed by the message content, and terminated with either `**CAMBIO**` (turn passing) or `**CONSENSO**` (channel completion).

#### Scenario: Append-only message history
- **WHEN** a session writes to the channel file
- **THEN** the new message MUST be appended after the last `---` separator
- **AND** previous messages MUST NOT be modified or deleted

#### Scenario: Parse sender from message header
- **WHEN** the file watcher detects a change to a channel file
- **THEN** the system parses the last `## {name} — {timestamp}` header to identify the sender session

### Requirement: Channel completion via CONSENSO
The system SHALL detect when a channel message ends with `**CONSENSO**` instead of `**CAMBIO**` and mark the channel as closed. No further routing SHALL occur on a closed channel.

#### Scenario: Session signals consensus
- **WHEN** a session writes a message ending with `**CONSENSO**`
- **THEN** system marks the channel as `closed` in state
- **AND** system shows a toast: "Channel '{topic}' reached consensus"
- **AND** no further injections are made to either participant

#### Scenario: User manually closes channel
- **WHEN** user closes a channel from the UI before CONSENSO
- **THEN** system marks the channel as `closed`
- **AND** no further injections are made
- **AND** the channel file is preserved (not deleted)

### Requirement: Channel timeout
The system SHALL notify the user if a channel has no new messages for a configurable timeout period (default: 5 minutes). This prevents stale channels from being forgotten.

#### Scenario: No response within timeout
- **WHEN** a `**CAMBIO**` message is routed to a session
- **AND** no new message appears in the channel file within the timeout period
- **THEN** system shows a toast: "Channel '{topic}' — no response from {session_name} in 5 minutes"

### Requirement: Channel viewer
The system SHALL provide a read-only view of the channel file content, accessible from the UI. The viewer MUST render the markdown with session name attribution and timestamps.

#### Scenario: Open channel viewer
- **WHEN** user clicks on an active channel indicator in the sidebar
- **THEN** system opens a tab in the right panel displaying the channel file content
- **AND** the viewer auto-refreshes when new messages are detected

### Requirement: Channel indicators in sidebar
The system SHALL display visual indicators in the sidebar for sessions that are participants in an active channel.

#### Scenario: Active channel badge
- **WHEN** a session is participating in an active channel
- **THEN** the SessionRow displays a channel icon or badge
- **AND** the badge shows the other participant's session name on hover

#### Scenario: Pending injection indicator
- **WHEN** a message is queued for injection into a session (target is working)
- **THEN** the SessionRow displays a pending-context badge with count
- **WHEN** the injection is delivered
- **THEN** the badge disappears
