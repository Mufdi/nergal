## MODIFIED Requirements

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
