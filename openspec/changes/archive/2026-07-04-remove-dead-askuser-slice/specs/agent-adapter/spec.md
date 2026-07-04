# agent-adapter

Ask-user is a non-blocking notifier; the adapter interface carries no blocking answer round-trip.

## ADDED Requirements

### Requirement: Ask-user is notifier-only at the adapter boundary

The `AgentAdapter` trait SHALL NOT expose a blocking ask-user answer method; ask-user hook events SHALL only drive non-blocking attention signaling (pending-ask state, tab blink/tint), with the agent's own TUI owning the prompt. The `ASK_USER_BLOCKING` capability flag SHALL remain as a declaration gating attention UX, independent of any answer round-trip.

#### Scenario: ask-user event flows notify-only

- **WHEN** a `PreToolUse[AskUserQuestion]` hook event arrives for a session
- **THEN** the backend emits the pending-ask attention event and does not create or await any FIFO/answer channel

#### Scenario: no answer entry points remain

- **WHEN** searching the codebase after this change
- **THEN** there is no `submit_ask_answer` Tauri command, no `AgentAdapter::submit_ask_answer` method, and no ask-FIFO registration path

#### Scenario: attention UX unaffected

- **GIVEN** a session whose adapter declares `ASK_USER_BLOCKING`
- **WHEN** ask-user fires and later resolves (`PostToolUse`)
- **THEN** the tab blink/tint behavior driven by `pendingAsksAtom`/`pendingAttentionAtom` works exactly as before
