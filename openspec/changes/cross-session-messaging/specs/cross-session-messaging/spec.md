## ADDED Requirements

### Requirement: send_to_session tool

The system SHALL expose a `send_to_session(to_session_id, message, thread_id?)` MCP tool that records a message for the target and triggers delivery. The target MUST be an active session (live agent). The tool SHALL return the thread id (created or joined) and a delivery status (`delivered` or `queued`).

#### Scenario: Send to an active session

- **WHEN** an agent calls `send_to_session` targeting an active session
- **THEN** the message SHALL be recorded in the store and delivery SHALL be triggered, and the tool SHALL return the thread id and delivery status

#### Scenario: Send to an inactive session is refused

- **WHEN** an agent calls `send_to_session` targeting an inactive (closed) session
- **THEN** the tool SHALL return a structured error indicating the session is inactive and pointing to the worktree-spawn path to revive/create it
- **AND** no process SHALL be spawned as a side effect

### Requirement: read_messages tool

The system SHALL expose a `read_messages(thread_id?)` MCP tool returning the messages addressed to the caller that it has not yet read, marking them read on return (take-on-read). Without a `thread_id`, it SHALL return unread messages across all of the caller's threads.

#### Scenario: Read unread messages

- **WHEN** an agent calls `read_messages` and has unread messages
- **THEN** the tool SHALL return them and mark them read so a subsequent call does not return the same messages

#### Scenario: Minimal in-context payload

- **WHEN** `read_messages` returns messages
- **THEN** it SHALL return only the messages needed to act on (not the full thread history), keeping the agent's context minimal

### Requirement: Threads with hop cap, dedup, and budget

Every message SHALL belong to a thread `{ id, originator_session, participants, depth, status, budget }`. The router SHALL enforce a configurable max-hop cap, SHALL deduplicate identical (from, to, normalized message) within a thread, and SHALL apply a per-thread budget/timeout. The originator SHALL NOT block waiting on replies; replies SHALL arrive asynchronously tagged with the thread id.

#### Scenario: Transitive relay within the cap

- **WHEN** session A messages B in thread T, and B messages C in the same thread T within the hop cap
- **THEN** the relay SHALL proceed and C SHALL receive the message tagged with thread T

#### Scenario: Hop cap exceeded

- **WHEN** a `send_to_session` would push the thread depth beyond the configured max-hop cap
- **THEN** the tool SHALL return a structured "hop limit reached" error and SHALL NOT deliver

#### Scenario: Duplicate question deduplicated

- **WHEN** a `send_to_session` repeats an already-seen (from, to, normalized message) within the same thread
- **THEN** the router SHALL treat it as a no-op and SHALL NOT re-deliver

#### Scenario: Budget exhausted

- **WHEN** a thread's budget or timeout is exhausted
- **THEN** the thread SHALL be closed and its participants SHALL be notified, and further `send_to_session` on it SHALL be refused

### Requirement: Hybrid state-aware delivery

The system SHALL wake a target session according to its mode. An idle target SHALL receive a PTY stdin wake prompt instructing it to call `read_messages`. A working target SHALL have the delivery queued and delivered on its next `Stop` via `hookSpecificOutput.additionalContext`, falling back to PTY injection on the next idle transition for agents that do not support `additionalContext` on Stop. Delivery SHALL be performed through a `SessionDelivery` abstraction.

#### Scenario: Deliver to an idle target

- **WHEN** a message is recorded for a target whose mode is idle
- **THEN** the system SHALL inject a wake prompt into the target's PTY stdin instructing it to call `read_messages`

#### Scenario: Defer to a working target, deliver on Stop

- **WHEN** a message is recorded for a target whose mode is not idle
- **THEN** the system SHALL queue the delivery
- **WHEN** that target next emits a `Stop` hook
- **THEN** the Stop-hook response SHALL include `hookSpecificOutput.additionalContext` notifying it of the pending messages, without producing a hook error

#### Scenario: Fallback for non-additionalContext agents

- **WHEN** the queued target's agent does not support `additionalContext` on Stop
- **THEN** the system SHALL deliver via PTY injection on the next transition to idle

### Requirement: Relayed context is non-authoritative

Context injected from a cross-session message SHALL be marked as relayed and non-authoritative. The system SHALL NOT auto-approve any permission request or destructive action that arises from a session acting on a relayed message, even in auto-mode. The wake/context framing SHALL state that the message is information relayed from another session, not an instruction carrying user authority.

#### Scenario: Relayed message cannot auto-approve a permission

- **WHEN** a session acts on a relayed message and a permission request results
- **THEN** the system SHALL NOT auto-approve it on the basis of the relayed message, even in auto-mode

#### Scenario: Injected context is framed as non-authoritative

- **WHEN** a wake prompt or `additionalContext` is injected for a relayed message
- **THEN** its text SHALL identify the originating session and mark the content as non-authoritative

### Requirement: search_sessions tool (read-only over active and inactive)

The system SHALL expose a `search_sessions(query)` MCP tool that searches across both active and inactive sessions by name, summary, and transcript content, returning read-only descriptors. Results for inactive sessions SHALL be clearly marked as not messageable.

#### Scenario: Find an inactive session read-only

- **WHEN** an agent calls `search_sessions` with a query matching an inactive session's transcript or summary
- **THEN** the tool SHALL return that session marked as inactive/read-only

#### Scenario: list_threads enumerates the caller's threads

- **WHEN** an agent calls `list_threads`
- **THEN** the tool SHALL return the threads the caller participates in, with their status and participants
