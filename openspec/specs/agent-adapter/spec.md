# agent-adapter Specification

## Purpose
TBD - created by archiving change agent-adapter-foundation. Update Purpose after archive.

## Requirements

### Requirement: AgentAdapter trait declares identity, capabilities, and transport

Nergal SHALL expose an `AgentAdapter` trait in `src-tauri/src/agents/mod.rs` that every CLI agent integration MUST implement. The trait SHALL expose:

- A unique `AgentId` (string newtype with **validated** constructor `AgentId::new(s) -> Result<Self, AdapterError>` enforcing regex `^[a-z][a-z0-9-]{0,31}$`).
- A human-readable `display_name`.
- A `capabilities()` accessor returning the set of `AgentCapability` flags supported.
- A `transport()` accessor returning the `Transport` variant (FileHooks | HttpSse | JsonlTail | RpcStdio).
- A `requires_nergal_setup() -> bool` accessor (CC + Codex true; OpenCode + Pi false).
- A lightweight `detect()` async method returning `DetectionResult { installed, binary_path, config_path, version, trusted_for_project }` that MUST NOT spawn child processes.
- An optional `refresh_version()` async method that may invoke the binary's `--version` flag in the background; default impl returns None.
- A `spawn(ctx: &SpawnContext)` method returning `SpawnSpec { binary, args, env, prompt }` where env MUST include `NERGAL_SESSION_ID = ctx.session_id` and `prompt` carries the free-text launch prompt (`LaunchPrompt { text, attach }`) outside `args`.
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
- **THEN** it SHALL return a `SpawnSpec` whose binary, args, env and prompt are sufficient for the runtime to launch the agent
- **AND** `args` SHALL NOT contain the launch prompt text
- **AND** the env map SHALL include `NERGAL_SESSION_ID = ctx.session_id`

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

- **WHEN** the user installs OpenCode after nergal is already running
- **AND** invokes Settings → "Rescan agents" or `nergal rescan-agents` from a terminal
- **THEN** the registry re-detects and OpenCode appears in `availableAgentsAtom` without restart

### Requirement: Hook event resolution uses in-memory cache with DB fallback and drop-on-miss

Hook events arriving on the Unix socket carry `nergal_session_id`. The dispatcher SHALL resolve the corresponding `agent_id` via:

1. `agent_id_cache: DashMap<SessionId, AgentId>` (populated by `create_session_with_agent` BEFORE PTY spawn, removed in `destroy_session`).
2. Fallback: `db::get_session(session_id)?.agent_id` if cache miss.
3. If both miss: log `warn!("orphan hook event for session {}; dropping", session_id)` and return early. NO panic, NO buffering.

#### Scenario: Cache hit on hot path

- **WHEN** a hook event arrives for a session created during the current nergal run
- **THEN** `agent_id_cache.get(session_id)` SHALL hit and the dispatcher SHALL use the cached `AgentId`
- **AND** SHALL NOT query SQLite

#### Scenario: Cache miss recovers via DB

- **WHEN** nergal restarts and a hook event arrives for a session whose row exists in DB but cache is empty
- **THEN** the dispatcher SHALL fall back to DB lookup
- **AND** populate the cache for subsequent events

#### Scenario: Orphan hook event dropped with warning

- **WHEN** a hook event arrives with a `nergal_session_id` not in cache and not in DB
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

### Requirement: SpawnSpec injects NERGAL_SESSION_ID via the adapter, not the PTY layer

The `pty.rs` module SHALL NOT hardcode any agent-specific env vars. The `SpawnSpec.env` returned by the adapter SHALL include `NERGAL_SESSION_ID` and any other adapter-specific env (e.g., a config path override).

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

### Requirement: AgentCapability set includes THEME_SYNC

`AgentCapability` SHALL include a `THEME_SYNC` flag. Adapters that advertise `THEME_SYNC` MUST implement `apply_theme`. Adapters that do NOT advertise it SHALL inherit the default `apply_theme` impl returning `Err(NotSupported(THEME_SYNC))`.

#### Scenario: THEME_SYNC serializes on the wire

- **WHEN** an adapter with `THEME_SYNC` in its capabilities is queried by the frontend
- **THEN** the capability array SHALL include the string `"THEME_SYNC"`
- **AND** the frontend SHALL be able to gate UI affordances (e.g., a "theme sync" status indicator) on its presence

### Requirement: AgentAdapter exposes apply_theme

`AgentAdapter` SHALL expose:

```rust
async fn apply_theme(&self, palette: &ThemePalette) -> Result<(), AdapterError>;
```

The default implementation SHALL return `Err(AdapterError::NotSupported(AgentCapability::THEME_SYNC))`. Adapters that advertise `THEME_SYNC` MUST override the default with an implementation that best-effort applies the palette to the agent's native theme system.

`apply_theme` is best-effort by contract: implementers MUST NOT propagate errors that would surface as user-facing notifications. Errors SHALL be logged by the registry dispatcher (see "Registry dispatches apply_theme to capable adapters" below).

#### Scenario: Default apply_theme returns NotSupported

- **WHEN** an adapter that does not declare `THEME_SYNC` has `apply_theme(palette)` called
- **THEN** the call SHALL return `Err(AdapterError::NotSupported(AgentCapability::THEME_SYNC))`
- **AND** no filesystem writes SHALL be performed

#### Scenario: Adapter advertising THEME_SYNC overrides apply_theme

- **WHEN** an adapter declares `AgentCapability::THEME_SYNC` in `capabilities().flags`
- **THEN** the adapter MUST override `apply_theme` with an implementation that performs at least one of: write a theme file the agent reads at startup, write a hot-reloaded theme file the agent picks up live, or invoke a live theme-switch API exposed by the agent

### Requirement: ThemePalette struct passed across the IPC boundary

`ThemePalette` SHALL be a serializable struct defined in `src-tauri/src/agents/mod.rs` with the following fields:

- `id: String` — stable theme id (e.g. `"v1-dark"`, `"v11-tokyo-night"`, `"custom-<id>"`).
- `is_dark: bool` — true when surface luminance < 0.5.
- `surface: String` — terminal background as `#rrggbb`.
- `foreground: String` — terminal foreground as `#rrggbb`.
- `card: String` — raised panel color.
- `secondary: String` — muted surface.
- `muted_foreground: String` — secondary text.
- `border: String` — border color (may be rgba; adapters convert as needed).
- `accent: String` — primary accent color.

The struct SHALL serialize with `camelCase` field names for the TS boundary.

#### Scenario: ThemePalette round-trips through serde

- **WHEN** a `ThemePalette` is serialized to JSON and deserialized back
- **THEN** every field SHALL be preserved
- **AND** the JSON SHALL use `camelCase` keys (`isDark`, `mutedForeground`)

### Requirement: Registry dispatches apply_theme to capable adapters

`AgentRegistry` SHALL expose `async fn apply_theme_to_all(&self, palette: ThemePalette)` that iterates registered adapters, filters those whose `capabilities().flags.contains(THEME_SYNC)`, and invokes `apply_theme(&palette)` on each. Per-adapter errors SHALL be logged at `warn!` and SHALL NOT propagate.

A Tauri command `apply_theme_to_agents(palette: ThemePalette)` SHALL forward to this method. The frontend `applyTheme` flow SHALL invoke this command after the DOM `data-theme` mutation commits (next animation frame), with a 150ms trailing debounce.

#### Scenario: Non-capable adapter skipped

- **WHEN** `apply_theme_to_all` is called with one adapter declaring `THEME_SYNC` and one not declaring it
- **THEN** only the capable adapter's `apply_theme` SHALL be invoked
- **AND** the call to the non-capable adapter SHALL NOT happen

#### Scenario: Adapter failure does not block other adapters

- **WHEN** one adapter's `apply_theme` returns `Err(...)`
- **THEN** the registry SHALL log the error at `warn!` with the adapter id
- **AND** SHALL continue invoking `apply_theme` on the remaining capable adapters
- **AND** the top-level Tauri command SHALL return `Ok(())` regardless

### Requirement: SpawnContext carries launch options and adapters declare permission presets

`SpawnContext` SHALL include `launch_options: Option<&LaunchOptions>` where `LaunchOptions { permission_preset, allow_skip_in_cycle, startup_command }` (defined in `models.rs`, persisted on the session row as a JSON column). Adapters SHALL map `permission_preset` to their native flags inside `spawn()` and SHALL declare what they support via:

- `permission_presets() -> &'static [PermissionPreset]` — the presets the adapter maps to **verified** native flags. Default impl returns `[Default]` only. The UI SHALL offer only the declared presets, so unsupported values are unreachable instead of silently ignored.
- `supports_allow_skip_cycle() -> bool` — whether the adapter maps `allow_skip_in_cycle` to a real flag. Default `false`.

`PermissionPreset` SHALL be a single-value enum (`Default | Plan | AcceptEdits | Auto | Bypass`): one permission mode per session, mirroring Claude Code's `--permission-mode`, whose documented equivalence `--dangerously-skip-permissions ≡ --permission-mode bypassPermissions` makes "skip" a mode rather than a composable flag. `startup_command` SHALL be handled by the PTY layer (agent-agnostic shell prelude), never by adapters.

#### Scenario: CC maps every preset to its native flag

- **WHEN** `ClaudeCodeAdapter::spawn` runs with a `permission_preset` of Plan, AcceptEdits, Auto, or Bypass
- **THEN** the args SHALL contain `--permission-mode plan|acceptEdits|auto` respectively, or `--dangerously-skip-permissions` for Bypass
- **AND** with `allow_skip_in_cycle: true` and a non-Bypass preset, the args SHALL also contain `--allow-dangerously-skip-permissions`
- **AND** with `allow_skip_in_cycle: true` and the Bypass preset, the redundant allow flag SHALL be omitted

#### Scenario: Adapter without a mapping never sees the preset offered

- **WHEN** the agent picker renders launch options for an adapter whose `permission_presets()` returns only `[Default]` (e.g. Pi)
- **THEN** no preset rows SHALL be offered for that agent

#### Scenario: Launch options re-apply on resume

- **WHEN** a session with persisted launch options is resumed (same `start_claude_session` path as a fresh spawn)
- **THEN** the preset flags SHALL be re-applied to the launch command
- **AND** the startup command prelude SHALL run again in the fresh shell

### Requirement: Ask-user is notifier-only at the adapter boundary

The `AgentAdapter` trait SHALL NOT expose a blocking ask-user answer method; ask-user hook events SHALL only drive non-blocking attention signaling (pending-ask state, tab blink/tint), with the agent's own TUI owning the prompt. The `ASK_USER_BLOCKING` capability flag SHALL remain as a declaration gating attention UX, independent of any answer round-trip.

#### Scenario: ask-user event flows notify-only

- **WHEN** a `PreToolUse[AskUserQuestion]` hook event arrives for a session
- **THEN** the backend emits the pending-ask attention event and does not create or await any FIFO/answer channel

#### Scenario: no answer entry points remain

- **WHEN** searching the codebase after this change
- **THEN** there is no `submit_ask_answer` Tauri command, no `AgentAdapter::submit_ask_answer` method, and no ask-FIFO registration path

#### Scenario: attention UX unaffected

- **GIVEN** a session whose adapter declares `ASK_USER_BLOCKING`
- **WHEN** ask-user fires and later resolves (`PostToolUse`)
- **THEN** the tab blink/tint behavior driven by `pendingAsksAtom`/`pendingAttentionAtom` works exactly as before

### Requirement: Launch prompt reaches the agent without shell parsing

`SpawnSpec` SHALL carry the free-text launch prompt in a dedicated field, separate from `args`. Adapters that accept a launch prompt (Claude Code, Codex, OpenCode) SHALL use that field. On a POSIX shell, the PTY layer SHALL write the prompt to a file in the per-user `0700` runtime directory with mode `0600`. The command typed into the shell SHALL reference that file and SHALL NOT contain the prompt text. The shell SHALL expand the file content as one argument without parsing it, and SHALL delete the file as it reads it. Before delivery, Nergal SHALL remove NUL bytes and trailing newlines from the prompt; a prompt left empty counts as absent. The agent SHALL then receive the prompt byte-identical. On a unix host, a launch with a prompt into a non-POSIX shell (PowerShell, including `pwsh` on Linux) SHALL fail and roll back. On Windows, PowerShell and cmd MAY keep the quoted inline argument, and PowerShell quoting SHALL escape the typographic single quotes U+2018 to U+201B.

#### Scenario: Prompt with history-expansion triggers on continuation lines
- **GIVEN** an interactive bash whose Space key runs `magic-space`
- **AND** the launch prompt is `line one\nline two <!-- x --> !! and !# here\nline three: \`echo hi\` $(echo no) it's done`
- **WHEN** Nergal types the launch command into that shell and the agent binary records its last argument
- **THEN** the recorded argument SHALL equal the prompt byte for byte
- **AND** the shell SHALL print no `parse error` and no `event not found`

#### Scenario: Prompt text never appears in the typed command
- **GIVEN** a POSIX shell, no startup prelude, and the launch prompt `hello 'world' !# $(id)`
- **WHEN** Nergal builds the launch command
- **THEN** the command SHALL NOT contain `hello`, `!#` or `$(id)`
- **AND** the command SHALL contain the single-quoted path of the prompt file
- **AND** the command SHALL contain exactly one newline, at its end

#### Scenario: Prompt file is private and deleted by the launch
- **GIVEN** a POSIX shell and the launch prompt `plan`
- **WHEN** Nergal writes the prompt file before typing the command
- **THEN** the file SHALL have mode `0600` and SHALL hold exactly `plan`
- **AND** after the shell runs the typed command, the file SHALL no longer exist

#### Scenario: Missing prompt file skips the agent
- **GIVEN** a typed launch command whose prompt file no longer exists
- **WHEN** the shell runs that command again, for example from history
- **THEN** the agent binary SHALL NOT start

#### Scenario: PowerShell on a unix host refuses a launch prompt
- **GIVEN** a unix host whose PTY shell is `pwsh`, and the launch prompt `hello`
- **WHEN** Nergal prepares the launch
- **THEN** the preparation SHALL return an error and no prompt file SHALL exist
- **AND** no command containing `hello` SHALL be built

#### Scenario: PowerShell quoting escapes typographic quotes
- **GIVEN** the text `it’s ‘x’ ‚y‛`
- **WHEN** Nergal quotes it for PowerShell
- **THEN** each of U+2018, U+2019, U+201A and U+201B SHALL appear doubled inside one single-quoted string
- **AND** a text holding any of those quotes SHALL always be quoted

#### Scenario: Prompt is normalized before delivery
- **GIVEN** the launch prompt `a\u0000b\n\n`
- **WHEN** Nergal prepares the launch
- **THEN** the agent SHALL receive `ab`

#### Scenario: No prompt adds nothing
- **GIVEN** a session whose initial prompt is absent, empty, or only newlines
- **WHEN** Nergal builds the launch command
- **THEN** the command SHALL end with the last adapter arg followed by the newline
- **AND** no prompt file SHALL be created

#### Scenario: Leftover prompt files are swept at startup
- **GIVEN** the prompt directory holds two files from a crashed run
- **WHEN** Nergal starts
- **THEN** the prompt directory SHALL hold no files

### Requirement: Launch prompt is never read as an agent option

The launch prompt SHALL reach the agent as prompt text even when it starts with `-`. The Claude Code and Codex launch commands SHALL place `--` before the prompt argument. The OpenCode launch command SHALL attach the prompt to its flag in the `--prompt=<value>` form.

#### Scenario: Option-like prompt for Claude Code
- **GIVEN** the Claude Code adapter and the launch prompt `--settings={"hooks":{}}`
- **WHEN** Nergal builds the launch command for a POSIX shell
- **THEN** the token `--` SHALL come right before the prompt reference
- **AND** the prompt reference SHALL be the last word of the command

#### Scenario: Option-like prompt for OpenCode
- **GIVEN** the OpenCode adapter and the launch prompt `--help`
- **WHEN** Nergal builds the launch command for a POSIX shell
- **THEN** the command SHALL attach the prompt reference to `--prompt=` with no space between them

### Requirement: A failed launch leaves no trace and keeps the prompt

Any error in `start_claude_session` after the PTY reservation SHALL leave no reservation, no live mark, no shell, no prompt file and no vault write for that launch, and SHALL keep the prompt. Before it returns the error, Nergal SHALL remove the reservation while this launch owns it, mark the session not live, kill the shell spawned for this launch, delete this launch's prompt file, and put the consumed prompt back in the session's prompt queue unless a newer prompt is already queued. At a shell's end of file, Nergal SHALL finalize the session in Obsidian only when that launch wrote its command to the PTY. Nergal SHALL NOT type the prompt into the shell on any failure. User decision 2026-11-23.

#### Scenario: Prompt file cannot be written
- **GIVEN** a session S reserved by launch L with the consumed prompt `hello`, and a prompt directory whose parent path is a regular file
- **WHEN** the launch delivery runs and fails
- **THEN** the start SHALL return an error and nothing SHALL be written to the PTY
- **AND** S SHALL NOT be live and SHALL hold no PTY reservation
- **AND** the queued prompt for S SHALL be `hello`

#### Scenario: Writing the command to the PTY fails
- **GIVEN** a launch whose prompt file was written and whose PTY writer returns an error
- **WHEN** the launch delivery runs
- **THEN** the start SHALL return an error
- **AND** the prompt file SHALL no longer exist
- **AND** the queued prompt for the session SHALL be the consumed prompt

#### Scenario: The session was killed during startup
- **GIVEN** a launch whose PTY instance was removed by a kill while the launch waited for the shell
- **WHEN** the launch delivery runs
- **THEN** no prompt file SHALL remain and the start SHALL return an error

#### Scenario: A newer queued prompt wins
- **GIVEN** a failed launch that consumed `old`, and a prompt `new` queued for the same session during the launch
- **WHEN** the rollback runs
- **THEN** the queued prompt for the session SHALL be `new`

#### Scenario: A launch that never wrote its command writes nothing to the vault
- **GIVEN** a launch L that ended before writing its command: rolled back, closed during startup (`Gone`), or killed before delivery
- **WHEN** the shell of L reaches end of file
- **THEN** Nergal SHALL NOT finalize the session in Obsidian
- **AND** a later launch of the same session that writes its command SHALL be finalized when its PTY ends

#### Scenario: Retry after a failed launch spawns a new shell
- **GIVEN** a start of session S that failed and was rolled back
- **WHEN** Nergal starts S again
- **THEN** the reservation SHALL be free, so the second start SHALL spawn a new PTY with a new PTY id
- **AND** the second start SHALL consume the requeued prompt
