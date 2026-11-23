## MODIFIED Requirements

### Requirement: Cooperative identity from the env hint

The system SHALL identify a caller by the `NERGAL_SESSION_ID` (and `CLAUDE_CODE_SESSION_ID`) it reports, validated against the daemon's live session registry (the live set of `session-directory`, not the adapter cache). Identity is cooperative (the env value is trusted within the uid boundary, not adversarially authenticated). An id that does not match a live session SHALL leave the caller unidentified. Identity SHALL be re-validated lazily on each tool call to survive a connect-before-register race, and the binding SHALL be torn down on disconnect.

#### Scenario: Valid env id resolves

- **WHEN** a shim reports a `NERGAL_SESSION_ID` that matches a live session in the registry
- **THEN** the daemon SHALL bind the connection to that session

#### Scenario: Unknown id is unidentified

- **WHEN** a shim reports an id that matches no live session
- **THEN** the caller SHALL be unidentified (null `whoami`)

#### Scenario: Connect-before-register resolves lazily

- **WHEN** a shim connects before its session is registered and is initially unidentified
- **THEN** a later tool call SHALL re-validate and bind the session once the registry knows the id

#### Scenario: Teardown on disconnect

- **WHEN** a shim disconnects (peer close)
- **THEN** the daemon SHALL drop the connection→session binding and any associated side-map entries

#### Scenario: A killed session's id is unidentified

- **GIVEN** session S had its agent PTY killed, but S is still in the adapter cache
- **WHEN** a shim reports `NERGAL_SESSION_ID=S`, or a `CLAUDE_CODE_SESSION_ID` that maps to S
- **THEN** the caller SHALL be unidentified (null `whoami`)
