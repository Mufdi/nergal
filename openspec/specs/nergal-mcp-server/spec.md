# nergal-mcp-server Specification

## Purpose
TBD - created by archiving change nergal-mcp-server. Update Purpose after archive.
## Requirements
### Requirement: MCP daemon owned by the nergal process

The system SHALL run a single MCP server as part of the nergal application process, holding the global view of all live sessions across every open workspace. The daemon SHALL be the sole authority for the session directory. A "live session" SHALL be defined as a session with an active PTY child tracked by the daemon.

#### Scenario: Daemon answers from global state

- **WHEN** an MCP shim forwards a tool call to the daemon
- **THEN** the daemon SHALL answer using its in-process global session state, including sessions in workspaces other than the caller's

#### Scenario: Daemon disabled by setting

- **WHEN** the MCP server is disabled in settings
- **THEN** the daemon SHALL not accept MCP connections, and a shim that starts SHALL return a structured "MCP disabled" error on tool calls without crashing

### Requirement: Dedicated bidirectional MCP transport

The system SHALL expose a dedicated Unix socket for MCP, separate from the fire-and-forget hook socket, speaking length-framed JSON-RPC with per-request response correlation. The socket SHALL be created with mode `0600` in a per-user directory. The existing hook socket SHALL NOT be modified to carry MCP traffic.

#### Scenario: Request/response correlation

- **WHEN** the shim sends an MCP request over the dedicated socket
- **THEN** the daemon SHALL return a framed response correlated to that request id

#### Scenario: Socket is access-restricted

- **WHEN** the MCP socket is created
- **THEN** it SHALL have mode `0600` so only the owning uid can connect

### Requirement: stdio MCP shim

The system SHALL provide a `nergal mcp` CLI subcommand that acts as a stdio MCP server: it speaks MCP JSON-RPC on its stdin/stdout to the spawning agent and relays requests to the daemon over the dedicated socket. The shim SHALL be agent-agnostic.

#### Scenario: Shim relays a tool call

- **WHEN** an agent invokes a nergal MCP tool
- **THEN** the shim SHALL relay the request to the daemon, await the framed response, and return it over stdio

#### Scenario: Daemon unreachable — degraded mode

- **WHEN** the shim cannot reach the daemon
- **THEN** the shim SHALL complete `initialize` locally, answer `tools/list` from a vendored static tool list (generated from the daemon registry at build time), and return a structured error for `tools/call`

### Requirement: uid-restricted socket is the enforced boundary

The MCP socket SHALL be the only access boundary, enforced by uid: mode `0600` in a per-user directory, plus a `peer_cred().uid()` check rejecting any connection whose uid differs from the app's. The system SHALL NOT rely on a `/proc` pid-walk for a security boundary (it is TOCTOU-unsound and provides no confidentiality against a same-uid process, which can read the agent's environment directly).

#### Scenario: Other uid rejected

- **WHEN** a process owned by a different uid attempts to connect
- **THEN** the daemon SHALL reject the connection (socket perms and/or the uid check)

### Requirement: Cooperative identity from the env hint

The system SHALL identify a caller by the `NERGAL_SESSION_ID` (and `CLAUDE_CODE_SESSION_ID`) it reports, validated against the daemon's live session registry. Identity is cooperative (the env value is trusted within the uid boundary, not adversarially authenticated). An id that does not match a live session SHALL leave the caller unidentified. Identity SHALL be re-validated lazily on each tool call to survive a connect-before-register race, and the binding SHALL be torn down on disconnect.

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

### Requirement: Directory is global-read within the uid

`list_sessions` and `get_session` SHALL NOT gate on caller identity: any same-uid caller that reaches the socket SHALL be able to read every live session's descriptor. This global-read posture SHALL be stated explicitly (not implied as a security boundary), consistent with the single-user threat model.

#### Scenario: Any same-uid caller reads the directory

- **WHEN** an identified or unidentified same-uid caller invokes `list_sessions`
- **THEN** the daemon SHALL return all live session descriptors (identity is not an access gate here)

### Requirement: whoami self-identification tool

The system SHALL expose a `whoami` MCP tool that returns the caller's own resolved session descriptor, or null when unidentified.

#### Scenario: Identified agent identifies itself

- **WHEN** an identified agent calls `whoami`
- **THEN** the tool SHALL return that session's id, name, workspace, and agent type

#### Scenario: Unidentified agent calls whoami

- **WHEN** an unidentified caller calls `whoami`
- **THEN** the tool SHALL return null (not a guessed identity)

### Requirement: Idempotent, reversible agent registration

The system SHALL register the `nergal mcp` shim in spawned agents' MCP configuration idempotently (no duplicate entries on re-run), pinning the installed absolute path `/usr/bin/nergal` (NOT a `$PATH` resolution, which would bake in the `~/.cargo/bin` shadow per CLAUDE.md). It SHALL register into every agent whose MCP-server config schema is supported — Claude Code (`~/.claude.json` `mcpServers`, JSON), Codex (`~/.codex/config.toml` `[mcp_servers.nergal]`, TOML edited format-preservingly), and OpenCode (`~/.config/opencode/opencode.json` `mcp.nergal` `{type:"local",command,enabled}`, JSON) — and SHALL skip an agent with no MCP-server mechanism (Pi: no `mcp` CLI/config surface). Per-agent registration SHALL be best-effort: a failure for one agent (or a missing/corrupt config) SHALL be logged and SHALL NOT block the others or the enable/disable toggle. It SHALL deregister at disable time on a best-effort basis (the app is running); it SHALL NOT attempt uninstall-time deregistration from maintainer scripts (multi-user `$HOME` is unreliable). An orphaned entry after uninstall SHALL degrade to a structured error at agent startup, not a hard agent failure.

#### Scenario: Registration is idempotent

- **WHEN** nergal registers the MCP server and the entry already exists
- **THEN** it SHALL NOT create a duplicate entry

#### Scenario: Per-agent registration preserves other config

- **WHEN** nergal registers into an agent config that already holds other MCP servers (and, for TOML, comments/formatting)
- **THEN** only the `nergal` entry SHALL be added/updated, leaving every other server, key, comment and whitespace intact

#### Scenario: An unsupported agent is skipped, a failing one does not block others

- **WHEN** an agent has no MCP-server config mechanism (Pi), OR one agent's config write fails
- **THEN** that agent SHALL be skipped/logged and the remaining agents SHALL still be registered, and the enable/disable toggle SHALL still succeed

#### Scenario: Deregistration on disable

- **WHEN** the MCP server is disabled
- **THEN** the `nergal mcp` entry SHALL be removed from agent configs so sessions do not try to spawn it

### Requirement: MCP server off by default

The setting `mcp_server_enabled` SHALL default to off for the initial release. The directory and tools SHALL be unavailable until the user opts in.

#### Scenario: Default off

- **WHEN** nergal runs with no prior MCP setting
- **THEN** the MCP server SHALL be disabled and not registered into agent configs

