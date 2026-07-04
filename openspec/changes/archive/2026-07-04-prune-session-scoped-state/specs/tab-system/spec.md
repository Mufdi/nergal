# tab-system

Reclaims session-scoped in-memory state when a session is permanently closed.

## ADDED Requirements

### Requirement: Session-scoped state is reclaimed on permanent close

When a session is permanently destroyed (soft-close finalize, or grace-window delete), the system SHALL remove that session's entries from every registered session-keyed store map. Session-keyed map atoms SHALL be created through the registering factory so new ones participate automatically. State MUST survive the soft-close/grace window itself (restore keeps full panel state).

#### Scenario: finalize prunes the maps

- **GIVEN** a session with populated activity, git, plan, and panel state
- **WHEN** its soft-close window elapses and finalize runs
- **THEN** no registered store map retains an entry under that session id

#### Scenario: restore within the window keeps state

- **WHEN** the user undoes a session close within the soft-close window
- **THEN** the session's panel state is intact (prune did not run)

#### Scenario: grace-window delete prunes too

- **WHEN** a session (or its workspace) is deleted via the grace-delete path and the grace timer fires
- **THEN** the session's entries are removed from all registered maps
