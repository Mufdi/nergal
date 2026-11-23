## MODIFIED Requirements

### Requirement: Agent-agnostic context-injection contract
The system SHALL inject assembled vault-note context into an agent session at spawn through the agent-adapter contract, never by bypassing it. `SpawnContext` SHALL carry an optional `injected_context: Option<&str>` (the assembled pinned-note block). The `AgentAdapter` trait SHALL declare a `context_injection() -> ContextInjection` capability whose variant decides how `injected_context` is folded into the launch command:

- `AppendSystemPromptFile` — write the block to an ephemeral per-session file and pass it via a system-prompt-file flag (Claude Code: `--append-system-prompt-file`).
- `PromptPreamble` — prepend the block, fenced and labeled, to the agent's launch prompt (delivered as the adapter's `SpawnSpec.prompt`: Codex after `--`, OpenCode as `--prompt=<value>`).
- `Unsupported` — the agent exposes no clean per-invocation channel; the block is dropped and the UI says so (no silent loss, no PTY-typed fallback).

The default trait impl SHALL be `Unsupported` so adapters that do not opt in are unaffected. When `injected_context` is `None`, `spawn()` SHALL behave exactly as before for every adapter.

#### Scenario: Claude Code injects via system-prompt file

- **WHEN** a Claude Code session spawns with a non-empty `injected_context`
- **THEN** the adapter SHALL write the block to a per-session file under `~/.config/nergal/spawn-context/`
- **AND** push `--append-system-prompt-file <path>` into the launch args
- **AND** the user's `initial_prompt`, if any, SHALL remain separate, carried in `SpawnSpec.prompt` and delivered after `--`

#### Scenario: Prompt-preamble agent folds context into its launch prompt

- **WHEN** a Codex or OpenCode session spawns with a non-empty `injected_context`
- **THEN** the adapter SHALL prepend the fenced, labeled context block to its launch prompt
- **AND** SHALL combine it with the user's `initial_prompt` when both are present

#### Scenario: Unsupported agent records the pin but skips injection

- **WHEN** a session whose adapter returns `Unsupported` has pinned notes
- **THEN** spawning SHALL NOT inject the context
- **AND** the session-tab chip SHALL indicate injection is unsupported for that agent
- **AND** no context SHALL be typed into the PTY as a fallback

#### Scenario: No pinned notes leaves spawn unchanged

- **WHEN** a session with no pinned notes spawns
- **THEN** `injected_context` SHALL be `None`
- **AND** every adapter's launch command SHALL be byte-identical to its pre-feature behavior
