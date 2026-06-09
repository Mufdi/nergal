## ADDED Requirements

### Requirement: MCP daemon owned by the cluihud process

The system SHALL run a single MCP server as part of the cluihud application process, holding the global view of all live sessions across every open workspace. The daemon SHALL be the sole authority for the session directory and SHALL be reachable by session-local MCP shims over the existing Unix socket.

#### Scenario: Daemon answers from global state

- **WHEN** an MCP shim forwards a tool call to the daemon
- **THEN** the daemon SHALL answer using its in-process global session state, including sessions in workspaces other than the caller's

#### Scenario: Daemon disabled by setting

- **WHEN** the MCP server is disabled in settings
- **THEN** the daemon SHALL not accept MCP shim connections, and a shim that starts SHALL return a structured "MCP disabled" error on tool calls without crashing

### Requirement: stdio MCP shim

The system SHALL provide a `cluihud mcp` CLI subcommand that acts as a stdio MCP server: it speaks MCP JSON-RPC on its stdin/stdout to the spawning agent and forwards requests to the daemon over the Unix socket. The shim SHALL be agent-agnostic (usable by any agent that supports stdio MCP servers).

#### Scenario: Shim bridges a tool call

- **WHEN** an agent invokes a cluihud MCP tool
- **THEN** the shim SHALL forward the request to the daemon, await the response, and return it to the agent over stdio

#### Scenario: Daemon unreachable

- **WHEN** the shim cannot reach the daemon (app not running or socket missing)
- **THEN** the shim SHALL still complete the MCP handshake and SHALL return a structured error ("cluihud daemon not reachable") on each tool call instead of hanging

### Requirement: Zero-config identity correlation

On connect, the shim SHALL report `CLUIHUD_SESSION_ID` (always injected by cluihud adapters) and `CLAUDE_CODE_SESSION_ID` (when present, for Claude Code) to the daemon. The daemon SHALL map the MCP connection to the cluihud session using `CLUIHUD_SESSION_ID` as authoritative, with `CLAUDE_CODE_SESSION_ID` as a confirming cross-check for CC sessions. No user configuration SHALL be required.

#### Scenario: CC session correlated

- **WHEN** a CC session's shim connects with both env vars present
- **THEN** the daemon SHALL resolve the caller to the cluihud session whose routing map matches `CLUIHUD_SESSION_ID`, and SHALL record `CLAUDE_CODE_SESSION_ID` as the confirmed CC identity

#### Scenario: Non-CC session correlated

- **WHEN** a Codex/Pi/OpenCode session's shim connects with only `CLUIHUD_SESSION_ID` present
- **THEN** the daemon SHALL resolve the caller from `CLUIHUD_SESSION_ID` alone

#### Scenario: Unidentified caller

- **WHEN** a shim connects with neither env var resolvable to a known session
- **THEN** the daemon SHALL mark the caller as unidentified, SHALL NOT add it to the directory, and SHALL return null from `whoami`

### Requirement: whoami self-identification tool

The system SHALL expose a `whoami` MCP tool that returns the caller's own resolved cluihud session descriptor (or null when unidentified), so an agent can locate itself within the directory.

#### Scenario: Agent identifies itself

- **WHEN** an identified agent calls `whoami`
- **THEN** the tool SHALL return that session's id, name, workspace, and agent type

#### Scenario: Unidentified agent calls whoami

- **WHEN** an unidentified caller calls `whoami`
- **THEN** the tool SHALL return null (not an error and not a guessed identity)

### Requirement: Agent MCP-config registration

The system SHALL register the `cluihud mcp` shim in the spawned agent's MCP configuration so the tools are available without manual user setup, for every agent that supports stdio MCP servers.

#### Scenario: CC session gets the tools

- **WHEN** cluihud spawns a Claude Code session with the MCP server enabled
- **THEN** the `cluihud mcp` server SHALL be present in that session's MCP configuration and its tools SHALL be callable
