# session-directory

## ADDED Requirements

### Requirement: PTY child processes are reaped on session teardown

A PTY-backed session (agent session, aux shell, or quake shell) SHALL reap its child process on teardown so no defunct (zombie) process remains after the session exits or is killed. Reaping SHALL occur after the kill signal is delivered, and SHALL be bounded so a hung child cannot deadlock teardown.

#### Scenario: normal exit is reaped

- **GIVEN** a PTY session whose child exits on its own
- **WHEN** the session is torn down
- **THEN** the child is `wait()`ed and leaves no defunct process

#### Scenario: killed child is reaped

- **GIVEN** a PTY session with a still-running child
- **WHEN** the session is torn down (kill_tree sends SIGTERM)
- **THEN** the child is reaped after the signal and leaves no defunct process

#### Scenario: many sessions leave no zombie accumulation

- **WHEN** many sessions are opened and closed over the app's lifetime
- **THEN** the count of defunct child processes attributable to Nergal stays at zero
