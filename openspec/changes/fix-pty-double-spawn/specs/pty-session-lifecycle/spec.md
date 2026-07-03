# pty-session-lifecycle

At most one PTY per session id, guaranteed under concurrent creation.

## ADDED Requirements

### Requirement: Session PTY creation is idempotent under concurrency

`start_claude_session` SHALL check-and-reserve the session's `session_ptys` slot under a single lock hold before spawning, SHALL return the existing PTY id when the slot is taken, and SHALL remove the reservation if the spawn fails — so no interleaving of concurrent calls can produce a second PTY for the same session or an entry-less (unkillable) PTY.

#### Scenario: concurrent double start yields one PTY

- **GIVEN** two concurrent `start_claude_session` calls for the same session id
- **WHEN** both execute
- **THEN** exactly one PTY process exists, both callers receive the same `pty_id`, and `kill_session_pty` can terminate it

#### Scenario: spawn failure releases the slot

- **GIVEN** a reservation was inserted and `spawn_pty` returns an error
- **WHEN** the call unwinds
- **THEN** the `session_ptys` entry is removed and a subsequent `start_claude_session` for that session succeeds cleanly

#### Scenario: established-session early return unchanged

- **WHEN** `start_claude_session` runs for a session that already has a live PTY
- **THEN** it returns the existing `pty_id` without spawning (today's behavior)
