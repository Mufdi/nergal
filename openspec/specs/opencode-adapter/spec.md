# opencode-adapter Specification

## Purpose
TBD - created by archiving change opencode-adapter. Update Purpose after archive.
## Requirements
### Requirement: OpenCode adapter implements AgentAdapter via HttpSse transport

The system SHALL ship an `OpenCodeAdapter` in `src-tauri/src/agents/opencode/mod.rs` that implements `AgentAdapter` and declares `Transport::HttpSse`. The adapter SHALL use `opencode serve` as its event source rather than spawning the OpenCode TUI in a PTY.

#### Scenario: Adapter declares HttpSse transport

- **WHEN** `OpenCodeAdapter::transport()` is called
- **THEN** it SHALL return `Transport::HttpSse { base_url, sse_path: "/event", permission_endpoint: "/session/:id/permissions/:pid" }`
- **AND** the `base_url` port SHALL be the port chosen by the supervisor at session start

#### Scenario: PTY layer is noop for OpenCode sessions

- **WHEN** `pty.rs::spawn_session(session_id, agent)` is called and `agent_id == 'opencode'`
- **THEN** no PTY child SHALL be created
- **AND** a no-pty session handle SHALL be returned to the session manager

### Requirement: Each session runs its own opencode serve instance

For every cluihud session bound to OpenCode, the system SHALL spawn a dedicated `opencode serve` child process on a per-session ephemeral port. Sessions SHALL NOT share a single `opencode serve` instance.

#### Scenario: Two concurrent OpenCode sessions

- **WHEN** the user has two simultaneous OpenCode sessions in cluihud
- **THEN** the supervisor SHALL spawn two `opencode serve` processes on two distinct ports
- **AND** each session's SSE client SHALL connect to its own server's `/event` endpoint

#### Scenario: Server lifecycle tied to session

- **WHEN** a cluihud OpenCode session is destroyed
- **THEN** the supervisor SHALL SIGTERM the corresponding `opencode serve` child within 5 seconds
- **AND** SHALL SIGKILL if it remains alive past the SIGTERM grace window

### Requirement: Port assignment is dynamic, not hardcoded

The supervisor SHALL request an ephemeral port from `opencode serve` (via `--port 0` or equivalent) and parse the chosen port from `opencode serve` stdout. Hardcoded ports (e.g., `:4096`) SHALL NOT be used.

#### Scenario: Port resolution from stdout

- **WHEN** supervisor spawns `opencode serve --port 0`
- **AND** `opencode serve` emits a stdout line matching the pattern `listening on http://127.0.0.1:<port>`
- **THEN** the supervisor SHALL extract the port and use it for SSE connection and permission POSTs

#### Scenario: Port range fallback

- **WHEN** `opencode serve` does not support `--port 0` (detected via stderr or version check)
- **THEN** the supervisor SHALL try ports in the range 49152–65535
- **AND** retry on `EADDRINUSE` until success or 10 attempts exhausted

### Requirement: SSE consumer normalizes events to the trait's TranscriptEvent

The SSE client SHALL parse events from `opencode serve`'s `/event` stream and translate them to `TranscriptEvent` and `BackendEvent` shapes consumed by the cluihud `EventSink`. The frontend SHALL NOT see SSE-specific shapes.

#### Scenario: tool.execute.before mapped to ToolUse

- **WHEN** an SSE event with type `tool.execute.before` arrives with payload `{ tool_name, input }`
- **THEN** the client SHALL emit `TranscriptEvent::ToolUse { name: tool_name, input }` to the sink

#### Scenario: permission.asked mapped to ask:user backend event

- **WHEN** an SSE event `permission.asked` arrives with payload `{ permission_id, prompt, options? }`
- **THEN** the client SHALL emit a backend `ask:user` event with `decision_path = http_endpoint_url`
- **AND** the frontend `AskUserModal` SHALL render with the prompt and options
- **AND** the modal's submit handler SHALL invoke `submit_ask_answer` which POSTs back

### Requirement: Permission responses go via REST POST, not FIFO

`submit_ask_answer(session_id, answers)` for OpenCode sessions SHALL invoke `POST /session/:opencode_session_id/permissions/:permission_id` on the per-session local server with the answer body.

#### Scenario: Ask answer translates to REST POST

- **WHEN** the user submits an answer to a pending OpenCode permission prompt
- **THEN** the backend SHALL POST to the OpenCode permission endpoint with the answer body
- **AND** the SSE stream SHALL deliver a `permission.replied` event confirming receipt

### Requirement: Adapter declares conservative capability set

`OpenCodeAdapter::capabilities()` SHALL return at minimum: `ASK_USER_BLOCKING`, `TOOL_CALL_EVENTS`, `STRUCTURED_TRANSCRIPT`. The flags `RAW_COST_PER_MESSAGE`, `TASK_LIST`, and `SESSION_RESUME` SHALL be declared based on empirical SSE schema mapping during the spike. The flag `PLAN_REVIEW` SHALL NOT be declared.

#### Scenario: Plan review unsupported

- **WHEN** `submit_plan_decision` is invoked on an OpenCode adapter
- **THEN** the call SHALL return `Err(AdapterError::NotSupported(PLAN_REVIEW))`
- **AND** the frontend PlanPanel SHALL be hidden for OpenCode sessions per capability gating

#### Scenario: Cost capability decided post-spike

- **WHEN** the spike (task 1.x) confirms or rejects the cost field shape on SSE
- **THEN** `RAW_COST_PER_MESSAGE` SHALL be added to or omitted from the capability flags accordingly
- **AND** the status bar token segment SHALL render or hide based on the resulting capability

### Requirement: Auto-detection scans OpenCode filesystem markers

`OpenCodeAdapter::detect()` SHALL return `installed: true` if any of the following hold:

- `~/.config/opencode/` directory exists
- `~/.local/share/opencode/` directory exists
- `opencode` binary is on PATH

#### Scenario: OpenCode installed via AppImage with no config yet

- **WHEN** the user has the `opencode` binary on PATH but has not run `opencode auth login` yet
- **THEN** `detect()` returns `installed: true` with `binary_path` set
- **AND** the agent picker shows OpenCode as available
- **AND** the settings panel shows a banner instructing the user to run `opencode auth login`

### Requirement: BYO credentials — cluihud does not store API keys

The OpenCode settings panel SHALL NOT contain inputs for provider API keys or tokens. Cluihud SHALL delegate credential management entirely to `opencode auth login` (executed by the user from a terminal).

#### Scenario: Settings panel guidance

- **WHEN** the user opens Settings → Agents → OpenCode
- **THEN** the panel SHALL display install status, detected version, and instructions to run `opencode auth login <provider>` from a terminal
- **AND** SHALL NOT contain any password input field

### Requirement: OpenCode sessions render chat panel, not terminal canvas

The frontend Workspace SHALL route the central content area based on `agent_id`. For OpenCode sessions, `<OpenCodeChat />` SHALL render in place of `<TerminalManager />`.

#### Scenario: Workspace renders chat for OpenCode

- **WHEN** the active session's `agent_id == 'opencode'`
- **THEN** `Workspace.tsx` SHALL render `<OpenCodeChat sessionId={...} />` in the central area
- **AND** SHALL NOT mount the terminal canvas for that session

#### Scenario: Workspace renders terminal for CC

- **WHEN** the active session's `agent_id == 'claude-code'`
- **THEN** `Workspace.tsx` SHALL render the terminal canvas as today

### Requirement: Orphaned opencode serve cleanup at startup

At app startup, the system SHALL scan `~/.local/state/cluihud/opencode-pids/` for PID files of `opencode serve` instances whose parent cluihud process is no longer alive, and SHALL kill those orphans.

#### Scenario: Cluihud crashed leaving opencode serve alive

- **WHEN** a previous cluihud run crashed without graceful shutdown
- **AND** an `opencode serve` child process is still running with its PID file present
- **THEN** at next cluihud startup, the supervisor SHALL detect the orphan and kill it before initializing new sessions

