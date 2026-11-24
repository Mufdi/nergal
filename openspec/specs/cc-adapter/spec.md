# cc-adapter Specification

## Purpose
TBD - created by archiving change agent-adapter-foundation. Update Purpose after archive.

## Requirements

### Requirement: Claude Code adapter implements the AgentAdapter contract

The system SHALL ship a `ClaudeCodeAdapter` in `src-tauri/src/agents/claude_code/mod.rs` that implements `AgentAdapter` and preserves all current nergal behavior for CC sessions. The adapter SHALL be registered as the default in `AgentRegistry`.

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

`ClaudeCodeAdapter::spawn(ctx)` SHALL return a `SpawnSpec` whose `binary` is `claude` (or the configured override from `config.claude_binary`), whose `args` resume a conversation only by explicit id (`--resume <conversation id>` when `ctx.resume_from` is a conversation id; `--resume` alone when `ctx.resume_from == "resume_pick"`; no flag for a fresh start), and whose `env` includes `NERGAL_SESSION_ID = ctx.session_id`. The adapter SHALL NOT emit `--continue`: it picks the newest conversation of the cwd, which belongs to another session when two sessions share a cwd.

#### Scenario: Fresh session spawn

- **WHEN** `spawn(ctx)` is called with `ctx.resume_from == None`
- **THEN** `SpawnSpec { binary: "claude", args: [], env: { NERGAL_SESSION_ID: ... } }` SHALL be returned

#### Scenario: Continue an existing session

- **WHEN** `spawn(ctx)` is called with `ctx.resume_from == Some("aaaa-1111")`
- **THEN** `SpawnSpec.args` SHALL start with `["--resume", "aaaa-1111"]`

#### Scenario: Let Claude Code prompt for the conversation

- **WHEN** `spawn(ctx)` is called with `ctx.resume_from == Some("resume_pick")`
- **THEN** `SpawnSpec.args` SHALL start with `["--resume"]` followed by no id

#### Scenario: The continue sentinel never reaches the CLI

- **WHEN** `spawn(ctx)` is called with `ctx.resume_from == Some("continue")`
- **THEN** `SpawnSpec.args` SHALL NOT contain `--continue`
- **AND** `SpawnSpec.args` SHALL NOT contain `--resume`

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

The CC adapter SHALL use the per-user Unix socket `hook.sock` in the IPC directory to receive hook events from the `nergal hook` CLI subcommands. For a blocking plan review, the hook SHALL create a FIFO named `plan-<pid>-<nonce>.fifo` in the per-user IPC directory, where `<nonce>` is a random UUID, so a reused pid never yields the path of an earlier review.

#### Scenario: Plan review FIFO write on user decision

- **WHEN** the user accepts or denies a plan in the UI for a CC session
- **AND** the frontend invokes `submit_plan_decision`
- **THEN** the CC adapter SHALL write the decision JSON to the FIFO referenced by `decision_path`
- **AND** the blocked `nergal hook plan-review` subprocess SHALL receive the decision and pass it back to CC via the hook protocol

#### Scenario: Ask-user FIFO write on user answer

- **WHEN** the user submits an AskUserQuestion answer for a CC session
- **THEN** the CC adapter SHALL write `{"answers": <map>}` to the FIFO referenced by `decision_path`

#### Scenario: Two reviews with the same pid get different FIFOs

- **WHEN** the plan-review FIFO path is built twice for pid 42
- **THEN** both paths SHALL sit in the per-user IPC directory, start with `plan-42-`, and differ

### Requirement: Setup flow for CC writes ~/.claude/settings.json hooks

`setup_agent(AgentId::claude_code())` SHALL write the Nergal hook configuration into `~/.claude/settings.json`. Hook matchers, async/sync flags, and timeouts SHALL follow the `HOOKS` table in `src-tauri/src/setup.rs`. An existing Nergal entry whose matcher differs from the table SHALL get its matcher upgraded in place, keeping its command form (bare, wrapper, or absolute exe).

#### Scenario: Setup writes all current hook entries

- **WHEN** `setup_agent('claude-code')` runs against a fresh `~/.claude/settings.json`
- **THEN** the resulting file SHALL include hook entries for SessionStart, SessionEnd, PermissionRequest[ExitPlanMode], PreToolUse + PreToolUse[AskUserQuestion], PostToolUse[Write|Edit|MultiEdit|Bash|TaskCreate|TaskUpdate|TodoWrite|NotebookEdit|Create|AskUserQuestion], Notification[permission_prompt|agent_needs_input|agent_completed], SubagentStart[.+], SubagentStop[.+], Stop, UserPromptSubmit
- **AND** the obsolete-hooks cleanup logic SHALL remove any pre-existing entries from older nergal versions

#### Scenario: Subagent hooks are async senders

- **WHEN** setup inserts the SubagentStart and SubagentStop entries
- **THEN** their commands SHALL be `nergal hook send subagent-start` and `nergal hook send subagent-stop`
- **AND** both SHALL be `async: true` with matcher `.+`, so CC internal agents with an empty `agent_type` never spawn the hook

#### Scenario: Existing Notification entry gets the widened matcher

- **WHEN** settings already hold a Notification entry `nergal hook notification` with matcher `permission_prompt`
- **THEN** setup SHALL rewrite only its matcher to `permission_prompt|agent_needs_input|agent_completed`
- **AND** it SHALL NOT add a second Notification entry

### Requirement: Plan watcher respects user's plansDirectory configuration

CC's plans-directory resolution SHALL be centralized in a single resolver consumed by both the plan watcher (`agents/claude_code/plan.rs`) and the `list_session_plans` Tauri command (via `plan_capability()`). The CC adapter's `plan_capability(session, cwd)` SHALL return `FileBased { dir: <resolved>, label: "Claude Code" }` where `<resolved>` is computed as:

1. Read `~/.claude/settings.json`, `<cwd>/.claude/settings.json`, and `<cwd>/.claude/settings.local.json` in CC's precedence order (project-local files override home).
2. Extract the merged `plansDirectory` string if any layer sets it.
3. Expand a leading `~/` against `$HOME`.
4. If the resulting path is absolute → use as-is.
5. If the resulting path is relative → join against `cwd`.
6. If no `plansDirectory` is set in any layer → fallback to `<cwd>/.claude/plans/`.

The plan watcher startup in `lib.rs` SHALL call this resolver instead of consuming `config.plans_directory`. The two code paths that previously diverged (watcher used nergal config; `list_session_plans` hardcoded `<cwd>/.claude/plans/`) MUST be unified through this resolver. This path convention SHALL remain CC-specific; other adapters' `plan_capability()` implementations MUST NOT use this resolver.

#### Scenario: Plan loaded from worktree-local plans dir (default case)
- **WHEN** a CC session in `/path/to/worktree` has plans at `/path/to/worktree/.claude/plans/<plan>.md`
- **AND** no `plansDirectory` is set in any settings layer
- **THEN** `plan_capability()` returns `FileBased { dir: "/path/to/worktree/.claude/plans", label: "Claude Code" }`
- **AND** both the watcher and `list_session_plans` SHALL use that path

#### Scenario: Absolute plansDirectory respected
- **WHEN** `~/.claude/settings.json` contains `"plansDirectory": "/custom/path/plans"`
- **THEN** `plan_capability()` returns `FileBased { dir: "/custom/path/plans", label: "Claude Code" }`

#### Scenario: Relative plansDirectory resolves against cwd
- **WHEN** `~/.claude/settings.json` contains `"plansDirectory": ".claude/plans"` AND the session `cwd` is `/path/to/repo`
- **THEN** `plan_capability()` returns `FileBased { dir: "/path/to/repo/.claude/plans", label: "Claude Code" }`

#### Scenario: Tilde expansion
- **WHEN** `~/.claude/settings.json` contains `"plansDirectory": "~/notes/plans"`
- **THEN** `plan_capability()` returns `FileBased { dir: "<HOME>/notes/plans", label: "Claude Code" }`

#### Scenario: Project-local settings override home settings
- **WHEN** `~/.claude/settings.json` sets `"plansDirectory": "/global"`
- **AND** `<cwd>/.claude/settings.json` sets `"plansDirectory": "/project"`
- **THEN** `plan_capability()` returns `FileBased { dir: "/project", label: "Claude Code" }`

#### Scenario: settings.local.json overrides settings.json in cwd
- **WHEN** `<cwd>/.claude/settings.json` sets `"plansDirectory": "/team"`
- **AND** `<cwd>/.claude/settings.local.json` sets `"plansDirectory": "/personal"`
- **THEN** `plan_capability()` returns `FileBased { dir: "/personal", label: "Claude Code" }`

#### Scenario: settings.json missing or no plansDirectory
- **WHEN** no settings layer contains `plansDirectory`
- **THEN** `plan_capability()` returns `FileBased { dir: "<cwd>/.claude/plans", label: "Claude Code" }`

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
- Session resume (`claude --resume <conversation id>` for the session's own conversation, `claude --resume` for the picker; never `claude --continue`)
- File-changed events updating ModifiedFiles panel

#### Scenario: All flows pass integration tests

- **WHEN** the integration test suite runs against the post-foundation codebase with a CC adapter
- **THEN** every flow listed above SHALL pass without modification of test expectations from the pre-foundation baseline

### Requirement: A plan review opens only with a decision path
A plan review SHALL open in the UI only from a `plan:ready` event that carries a non-empty `decision_path`. The plan panel SHALL show Approve and Revise only on the document of the live review: the review state is `pending_review` and the document's decision path is non-empty and equal to the session's. A `plan:ready` event without one whose content equals the content the session last received (`original`) SHALL keep the stored decision path, session id, review state, and any edit the user made. A `plan:ready` event without one and with different content SHALL replace the plan, clear the stored decision path and set the review state to `idle`.

#### Scenario: A preview that arrives after its review keeps the decision path
- **GIVEN** session S received `plan:ready` with `path: "/p/a.md"`, `content: "# A"` and `decision_path: "/run/user/1000/nergal/plan-42-u.fifo"`, and its review state is `pending_review`
- **AND** the user edited the plan content to `# A edited`
- **WHEN** `plan:ready` arrives for S with `content: "# A"` and no `decision_path`, with `path: "/p/a.md"` or with `path: "/p/b.md"`
- **THEN** the session plan and the document for the preview's path SHALL hold decision path `/run/user/1000/nergal/plan-42-u.fifo` and content `# A edited`
- **AND** the review state SHALL stay `pending_review`

#### Scenario: A preview with new content starts a new cycle
- **GIVEN** session S last received plan content `# A`, holds decision path `/run/user/1000/nergal/plan-42-u.fifo` and review state `pending_review`
- **WHEN** `plan:ready` arrives for S with `content: "# B"` and no `decision_path`
- **THEN** the plan content SHALL be `# B`, the stored decision path SHALL be empty, and the review state SHALL be `idle`

#### Scenario: An old plan's tab shows no review controls
- **GIVEN** session S has review state `pending_review` with decision path `/run/user/1000/nergal/plan-42-u.fifo` on `/p/a.md`
- **WHEN** the user opens the tab of an earlier plan `/p/old.md` whose stored decision path is empty or different
- **THEN** that tab SHALL show no Approve and no Revise

#### Scenario: A preview alone opens no review
- **GIVEN** session S has review state `idle` and no stored plan
- **WHEN** `plan:ready` arrives for S without `decision_path`
- **THEN** the plan content SHALL show and the review state SHALL stay `idle`

### Requirement: A plan decision reaches the waiting hook or fails visibly
On Unix, `submit_plan_decision` SHALL claim the session's registration before writing: it SHALL proceed only when the decision path equals the path registered by the latest `PlanReview` event, and the claim SHALL remove that registration, so at most one decision is written per review. It SHALL write only to an existing FIFO that has a reader, and SHALL never create a file, follow a symlink, or take more than 5 seconds. A FIFO with no reader SHALL fail within 1 second, whatever the write deadline. A `PlanReview` event SHALL register its FIFO only when the FIFO's parent directory is the per-user IPC directory. When the UI cannot deliver a decision, it SHALL show an error toast titled with the action (`Approve failed` or `Revise failed`) whose description tells the user to answer in the terminal, and the review state SHALL become `undelivered`. That state hides Approve and Revise, keeps the annotations visible and clearable, and shows `Not delivered. Answer in the terminal.` in the panel header. It lasts until a `plan:ready` opens a review or brings new content, or the session stops. While a decision for a session is being sent, its Approve and Revise SHALL be disabled. A failed Revise SHALL keep the annotations.

#### Scenario: An unregistered path is rejected before any open
- **GIVEN** session S has registered `/run/user/1000/nergal/plan-42-u.fifo`
- **WHEN** `submit_plan_decision` targets `/tmp/other.fifo` for S
- **THEN** it SHALL return an error, open no file, and keep the registration

#### Scenario: Two submits for one review write one decision
- **GIVEN** session S has registered a live FIFO
- **WHEN** two `submit_plan_decision` calls for S and that path run at the same time
- **THEN** exactly one SHALL write its decision
- **AND** the other SHALL return an error and write nothing

#### Scenario: A missing FIFO is an error and creates nothing
- **WHEN** the decision writer targets a path that does not exist
- **THEN** it SHALL return an error
- **AND** no file SHALL exist at that path afterwards

#### Scenario: A regular file or a symlink is not a FIFO
- **GIVEN** a regular file with content `x`, or a symlink to a FIFO, at the decision path
- **WHEN** the decision writer targets it
- **THEN** it SHALL return an error and the file content SHALL stay `x`

#### Scenario: A FIFO with no reader is an error
- **GIVEN** a FIFO at the decision path that no process has open for reading
- **WHEN** the decision writer targets it
- **THEN** it SHALL return an error within 1 second, with a 1-second or a 5-second deadline

#### Scenario: A reader that never reads hits the deadline
- **GIVEN** a FIFO with a reader open that never reads
- **WHEN** the decision writer writes 100000 bytes with a 200 ms deadline
- **THEN** it SHALL return an error within 1 second

#### Scenario: A live FIFO receives the whole decision
- **GIVEN** a FIFO at the decision path with a reader already open
- **WHEN** the decision writer writes a JSON value 100000 bytes long
- **THEN** the reader SHALL receive all 100000 bytes and parse them as one JSON value

#### Scenario: A review without a decision path shows no controls
- **GIVEN** session S has review state `pending_review` and the open document's stored decision path is empty
- **WHEN** the plan panel renders that document
- **THEN** it SHALL show no Approve and no Revise

#### Scenario: The FIFO must sit in the IPC directory
- **WHEN** a `PlanReview` event names `/tmp/plan-42-u.fifo` while the IPC directory is `/run/user/1000/nergal`
- **THEN** the FIFO SHALL not be registered and `plan:ready` SHALL carry an empty `decision_path`

#### Scenario: A held key sends one decision
- **GIVEN** the plan panel shows a review with focus in the panel
- **WHEN** the user holds `A` so the key repeats
- **THEN** the panel SHALL call `submit_plan_decision` once
- **AND** a repeat of `A`, `R` or `Ctrl+Shift+R` that arrives after the review left `pending_review` SHALL send nothing

#### Scenario: A failed Revise keeps the annotations
- **GIVEN** a review with two annotations whose delivery fails
- **WHEN** the user presses `R`
- **THEN** a `Revise failed` toast SHALL show, the review state SHALL become `undelivered`, and both annotations SHALL stay highlighted in the plan
- **AND** pressing `X` and confirming SHALL clear them

### Requirement: An approval carries the tool input
The `nergal hook plan-review` subprocess SHALL answer an approval with a `PermissionRequest` decision whose `behavior` is `allow` and whose `updatedInput` equals the `tool_input` object it read from stdin, because Claude Code ignores an `allow` without `updatedInput` for `ExitPlanMode`. A denial SHALL keep `behavior: "deny"` with the message. Without `NERGAL_SESSION_ID` the subprocess SHALL print nothing and exit 0, so Claude Code shows its own dialog. A released review SHALL print nothing and exit 0.

#### Scenario: Approve echoes the plan input
- **GIVEN** the hook read `{"tool_name":"ExitPlanMode","tool_input":{"plan":"# A","planFilePath":"/p/a.md"}}` from stdin
- **WHEN** it receives `{"approved": true}`
- **THEN** stdout SHALL be one JSON object with `hookSpecificOutput.hookEventName` `PermissionRequest`, `decision.behavior` `allow`, and `decision.updatedInput` equal to `{"plan":"# A","planFilePath":"/p/a.md"}`

#### Scenario: Revise keeps the deny message
- **WHEN** the hook receives `{"approved": false, "message": "fix step 2"}`
- **THEN** stdout SHALL hold `decision.behavior` `deny` and `decision.message` `fix step 2`, and no `updatedInput`

#### Scenario: No Nergal session prints no decision
- **GIVEN** `NERGAL_SESSION_ID` is unset or empty
- **WHEN** the hook runs
- **THEN** stdout SHALL be empty and the exit code SHALL be 0

#### Scenario: A released review prints no decision
- **WHEN** the hook receives `{"released": true}`
- **THEN** stdout SHALL be empty and the exit code SHALL be 0

### Requirement: A review resolved outside the panel releases its hook
Nergal SHALL register its `PostToolUse` hook for `ExitPlanMode` in addition to the tools it already matches. It SHALL release a session's plan review on a `PostToolUse` for `ExitPlanMode` from the main agent, on a `PlanReview` for a session that still has a registered FIFO (before registering the new one), on `Stop`, and on `SessionEnd`. Releasing SHALL remove the registration and, when one existed, write `{"released": true}` to its FIFO with a deadline of at most 1 second, off the hook server's event loop; a failed write SHALL only be logged. On a `PostToolUse` for `ExitPlanMode` from the main agent, Nergal SHALL emit `plan:review-closed` with the session id. A deny or Esc answered in Claude Code's own dialog emits no event Nergal registers, so such a review stays registered until one of these events.

#### Scenario: A terminal approval releases the hook and closes the review
- **GIVEN** session S has registered FIFO F with a reader
- **WHEN** a `PostToolUse` event for `ExitPlanMode` without `agent_id` arrives for S
- **THEN** the registration for S SHALL be gone, the reader of F SHALL receive `{"released": true}`, and `plan:review-closed` SHALL be emitted for S

#### Scenario: A new review releases the old hook
- **GIVEN** session S has registered FIFO F1 with a reader
- **WHEN** a `PlanReview` event for S arrives with FIFO F2 in the IPC directory
- **THEN** the reader of F1 SHALL receive `{"released": true}` and S SHALL have F2 registered

#### Scenario: Stop releases a waiting hook
- **GIVEN** session S has registered FIFO F with a reader
- **WHEN** a `Stop` event for S arrives
- **THEN** the reader of F SHALL receive `{"released": true}` and S SHALL have no registration

#### Scenario: A subagent's PostToolUse releases nothing
- **GIVEN** session S has registered FIFO F
- **WHEN** a `PostToolUse` event for `ExitPlanMode` with an `agent_id` arrives for S
- **THEN** F SHALL stay registered and no `plan:review-closed` SHALL be emitted

#### Scenario: Setup adds ExitPlanMode to the PostToolUse matcher
- **GIVEN** settings hold the Nergal `PostToolUse` entry with matcher `Write|Edit|MultiEdit|Bash|TaskCreate|TaskUpdate|TodoWrite|NotebookEdit|Create|AskUserQuestion`
- **WHEN** setup runs
- **THEN** that entry's matcher SHALL also include `ExitPlanMode` and no second Nergal `PostToolUse` entry SHALL exist

### Requirement: The panel shows a review answered in the terminal
On `plan:review-closed`, the panel SHALL move a review in `pending_review` or `undelivered` to state `answered`, and a review in `submitted` to `answered` with an error toast titled `Revise not delivered`. `answered` hides Approve and Revise, keeps the annotations visible and clearable, and shows `Answered in the terminal.` in the header, plus `Annotations were not sent.` when annotations exist. It SHALL last until a `plan:ready` opens a review or brings new content, or the session ends; `Stop` SHALL keep it.

#### Scenario: The panel shows the terminal answer and keeps annotations
- **GIVEN** session S has review state `pending_review` and two annotations
- **WHEN** `plan:review-closed` arrives for S
- **THEN** the review state SHALL be `answered`, both annotations SHALL stay, Approve and Revise SHALL be hidden, and the header SHALL read `Answered in the terminal. Annotations were not sent.`

#### Scenario: A decision after a terminal answer fails visibly
- **GIVEN** session S was released by a `PostToolUse` for `ExitPlanMode`
- **WHEN** the panel calls `submit_plan_decision` for S with the old decision path
- **THEN** it SHALL return an error and the panel SHALL show `Approve failed` or `Revise failed`

#### Scenario: A revise that lost the race reports it
- **GIVEN** session S has review state `submitted` after a Revise
- **WHEN** `plan:review-closed` arrives for S
- **THEN** a `Revise not delivered` error toast SHALL show and the review state SHALL be `answered`

#### Scenario: Stop keeps the answered state
- **GIVEN** session S has review state `answered`
- **WHEN** a `Stop` event for S arrives
- **THEN** the review state SHALL stay `answered`
