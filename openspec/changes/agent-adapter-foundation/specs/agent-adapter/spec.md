## ADDED Requirements

### Requirement: AgentAdapter trait declares identity, capabilities, and transport

Cluihud SHALL expose an `AgentAdapter` trait in `src-tauri/src/agents/mod.rs` that every CLI agent integration MUST implement. The trait SHALL expose:

- A unique `AgentId` (string newtype with **validated** constructor `AgentId::new(s) -> Result<Self, AdapterError>` enforcing regex `^[a-z][a-z0-9-]{0,31}$`).
- A human-readable `display_name`.
- A `capabilities()` accessor returning the set of `AgentCapability` flags supported.
- A `transport()` accessor returning the `Transport` variant (FileHooks | HttpSse | JsonlTail | RpcStdio).
- A `requires_cluihud_setup() -> bool` accessor (CC + Codex true; OpenCode + Pi false).
- A lightweight `detect()` async method returning `DetectionResult { installed, binary_path, config_path, version, trusted_for_project }` that MUST NOT spawn child processes.
- An optional `refresh_version()` async method that may invoke the binary's `--version` flag in the background; default impl returns None.
- A `spawn(ctx: &SpawnContext)` method returning `SpawnSpec { binary, args, env }` where env MUST include `CLUIHUD_SESSION_ID = ctx.session_id`.
- A sync `parse_transcript_line(line)` method returning `Option<TranscriptEvent>`.
- A `start_event_pump(session_id, sink)` async method that begins listening for the agent's events using its declared transport, forwarding into the shared `EventSink`.
- A `stop_event_pump(session_id)` async method (idempotent) that terminates background tasks started by start_event_pump.
- An async `submit_plan_decision(session_id, decision)` returning `Err(NotSupported(PLAN_REVIEW))` if the adapter lacks `PLAN_REVIEW`. Default impl returns NotSupported.
- An async `submit_ask_answer(session_id, answers)` returning `Err(NotSupported(ASK_USER_BLOCKING))` if the adapter lacks `ASK_USER_BLOCKING`. Default impl returns NotSupported.

#### Scenario: AgentId rejects invalid identifiers

- **WHEN** `AgentId::new("../etc/passwd")` is called
- **THEN** the call SHALL return `Err(AdapterError::InvalidAgentId(...))`
- **AND** SHALL NOT construct an `AgentId` value

#### Scenario: AgentId accepts valid identifiers

- **WHEN** `AgentId::new("opencode")` or `AgentId::new("claude-code")` is called
- **THEN** the call SHALL return `Ok(AgentId)`

#### Scenario: Adapter declares capabilities at registration

- **WHEN** the registry calls `AgentAdapter::capabilities()` on a registered adapter
- **THEN** the returned `AgentCapabilities` SHALL list every `AgentCapability` flag the adapter actually services
- **AND** absent flags SHALL signal that calls to the corresponding methods MAY return `Err(NotSupported)`

#### Scenario: Spawn returns transport-appropriate process spec

- **WHEN** `spawn(ctx)` is called with a fresh `SpawnContext`
- **THEN** it SHALL return a `SpawnSpec` whose binary, args, and env are sufficient for the runtime to launch the agent
- **AND** the env map SHALL include `CLUIHUD_SESSION_ID = ctx.session_id`

#### Scenario: Adapter without PLAN_REVIEW rejects plan decisions

- **WHEN** `submit_plan_decision` is called on an adapter whose `capabilities().contains(PLAN_REVIEW)` is false
- **THEN** the call SHALL return `Err(AdapterError::NotSupported { capability: PLAN_REVIEW })`
- **AND** no FIFO write or RPC call SHALL be performed

### Requirement: AgentCapability is a bitflags set, not an open trait

`AgentCapability` SHALL be defined using the `bitflags` macro and SHALL include exactly these flags in v1: `PLAN_REVIEW`, `ASK_USER_BLOCKING`, `TOOL_CALL_EVENTS`, `STRUCTURED_TRANSCRIPT`, `RAW_COST_PER_MESSAGE`, `TASK_LIST`, `SESSION_RESUME`, `ANNOTATIONS_INJECT`. Adding new flags SHALL be a non-breaking change for adapters that don't claim them.

#### Scenario: Bitflags allow O(1) capability checks

- **WHEN** UI code asks "does the active agent support PLAN_REVIEW?"
- **THEN** the check SHALL be `capabilities.flags.contains(AgentCapability::PLAN_REVIEW)` returning bool

#### Scenario: Capabilities serialize as string array on the wire

- **WHEN** the backend emits the agent metadata to the frontend
- **THEN** capabilities SHALL serialize as a `Vec<&'static str>` (e.g. `["PLAN_REVIEW", "TASK_LIST"]`)
- **AND** the frontend SHALL parse them into a `Set<string>` for O(1) `has()` checks

### Requirement: AgentRegistry holds adapters with priority ordering and runs lightweight detection

`AgentRegistry` SHALL be a process-wide singleton (held in Tauri app state) that registers `Arc<dyn AgentAdapter>` instances at app setup, exposes lookup by `AgentId`, rejects duplicate ID registrations with `Err(DuplicateAgentId)`, provides a `scan()` method that calls `detect()` on every registered adapter (lightweight, no child spawn), and exposes `priority_list()` returning the canonical agent priority order: `[claude-code, codex, opencode, pi]`.

#### Scenario: All registered adapters scanned on startup, version refresh deferred

- **WHEN** `AgentRegistry::scan()` is invoked at app startup
- **THEN** every registered adapter's `detect()` is awaited (filesystem checks only, no child spawn)
- **AND** the result populates the frontend `availableAgentsAtom`
- **AND** `refresh_version()` is dispatched as a background tokio task per detected adapter, updating the metadata atom asynchronously when complete

#### Scenario: Detect must not spawn child processes

- **WHEN** `ClaudeCodeAdapter::detect()` runs at startup
- **THEN** it SHALL only do filesystem checks (`~/.claude/`, `which claude`)
- **AND** SHALL NOT invoke `claude --version` (that happens in `refresh_version()` async)

#### Scenario: Duplicate AgentId rejected

- **WHEN** `AgentRegistry::register(adapter)` is called with an `AgentId` already present
- **THEN** the call SHALL return `Err(AdapterError::DuplicateAgentId(...))`
- **AND** the existing adapter SHALL remain in place

#### Scenario: Adapter not detected is still listed but disabled

- **WHEN** an adapter's `detect()` returns `installed: false`
- **THEN** the registry MUST still list the adapter as known
- **AND** the agent picker SHALL show it greyed out with an install hint or link

### Requirement: Manual rescan command refreshes detection

The system SHALL expose a Tauri command `rescan_agents()` that re-runs `AgentRegistry::scan()` and updates the frontend store. There SHALL NOT be a continuous filesystem watcher polling for agent installation changes (cost vs benefit not justified).

#### Scenario: User installs an agent post-startup

- **WHEN** the user installs OpenCode after cluihud is already running
- **AND** invokes Settings → "Rescan agents" or `cluihud rescan-agents` from a terminal
- **THEN** the registry re-detects and OpenCode appears in `availableAgentsAtom` without restart

### Requirement: Hook event resolution uses in-memory cache with DB fallback and drop-on-miss

Hook events arriving on the Unix socket carry `cluihud_session_id`. The dispatcher SHALL resolve the corresponding `agent_id` via:

1. `agent_id_cache: DashMap<SessionId, AgentId>` (populated by `create_session_with_agent` BEFORE PTY spawn, removed in `destroy_session`).
2. Fallback: `db::get_session(session_id)?.agent_id` if cache miss.
3. If both miss: log `warn!("orphan hook event for session {}; dropping", session_id)` and return early. NO panic, NO buffering.

#### Scenario: Cache hit on hot path

- **WHEN** a hook event arrives for a session created during the current cluihud run
- **THEN** `agent_id_cache.get(session_id)` SHALL hit and the dispatcher SHALL use the cached `AgentId`
- **AND** SHALL NOT query SQLite

#### Scenario: Cache miss recovers via DB

- **WHEN** cluihud restarts and a hook event arrives for a session whose row exists in DB but cache is empty
- **THEN** the dispatcher SHALL fall back to DB lookup
- **AND** populate the cache for subsequent events

#### Scenario: Orphan hook event dropped with warning

- **WHEN** a hook event arrives with a `cluihud_session_id` not in cache and not in DB
- **THEN** the dispatcher SHALL log a warning and return without dispatching
- **AND** SHALL NOT panic, SHALL NOT mutate any atom

### Requirement: Sessions are bound to one agent for their lifetime

Every session row in the `sessions` table SHALL include `agent_id TEXT NOT NULL` and `agent_internal_session_id TEXT NULL` (used by Pi/Codex for resume). The agent SHALL NOT be changeable after session creation. Existing sessions migrated from a pre-foundation version SHALL default to `agent_id = 'claude-code'` and `agent_internal_session_id = NULL`.

#### Scenario: Pre-existing CC session migrates with default agent_id

- **WHEN** the DB migration adding the `agent_id` column runs
- **THEN** all existing rows SHALL acquire `agent_id = 'claude-code'`
- **AND** their behavior post-migration SHALL be identical to pre-migration

#### Scenario: Cannot switch agent on an active session

- **WHEN** the frontend tries to invoke `set_session_agent(sessionId, newAgentId)` on a session that is not freshly created
- **THEN** the backend SHALL return `Err(SessionLocked)` and emit no state change

### Requirement: Capability-gated rendering with synchronous capability population

Frontend Jotai stores SHALL check the active session's capability set before mutating dependent atoms when handling backend events. The capability set SHALL be populated **synchronously** from the session row payload at activation, NOT via a separate async invoke. Components rendering capability-bound UI SHALL guard at the component root and return `null` when the capability is absent.

#### Scenario: Capabilities arrive synchronously with session activation

- **WHEN** the backend emits `session:activated` (or returns a `Session` from `list_sessions` / `get_session`)
- **THEN** the payload SHALL include `agent_id: string` AND `agent_capabilities: string[]`
- **AND** the frontend SHALL populate `agentCapabilitiesAtom` from the array directly without any additional invoke
- **AND** subsequent hook listeners armed in the same activation tick SHALL see the populated capability set

#### Scenario: Plan event arrives for an agent without PLAN_REVIEW

- **WHEN** the backend emits `plan:ready` for a session whose adapter lacks `PLAN_REVIEW` (defense-in-depth — should not normally occur because the backend dispatcher gates by capability before emitting)
- **THEN** the frontend listener in `stores/hooks.ts` SHALL drop the event
- **AND** SHALL log a warning to the dev console

#### Scenario: PlanPanel hides for adapter without PLAN_REVIEW

- **WHEN** the active session's adapter does not declare `PLAN_REVIEW`
- **THEN** `<PlanPanel>` SHALL return `null` at the top of its render
- **AND** the panel toggle in the topbar SHALL be hidden or disabled

#### Scenario: Race during cold-start session activation

- **WHEN** the frontend tries to mount session UI before `session:activated` has resolved capabilities
- **THEN** hook listeners SHALL queue events into a per-session in-memory buffer (max 100 events)
- **AND** flush the buffer once `agent_capabilities` arrives
- **AND** drop events for capabilities still absent after flush

### Requirement: SpawnSpec injects CLUIHUD_SESSION_ID via the adapter, not the PTY layer

The `pty.rs` module SHALL NOT hardcode any agent-specific env vars. The `SpawnSpec.env` returned by the adapter SHALL include `CLUIHUD_SESSION_ID` and any other adapter-specific env (e.g., a config path override).

#### Scenario: PTY spawn uses adapter env

- **WHEN** `pty.rs::spawn_session(session_id, agent)` runs
- **THEN** the resulting child process env SHALL include all entries in `SpawnSpec.env`
- **AND** SHALL NOT add any `claude`-specific or `~/.claude/`-specific values from outside the adapter

### Requirement: Cost extractor emits raw tokens; aggregator owns running totals

The trait method `parse_transcript_line(line)` SHALL emit `TranscriptEvent::Cost(RawCost { model_id, input_tokens, output_tokens, cache_read_tokens, cache_write_tokens })` per line when the line carries usage fields. **Per-session running totals** SHALL be owned by a generic `SessionCostAggregator` (in `src-tauri/src/agents/cost_aggregator.rs`), NOT by any adapter. The dispatcher SHALL invoke `aggregator.add(&raw)` whenever a Cost event arrives.

#### Scenario: Per-line cost emission

- **WHEN** a transcript line contains usage fields
- **THEN** the adapter's `parse_transcript_line` SHALL emit one `TranscriptEvent::Cost(RawCost { ... })` for that line
- **AND** SHALL NOT compute or emit USD

#### Scenario: Aggregator accumulates across lines

- **WHEN** three consecutive lines emit Cost events with input_tokens 100, 200, 300
- **THEN** `aggregator.snapshot().input_tokens` SHALL be 600 after all three are processed
- **AND** `aggregator.snapshot().messages_counted` SHALL be 3

### Requirement: USD calculation preserved for CC via private legacy bridge

To prevent UX regression for existing CC users, the `ClaudeCodeAdapter` SHALL retain a private `legacy_usd_for_sonnet4(raw: &RawCost) -> f64` function reproducing the previous Sonnet 4 USD math. A Tauri command `get_session_cost_usd(session_id) -> Option<f64>` SHALL return `Some(usd)` for CC sessions and `None` for other adapters. The status bar SHALL render USD only when this command returns `Some`. The hardcoded constants SHALL move into the CC adapter's private namespace; the trait core SHALL NOT expose USD.

#### Scenario: CC status bar shows USD post-foundation

- **WHEN** a CC session is active and cost events have been processed
- **THEN** `get_session_cost_usd(session_id)` SHALL return `Some(usd)` computed via `legacy_usd_for_sonnet4(snapshot)`
- **AND** the status bar SHALL render the USD value as it did pre-foundation

#### Scenario: Non-CC session shows tokens only

- **WHEN** a non-CC session is active and cost events arrived (if the adapter declared `RAW_COST_PER_MESSAGE`)
- **THEN** `get_session_cost_usd(session_id)` SHALL return `None`
- **AND** the status bar SHALL render token totals (input/output/cache) but no USD value

### Requirement: OpenSpec reader is agent-agnostic

The OpenSpec watcher and reader (`src-tauri/src/openspec.rs`, post-move from `claude/openspec.rs`) SHALL NOT be part of any `AgentAdapter` implementation. Any session in a project with an `openspec/` directory SHALL surface OpenSpec content regardless of agent.

#### Scenario: OpenSpec panel renders for non-CC session

- **WHEN** an OpenCode (or Pi or Codex) session is active in a project containing `openspec/specs/<id>/spec.md`
- **THEN** the OpenSpec panel SHALL render its contents
- **AND** the FileWatcher SHALL emit `openspec:changed` events to the frontend exactly as it does for CC sessions
