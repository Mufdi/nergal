## ADDED Requirements

### Requirement: Pi adapter implements AgentAdapter via JsonlTail transport

The system SHALL ship a `PiAdapter` in `src-tauri/src/agents/pi/mod.rs` that implements `AgentAdapter` and declares `Transport::JsonlTail`. The adapter SHALL spawn the Pi binary in a PTY (TUI nativo visible) and SHALL run a separate file watcher that tails the Pi-generated JSONL session log.

#### Scenario: Adapter declares JsonlTail transport

- **WHEN** `PiAdapter::transport()` is called
- **THEN** it SHALL return `Transport::JsonlTail { sessions_dir: ~/.pi/agent/sessions }`

#### Scenario: PTY spawn for Pi is active (unlike OpenCode)

- **WHEN** `pty.rs::spawn_session(session_id, agent)` runs and `agent_id == 'pi'`
- **THEN** the PTY layer SHALL spawn `pi` (or the configured binary path) with the args from `SpawnSpec`
- **AND** the Pi TUI SHALL be visible in the terminal canvas as for CC sessions

### Requirement: JSONL session file resolution per cwd

The adapter SHALL derive Pi's session storage path from the session's `cwd` using Pi's slashes-to-dashes encoding convention, then poll for the newest `.jsonl` file in that directory after spawning Pi.

#### Scenario: Session JSONL discovered after spawn

- **WHEN** Pi is spawned for a session with `cwd = /home/user/projects/foo`
- **THEN** the adapter SHALL look in `~/.pi/agent/sessions/--home-user-projects-foo--/`
- **AND** SHALL poll every 100ms for up to 2 seconds for a new `.jsonl` file
- **AND** the newest file SHALL be selected as the session's JSONL

#### Scenario: JSONL not found within timeout

- **WHEN** no `.jsonl` file appears within 2 seconds after Pi spawn
- **THEN** the adapter SHALL log an error and surface a toast "Pi session JSONL not found — events will not stream"
- **AND** the session continues with TUI but without activity drawer / cost updates

### Requirement: Tail-f semantics with offset tracking

The JSONL tail SHALL read existing content on first open (catch-up), then continue reading from the last offset on each notify modify event. Lines SHALL NOT be re-emitted across modify events.

#### Scenario: Catch-up on open

- **WHEN** the JSONL file already contains 50 lines when the tail opens
- **THEN** the tail SHALL parse and emit events for all 50 lines once
- **AND** SHALL track offset = file size at end of catch-up

#### Scenario: Append-only follow

- **WHEN** Pi appends a new line to the JSONL after the tail is initialized
- **THEN** notify SHALL emit a modify event
- **AND** the tail SHALL read from `last_offset` to EOF
- **AND** emit one event per new line, then update `last_offset`

### Requirement: Capability set for Pi excludes plan and ask-user

`PiAdapter::capabilities()` SHALL declare exactly: `TOOL_CALL_EVENTS`, `STRUCTURED_TRANSCRIPT`, `RAW_COST_PER_MESSAGE`, `SESSION_RESUME`. The flags `PLAN_REVIEW`, `ASK_USER_BLOCKING`, `ANNOTATIONS_INJECT`, and `TASK_LIST` SHALL NOT be declared.

#### Scenario: PlanPanel and AskUserModal hidden

- **WHEN** the active session's `agent_id == 'pi'`
- **THEN** the foundation's capability gating SHALL hide `<PlanPanel>` and `<AskUserModal>`
- **AND** no Pi-specific frontend code SHALL be needed for this hiding

#### Scenario: Plan or ask-user invocation rejected

- **WHEN** `submit_plan_decision` or `submit_ask_answer` is invoked on a Pi adapter (defensive scenario, should not happen with correct gating)
- **THEN** the call SHALL return `Err(AdapterError::NotSupported(...))`
- **AND** no I/O SHALL be performed

### Requirement: Cost extraction returns raw tokens, drops Pi's USD

Pi's transcript exposes a rich `usage` object on assistant messages including `cost: { input, output, cacheRead, cacheWrite, total }` in USD. The adapter SHALL extract only the token counts (`input`, `output`, `cacheRead`, `cacheWrite`) into `RawCost` and SHALL discard the `cost` USD object. This aligns with foundation Decision 6 (pricing fuera del adapter).

#### Scenario: Assistant message with usage emits Cost event

- **WHEN** a JSONL line contains `{"type": "agent", "role": "assistant", "usage": {"input": 1234, "output": 567, "cacheRead": 89, "cacheWrite": 0, "totalTokens": ..., "cost": {...}}, "model": "claude-3-5-sonnet"}`
- **THEN** the parser SHALL emit `TranscriptEvent::Cost(RawCost { input_tokens: 1234, output_tokens: 567, cache_read_tokens: 89, cache_write_tokens: 0, model_id: Some("claude-3-5-sonnet") })`
- **AND** the `cost.usd` object SHALL NOT propagate to the frontend

### Requirement: Tool call and tool result events map to TranscriptEvent

The parser SHALL emit `ToolUse` and `ToolResult` events for `tool_call` and `tool_result` JSONL entries respectively.

#### Scenario: tool_call mapping

- **WHEN** a line contains `{"type": "tool_call", "id": "tc_abc", "name": "Bash", "arguments": {"command": "ls"}}`
- **THEN** the parser SHALL emit `TranscriptEvent::ToolUse { name: "Bash", input: {"command": "ls"} }`

#### Scenario: tool_result mapping

- **WHEN** a line contains `{"type": "tool_result", "toolCallId": "tc_abc", "toolName": "Bash", "output": {...}}`
- **THEN** the parser SHALL emit `TranscriptEvent::ToolResult { tool_use_id: "tc_abc", output: {...} }`

### Requirement: Session resume via Pi UUID

When a Pi session is created in cluihud, the adapter SHALL extract Pi's session UUID from the JSONL header (the `session` entry on line 1) and persist it in the cluihud session row's `agent_internal_session_id` column. On resume, the adapter SHALL pass `--resume <uuid>` as a spawn arg.

#### Scenario: UUID extraction from header

- **WHEN** the JSONL tail reads its first line
- **AND** the line is `{"type": "session", "version": 3, "id": "uuid-1234-...", "timestamp": ..., "cwd": ...}`
- **THEN** the adapter SHALL persist `id` to the cluihud session row's `agent_internal_session_id`

#### Scenario: Resume invokes pi --resume

- **WHEN** the user resumes a Pi session in cluihud
- **AND** the cluihud session row has `agent_internal_session_id = "uuid-1234-..."`
- **THEN** `spawn(ctx)` SHALL return `SpawnSpec { binary: pi, args: ["--resume", "uuid-1234-..."], env: {...} }`

### Requirement: Auto-detection scans Pi state directory and PATH

`PiAdapter::detect()` SHALL return `installed: true` if `~/.pi/agent/` exists OR `pi` is on PATH.

#### Scenario: Pi installed but never run

- **WHEN** `pi` is on PATH but `~/.pi/agent/` does not exist (binary installed via `npm install -g`, never executed)
- **THEN** `detect()` returns `installed: true` with `binary_path` set, `config_path: None`
- **AND** the agent picker shows Pi as available

### Requirement: Settings panel for Pi is read-only with credential delegation

The Pi section of the AgentsSettings panel SHALL show install status, version, and link to Pi documentation for credential configuration. There SHALL NOT be any setup/install button (Pi has no hook config) and SHALL NOT contain any credential input fields.

#### Scenario: Pi settings panel layout

- **WHEN** the user opens Settings → Agents → Pi
- **THEN** the panel SHALL display: detection status, detected version, a banner stating "Pi has no plan mode or ask-user blocking — observation only", and a link to Pi docs

### Requirement: No setup_agent action for Pi

The `setup_agent('pi')` Tauri command SHALL return early with success and a no-op log message. There SHALL NOT be any filesystem write to Pi's config directory by cluihud.

#### Scenario: setup_agent('pi') invoked from settings

- **WHEN** the frontend invokes `setup_agent('pi')`
- **THEN** the backend SHALL log "Pi requires no cluihud setup; configure credentials via Pi's own flow" and return Ok
- **AND** no file SHALL be written
