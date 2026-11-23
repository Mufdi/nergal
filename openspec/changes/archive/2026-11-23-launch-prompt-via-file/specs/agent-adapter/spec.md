## ADDED Requirements

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

## MODIFIED Requirements

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
