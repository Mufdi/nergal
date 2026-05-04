# cc-adapter Specification

## Purpose
TBD - created by archiving change agent-adapter-foundation. Update Purpose after archive.
## Requirements
### Requirement: Claude Code adapter implements the AgentAdapter contract

The system SHALL ship a `ClaudeCodeAdapter` in `src-tauri/src/agents/claude_code/mod.rs` that implements `AgentAdapter` and preserves all current cluihud behavior for CC sessions. The adapter SHALL be registered as the default in `AgentRegistry`.

#### Scenario: Registry returns CC adapter by id

- **WHEN** `AgentRegistry::get(AgentId::claude_code())` is called
- **THEN** an `Arc<dyn AgentAdapter>` for the CC implementation SHALL be returned

#### Scenario: CC adapter declares full capability set

- **WHEN** `ClaudeCodeAdapter::capabilities()` is read
- **THEN** the returned flags SHALL include `PLAN_REVIEW`, `ASK_USER_BLOCKING`, `TOOL_CALL_EVENTS`, `STRUCTURED_TRANSCRIPT`, `RAW_COST_PER_MESSAGE`, `TASK_LIST`, `SESSION_RESUME`, `ANNOTATIONS_INJECT`

### Requirement: CC adapter detects via filesystem

`ClaudeCodeAdapter::detect()` SHALL return `installed: true` when at least one of the following holds:

- `~/.claude/` directory exists
- `claude` binary is on `PATH` (resolvable via `which`)

The returned `binary_path` SHALL come from `which claude` if available; the `config_path` SHALL be `~/.claude/`.

#### Scenario: CC detected with binary and config

- **WHEN** the user has Claude Code installed (binary on PATH and `~/.claude/` exists)
- **THEN** `detect()` returns `DetectionResult { installed: true, binary_path: Some(...), config_path: Some("~/.claude"), version: <claude --version output if cheap to call> }`

#### Scenario: CC absent

- **WHEN** neither `~/.claude/` nor `claude` on PATH exists
- **THEN** `detect()` returns `DetectionResult { installed: false, binary_path: None, config_path: None, version: None }`
- **AND** the agent picker SHALL show CC greyed out with an install hint

### Requirement: CC adapter spawns claude binary with current behavior

`ClaudeCodeAdapter::spawn(ctx)` SHALL return a `SpawnSpec` whose `binary` is `claude` (or the configured override from `config.claude_binary`), whose `args` follow the existing pty.rs convention (`--continue` when `ctx.resume_from` is Some and the session was previously active; `--resume` for explicit resume; no flag for fresh start), and whose `env` includes `CLUIHUD_SESSION_ID = ctx.session_id`.

#### Scenario: Fresh session spawn

- **WHEN** `spawn(ctx)` is called with `ctx.resume_from == None`
- **THEN** `SpawnSpec { binary: "claude", args: [], env: { CLUIHUD_SESSION_ID: ... } }` SHALL be returned

#### Scenario: Continue an existing session

- **WHEN** `spawn(ctx)` is called with `ctx.resume_from == Some(prior_session_id)` and continuation is desired
- **THEN** `SpawnSpec.args` SHALL be `["--continue"]` (or `["--resume", prior_session_id]` if explicit resume)

### Requirement: CC adapter parses CC's JSONL transcript schema

`ClaudeCodeAdapter::parse_transcript_line(line)` SHALL parse Claude Code's `.jsonl` transcript format and emit `TranscriptEvent` variants. The fields read SHALL include the current `entry.message.role`, `entry.message.content[]`, and `entry.message.usage.{input_tokens, output_tokens, cache_read_input_tokens, cache_creation_input_tokens}`.

#### Scenario: Tool use event extraction

- **WHEN** a transcript line contains `{"message": {"content": [{"type": "tool_use", "name": "Edit", "input": {...}}]}}`
- **THEN** `parse_transcript_line` SHALL emit `TranscriptEvent::ToolUse { name: "Edit", input: ... }`

#### Scenario: Cost line extraction without USD

- **WHEN** a transcript line contains `{"message": {"usage": {"input_tokens": 1234, "output_tokens": 567, "cache_read_input_tokens": 0, "cache_creation_input_tokens": 89}}}`
- **THEN** `parse_transcript_line` SHALL emit `TranscriptEvent::Cost(RawCost { input_tokens: 1234, output_tokens: 567, cache_read_tokens: 0, cache_write_tokens: 89, model_id: <from message.model if present, else None> })`
- **AND** no USD value SHALL be computed at this layer

### Requirement: CC adapter uses Unix socket + FIFO transports for events and decisions

The CC adapter SHALL use the existing Unix socket at `/tmp/cluihud.sock` to receive hook events from the `cluihud hook send` CLI subcommands. For blocking decisions (plan review, ask-user), it SHALL continue to use FIFOs at `/tmp/cluihud-plan-<pid>.fifo` and `/tmp/cluihud-ask-<pid>.fifo`.

#### Scenario: Plan review FIFO write on user decision

- **WHEN** the user accepts or denies a plan in the UI for a CC session
- **AND** the frontend invokes `submit_plan_decision`
- **THEN** the CC adapter SHALL write the decision JSON to the FIFO referenced by `decision_path`
- **AND** the blocked `cluihud hook plan-review` subprocess SHALL receive the decision and pass it back to CC via the hook protocol

#### Scenario: Ask-user FIFO write on user answer

- **WHEN** the user submits an AskUserQuestion answer for a CC session
- **THEN** the CC adapter SHALL write `{"answers": <map>}` to the FIFO referenced by `decision_path`

### Requirement: Setup flow for CC writes ~/.claude/settings.json hooks

`setup_agent(AgentId::claude_code())` SHALL produce the same `~/.claude/settings.json` hook configuration as the pre-foundation `cluihud setup` command. Hook matchers, async/sync flags, and timeouts SHALL match the current behavior exactly.

#### Scenario: Setup writes all current hook entries

- **WHEN** `setup_agent('claude-code')` runs against a fresh `~/.claude/settings.json`
- **THEN** the resulting file SHALL include hook entries for SessionStart, SessionEnd, PermissionRequest[ExitPlanMode], PreToolUse + PreToolUse[AskUserQuestion], PostToolUse[Write|Edit|MultiEdit|Bash|TaskCreate|TaskUpdate|TodoWrite|NotebookEdit|Create], TaskCreated, TaskCompleted, CwdChanged, FileChanged, PermissionDenied, Stop, UserPromptSubmit
- **AND** the obsolete-hooks cleanup logic SHALL remove any pre-existing entries from older cluihud versions

### Requirement: Plan watcher respects user's plansDirectory configuration

`agents/claude_code/plan.rs` SHALL watch `<cwd>/.claude/plans/` first; the global fallback path SHALL be derived by reading `~/.claude/settings.json` `plansDirectory` field if set, otherwise `~/.claude/plans/`. This path convention SHALL be CC-specific; other adapters SHALL NOT inherit it.

#### Scenario: Plan loaded from worktree-local plans dir

- **WHEN** a CC session in `/path/to/worktree` has a plan at `/path/to/worktree/.claude/plans/<plan>.md`
- **THEN** `agents/claude_code/plan.rs` SHALL load it preferentially over any global path

#### Scenario: User-customized plansDirectory respected

- **WHEN** the user has configured `~/.claude/settings.json` with `plansDirectory: "/custom/path/plans"`
- **AND** no worktree-local `.claude/plans/` exists
- **THEN** the plan watcher SHALL read from `/custom/path/plans` (the user-configured global), not from the default `~/.claude/plans/`

#### Scenario: settings.json missing or no plansDirectory

- **WHEN** `~/.claude/settings.json` does not exist or does not contain `plansDirectory`
- **THEN** the plan watcher SHALL fall back to `~/.claude/plans/` as the global default

### Requirement: Task parser remains CC tool-name-aware

The task parser at `agents/claude_code/tasks.rs` (moved from `tasks/transcript_parser.rs`) SHALL recognize tool names `TodoWrite`, `TodoUpdate`, `Task`, `TaskCreate`, `TaskUpdate` from CC's tool palette.

#### Scenario: Task event from TodoWrite tool

- **WHEN** a CC transcript line contains a tool_use with `name: "TodoWrite"` and `input.command: "create"`
- **THEN** the parser SHALL emit a task-created event consumable by the frontend `TaskStore`

### Requirement: Cost extractor moves Sonnet 4 pricing into a private CC adapter helper

`agents/claude_code/cost.rs` SHALL expose `parse_cost_line(line) -> Option<RawCost>` for the trait, NOT a public USD calculator. The previous Sonnet 4 USD constants ($3/MTok input, $15/MTok output, $0.30/MTok cache_read, $3.75/MTok cache_write) SHALL move into a **private** `legacy_usd_for_sonnet4(raw: &RawCost) -> f64` function inside `agents/claude_code/cost.rs`. This keeps the trait clean while preserving CC's USD display in the status bar (no UX regression).

#### Scenario: parse_cost_line emits RawCost without USD

- **WHEN** `parse_cost_line(line)` is called on a usage-bearing transcript line
- **THEN** it SHALL return `Some(RawCost { ... })` with token counts only
- **AND** SHALL NOT compute USD

#### Scenario: legacy_usd_for_sonnet4 used internally by CC adapter

- **WHEN** the Tauri command `get_session_cost_usd(session_id)` is invoked for a CC session
- **THEN** the CC adapter SHALL call `legacy_usd_for_sonnet4(&aggregator.snapshot())` internally
- **AND** return `Some(usd)` from the command

#### Scenario: Status bar preserves USD for CC

- **WHEN** a CC session has accumulated cost
- **THEN** the status bar SHALL render input + output + cache_read + cache_write token totals AND the USD value (computed via `legacy_usd_for_sonnet4`)
- **AND** the user-facing display SHALL be identical to pre-foundation

### Requirement: Zero regression for CC users post-refactor

The end-to-end flows that work pre-foundation SHALL continue to work post-foundation, validated by integration tests:

- Plan review (ExitPlanMode → PlanPanel → submit decision → FIFO → CC continues)
- AskUserQuestion (PreToolUse → AskUserModal → submit answers → FIFO → CC continues)
- Task list (TodoWrite/TaskCreate/TaskUpdate → TaskPanel updates)
- Cost token counts in status bar (transcript → RawCost → token display)
- Annotation injection via UserPromptSubmit hook
- Session resume (`claude --continue` / `claude --resume`)
- File-changed events updating ModifiedFiles panel

#### Scenario: All flows pass integration tests

- **WHEN** the integration test suite runs against the post-foundation codebase with a CC adapter
- **THEN** every flow listed above SHALL pass without modification of test expectations from the pre-foundation baseline

