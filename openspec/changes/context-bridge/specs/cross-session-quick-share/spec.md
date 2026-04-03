## ADDED Requirements

### Requirement: Quick share context to another session
The system SHALL allow the user to compose and send a one-way context message from one session to another. The context MUST be injected into the target session's next prompt via the `inject_edits()` pipeline.

#### Scenario: Send quick share via shortcut
- **WHEN** user presses Ctrl+Shift+S
- **THEN** system opens a mini-composer overlay with a text area and a session picker listing all active sessions (excluding the current one)
- **WHEN** user writes context text, selects a target session, and confirms
- **THEN** system writes the context to the target session's HookState as `pending_context`
- **AND** system shows a toast: "Context queued for {session_name}"

#### Scenario: Quick share delivered on next prompt
- **WHEN** the target session has pending quick-share context in its HookState
- **AND** a `UserPromptSubmit` event fires for that session
- **THEN** `inject_edits()` reads and clears the `pending_context` field
- **AND** appends the context to the prompt message in a structured format with source attribution

#### Scenario: Quick share with empty text
- **WHEN** user attempts to send a quick share with empty text
- **THEN** system does not write to HookState and shows a validation error in the composer

### Requirement: Session-scoped HookState
The system SHALL use per-session HookState files at `~/.claude/cluihud-state-{session_id}.json` instead of a single global file. Each session's `inject_edits()` invocation MUST read only its own state file.

#### Scenario: Concurrent sessions with independent state
- **WHEN** session A has pending plan edits and session B has pending quick-share context
- **THEN** `inject_edits()` for session A reads only `cluihud-state-{A}.json`
- **AND** `inject_edits()` for session B reads only `cluihud-state-{B}.json`
- **AND** neither session's injection interferes with the other

#### Scenario: Backwards compatibility during migration
- **WHEN** `inject_edits()` runs for a session and the session-scoped file does not exist
- **THEN** the system falls back to reading the global `cluihud-state.json`

### Requirement: Pending context indicator
The system SHALL display a visual indicator on the SessionRow when a session has pending quick-share context that has not yet been injected.

#### Scenario: Badge appears on share
- **WHEN** a quick share is sent to a target session
- **THEN** the target session's SessionRow displays a pending-context badge

#### Scenario: Badge disappears on injection
- **WHEN** the target session's `UserPromptSubmit` fires and `inject_edits()` consumes the pending context
- **THEN** the pending-context badge disappears from the SessionRow

### Requirement: Quick share injection format
The injected context MUST include source attribution (sender session name) and timestamp so Claude can understand where the context came from.

#### Scenario: Injection format
- **WHEN** `inject_edits()` processes pending quick-share context
- **THEN** the injected text follows this format:
  ```
  [Shared context from session "{sender_name}" ({relative_time} ago)]:
  {context_text}
  ```
- **AND** the text is appended after any pending plan edits or annotations

### Requirement: One pending quick share per session
The system SHALL support at most one pending quick-share context per target session. Sending a new quick share to a session that already has pending context SHALL replace the previous one.

#### Scenario: Replace pending context
- **WHEN** user sends a quick share to session B which already has pending context
- **THEN** the previous pending context is replaced with the new one
- **AND** system shows a toast: "Previous context replaced for {session_name}"
