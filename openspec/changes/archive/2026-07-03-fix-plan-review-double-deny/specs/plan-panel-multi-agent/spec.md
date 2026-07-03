# plan-panel-multi-agent

## ADDED Requirements

### Requirement: Single decision emission on every plan-review path

The plan-review hook SHALL emit exactly one `hookSpecificOutput` decision to stdout for every resolution path — a real user decision, the wall-clock backstop, or the dead-GUI fallback. No path SHALL write two concatenated decision objects.

#### Scenario: wall-clock backstop emits one deny

- **GIVEN** a plan review where the user never responds
- **WHEN** the wall-clock backstop fires
- **THEN** the hook writes a single well-formed deny decision (with the timeout message) to stdout and the agent does not hang

#### Scenario: dead-GUI fallback emits one deny

- **GIVEN** a plan review in progress
- **WHEN** the Nergal GUI process dies before the user responds
- **THEN** the hook writes a single well-formed deny decision (with the GUI-gone message) to stdout

#### Scenario: normal decision emits one output

- **WHEN** the user approves or rejects the plan in the GUI
- **THEN** the hook writes exactly one corresponding allow/deny decision to stdout
