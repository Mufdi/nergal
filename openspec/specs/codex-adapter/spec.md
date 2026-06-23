# codex-adapter Specification

## Purpose
TBD - created by archiving change codex-adapter. Update Purpose after archive.
## Requirements
### Requirement: Codex adapter implements AgentAdapter via FileHooks transport

The system SHALL ship a `CodexAdapter` in `src-tauri/src/agents/codex/mod.rs` that implements `AgentAdapter` and declares `Transport::FileHooks { settings_path: ~/.codex/hooks.json, hook_event_names: [...] }`. The adapter SHALL reuse the foundation's Unix socket and dispatch infrastructure — no new transport machinery.

#### Scenario: Adapter declares FileHooks transport with Codex paths

- **WHEN** `CodexAdapter::transport()` is called
- **THEN** it SHALL return `Transport::FileHooks { settings_path: PathBuf::from("~/.codex/hooks.json"), hook_event_names: ["SessionStart", "SessionEnd", "PreToolUse", "PostToolUse", "PermissionRequest", "UserPromptSubmit", "Stop"] }`

#### Scenario: Hook event arrives via shared Unix socket

- **WHEN** Codex fires a hook (e.g., `PreToolUse`) configured by `setup_agent('codex')`
- **AND** the hook command is `nergal hook send pre-tool --agent codex`
- **THEN** the existing `/tmp/nergal.sock` server SHALL receive the JSON payload
- **AND** the foundation's dispatcher SHALL resolve `agent_id == 'codex'` from the `nergal_session_id` and route the parse step to `CodexAdapter`

### Requirement: setup_agent('codex') writes hooks.json with nergal entries

The system SHALL provide `setup_agent('codex')` that writes `~/.codex/hooks.json` with the nergal hook entries listed in design.md. Existing non-nergal hook entries SHALL be preserved (conservative merge).

#### Scenario: Setup on fresh Codex install

- **WHEN** `setup_agent('codex')` runs and `~/.codex/hooks.json` does not exist
- **THEN** the file SHALL be created with the full set of nergal hook entries

#### Scenario: Setup with existing user hooks

- **WHEN** `setup_agent('codex')` runs and `~/.codex/hooks.json` contains user-defined hooks under `PostToolUse` that are not nergal's
- **THEN** the nergal entries SHALL be added alongside without removing the user's
- **AND** any obsolete nergal entries from older versions SHALL be cleaned

### Requirement: Codex adapter capabilities exclude plan review

`CodexAdapter::capabilities()` SHALL declare at minimum: `ASK_USER_BLOCKING`, `TOOL_CALL_EVENTS`, `STRUCTURED_TRANSCRIPT`, `SESSION_RESUME`. The flags `RAW_COST_PER_MESSAGE`, `TASK_LIST`, and `ANNOTATIONS_INJECT` SHALL be set based on empirical schema mapping during the spike. The flag `PLAN_REVIEW` SHALL NOT be declared.

#### Scenario: Plan panel hidden for Codex

- **WHEN** the active session's `agent_id == 'codex'`
- **THEN** `<PlanPanel>` SHALL return null (per foundation capability gating)
- **AND** `submit_plan_decision` on this adapter SHALL return `Err(NotSupported(PLAN_REVIEW))`

### Requirement: PermissionRequest mapped to ask:user backend event

When Codex's `PermissionRequest` hook fires and `nergal hook ask-user --agent codex` is invoked, the adapter SHALL translate the payload into a backend `ask:user` event consumed by the existing AskUserModal frontend.

#### Scenario: PermissionRequest becomes ask:user

- **WHEN** Codex fires PermissionRequest with payload `{ session_id, tool_name, tool_input: {...}, ... }`
- **THEN** the adapter SHALL emit `ask:user` with `{ session_id, decision_path: <fifo>, prompt: <derived from tool_name + tool_input>, options: ["allow", "deny"] }`
- **AND** the AskUserModal SHALL render with the prompt and the two-option choice

#### Scenario: User submits decision; FIFO unblocks Codex hook

- **WHEN** the user submits "allow" via AskUserModal for a Codex session
- **THEN** the frontend SHALL invoke `submit_ask_answer(session_id, { permissionDecision: "allow" })`
- **AND** the adapter SHALL write the response JSON to `/tmp/nergal-ask-<pid>.fifo`
- **AND** the blocked `nergal hook ask-user` subprocess SHALL output the response JSON to stdout, which Codex consumes as the hook return value

### Requirement: Auto-detection scans Codex filesystem markers

`CodexAdapter::detect()` SHALL return `installed: true` if `~/.codex/` directory exists OR `codex` is on PATH. Detection result SHALL include a `trusted_for_project: Option<bool>` field reflecting whether Codex trusts the active project (best-effort heuristic).

#### Scenario: Codex installed but not trusted for current project

- **WHEN** `codex` is on PATH and `~/.codex/` exists
- **AND** the active project does not have a Codex trust marker
- **THEN** `detect()` returns `installed: true` with `trusted_for_project: Some(false)`
- **AND** the frontend SHALL render a banner in the Codex session row indicating trust is required

### Requirement: Session resume via UUID from rollout filename

The adapter SHALL extract Codex's session UUID from the rollout filename `~/.codex/sessions/YYYY/MM/DD/rollout-<uuid>.jsonl` immediately after session start, and persist it in the nergal session row's `agent_internal_session_id` column. On resume, `spawn(ctx)` SHALL invoke `codex resume <uuid>`.

#### Scenario: Rollout file resolution post-spawn

- **WHEN** a Codex session is spawned at time T
- **THEN** the adapter SHALL watch `~/.codex/sessions/YYYY/MM/DD/` for new rollout files appearing within 2 seconds of T
- **AND** the closest-mtime match SHALL be selected as the session's rollout
- **AND** the UUID SHALL be extracted from the filename and persisted

#### Scenario: Resume invokes codex resume

- **WHEN** the user resumes a Codex session in nergal
- **AND** `agent_internal_session_id == "abc-123"`
- **THEN** `spawn(ctx)` returns `SpawnSpec { binary: codex, args: ["resume", "abc-123"], env: { ... } }`

### Requirement: Cost capture follows foundation Decision 6

If the rollout schema includes per-message token usage fields (confirmed in spike), `CodexAdapter::parse_transcript_line()` SHALL emit `Cost(RawCost { ... })` events. USD calculation SHALL NOT happen in this adapter. If cache token fields are absent (OpenAI APIs may not report them in the same shape as Anthropic), they SHALL default to 0 in `RawCost`.

#### Scenario: Rollout entry with usage fields

- **WHEN** a rollout line contains `{"type": "message", "role": "assistant", "content": "...", "usage": {"prompt_tokens": 1234, "completion_tokens": 567, "total_tokens": 1801}}`
- **AND** the spike confirmed this shape exists
- **THEN** the parser SHALL emit `TranscriptEvent::Cost(RawCost { input_tokens: 1234, output_tokens: 567, cache_read_tokens: 0, cache_write_tokens: 0, model_id: <from elsewhere or None> })`

### Requirement: Trust-gate banner surfaces in Codex session UI

For Codex sessions in projects where the trust-gate has not been satisfied, the nergal UI SHALL display a banner (e.g., in the SessionRow or top of the workspace) instructing the user to run `codex trust` from a terminal in the project.

#### Scenario: Banner visible until trust granted

- **WHEN** a Codex session is created in a project where `detect()` reported `trusted_for_project: Some(false)`
- **THEN** a banner SHALL render in the session view: "Codex requires trust for this project. Run `codex trust` from a terminal in this project, then rescan agents."
- **AND** the banner SHALL include a "Rescan" button that re-runs detection

#### Scenario: Trust auto-detected after user grants it externally

- **WHEN** the user has run `codex trust` externally
- **AND** invokes "Rescan" or `nergal rescan-agents`
- **THEN** detection SHALL update `trusted_for_project: Some(true)`
- **AND** the banner SHALL disappear

### Requirement: No plan-mode synthesis attempted

The Codex adapter SHALL NOT attempt to synthesize a plan-review flow from `PreToolUse` patterns or any other workaround. The PlanPanel is hidden via capability gating, period.

#### Scenario: Codex executes a tool that would require plan review on CC

- **WHEN** Codex executes `Edit` on a file in a session
- **THEN** the nergal UI SHALL NOT show any plan-review modal or panel
- **AND** the action SHALL flow through normal hook events (PreToolUse, PostToolUse) like any other tool call

### Requirement: codex adapter advertises and implements THEME_SYNC (limited)

The codex adapter SHALL declare `AgentCapability::THEME_SYNC` in its capabilities bitset and SHALL implement `apply_theme(palette)` such that:

1. `~/.codex/config.toml` SHALL be edited atomically to upsert `tui.theme = "<derived>"`, where `<derived>` is mapped from `palette.is_dark`. Today both branches collapse to `"monochrome"` pending a future codex CLI light-theme name.
2. Other TOML keys (`mcp_servers`, etc.) SHALL be preserved with their formatting + comments via `toml_edit`.
3. The file SHALL be created if missing, with `[tui] theme = "<derived>"` as its sole content.

The spec acknowledges a **known limitation**: codex's `tui.theme` only affects syntax-highlighting colors and requires a codex restart to take effect. The TUI canvas background does NOT change. This requirement ships to keep parity in the capability surface and to be ready when codex CLI widens its theme keys.

#### Scenario: Apply theme writes tui.theme into config.toml

- **WHEN** `apply_theme(palette)` is called
- **THEN** `~/.codex/config.toml` SHALL contain `theme = "<derived>"` under the `[tui]` table
- **AND** if `[tui]` did not exist, it SHALL be created
- **AND** the write SHALL be atomic

#### Scenario: Other config keys preserved

- **WHEN** `~/.codex/config.toml` contains an `[mcp_servers.foo]` table before `apply_theme` is called
- **THEN** the `[mcp_servers.foo]` table SHALL remain intact after the write
- **AND** comments and whitespace in unrelated lines SHALL be preserved

#### Scenario: Missing config.toml is created

- **WHEN** `~/.codex/config.toml` does not exist
- **THEN** `apply_theme` SHALL create it with the minimal `[tui]` table containing `theme = "<derived>"`

#### Scenario: Live update is NOT attempted

- **WHEN** `apply_theme` runs against active codex sessions
- **THEN** no IPC SHALL be performed to running codex processes
- **AND** the theme change applies only to subsequently spawned codex sessions
- **AND** even for those, only syntax highlighting reflects the change — the TUI background remains controlled by codex's internal defaults

