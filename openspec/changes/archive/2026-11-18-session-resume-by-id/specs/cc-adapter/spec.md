## MODIFIED Requirements

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
