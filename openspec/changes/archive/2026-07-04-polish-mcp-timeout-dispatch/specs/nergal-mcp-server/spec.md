## ADDED Requirements

### Requirement: Subprocess-backed tracker tools are bounded

The subprocess-backed MCP tools (`get_pr_status`/`get_git_status`, which shell out to `gh`/`git`) SHALL execute off the async accept loop and SHALL be bounded by a timeout, so a hung subprocess (e.g. a network-stalled `gh`) never blocks the daemon's request-handling worker indefinitely. On timeout the tool SHALL return a JSON-RPC error to the caller rather than hang.

#### Scenario: hung gh does not block the daemon

- **GIVEN** `get_pr_status` invokes a `gh` call that stalls on the network
- **WHEN** the tool's timeout elapses
- **THEN** the caller receives a tool error and the daemon's worker is free to serve other connections

#### Scenario: fast path is unaffected

- **GIVEN** a `gh`/`git` call that completes normally within the timeout
- **WHEN** the tool runs
- **THEN** it returns the same rollup it does today, with no observable change
