## MODIFIED Requirements

### Requirement: Marker file lifecycle
The system SHALL drop a marker file at `~/.config/nergal/pending-mocs/<session_id>.json` on every event that should produce a session MOC. Triggers MUST be:

- `HookEvent::SessionEnd` arriving on the hook socket.
- Explicit user-driven session deletion via `delete_session`.
- Workspace deletion via `delete_workspace` (one marker per session in the deleted workspace).
- `WindowEvent::CloseRequested` (one marker per active session).
- PTY-side session termination (worker thread observes EOF on the PTY), only for a PTY whose launch command was written to it. A PTY whose launch was rolled back, or that was closed before its launch command was written, SHALL NOT drop a marker.

Each marker MUST contain JSON with at minimum: `session_id`, `workspace_id`, `agent_id`, `trigger` (one of the above), `created_at` (unix ms).

The system SHALL write markers atomically (tmp file + rename). The system SHALL delete a marker only after its MOC + backlink propagation completes successfully.

#### Scenario: SessionEnd hook drops a marker

- **WHEN** the hook server receives a `SessionEnd` event for session S
- **AND** the workspace owning S has a non-null `vault_root`
- **THEN** the dispatcher SHALL write `~/.config/nergal/pending-mocs/S.json`
- **AND** spawn `nergal post-session` detached

#### Scenario: App close drops markers for all active sessions

- **WHEN** the user closes Nergal
- **AND** sessions S1 and S2 are active in workspaces with configured vaults
- **THEN** the close handler SHALL write `S1.json` and `S2.json` markers
- **AND** spawn the runner detached
- **AND** `app.exit()` SHALL be called immediately afterwards

#### Scenario: Workspace without vault skips marker

- **WHEN** SessionEnd fires for a session whose workspace has `vault_root = null`
- **THEN** no marker SHALL be written (no work to do)

#### Scenario: EOF of a launched PTY drops a marker
- **GIVEN** a session S whose launch command was written to its PTY P, in a workspace with a non-null `vault_root`
- **WHEN** the reader thread of P observes EOF
- **THEN** the system SHALL write `~/.config/nergal/pending-mocs/S.json` unless S was already finalized in this process

#### Scenario: EOF of a PTY that never received its launch command drops nothing
- **GIVEN** a PTY P of session S whose launch was rolled back, or that was closed during startup
- **WHEN** the reader thread of P observes EOF
- **THEN** no marker SHALL be written for S, and a later launched PTY of S SHALL still drop one at its EOF
