## ADDED Requirements

### Requirement: Opt-in AI session summaries (net-new machinery)

AI summaries SHALL be net-new inference machinery (no LLM-invocation path exists in the backend today). The capability SHALL be off by default, configurable globally with a per-project override. When disabled, no transcript SHALL be read, no model SHALL be invoked, and no summary SHALL be produced, stored, or sent.

#### Scenario: Disabled by default

- **WHEN** a session runs and AI summaries have never been enabled
- **THEN** no transcript SHALL be read, no model invoked, and the descriptor's `summary` SHALL be null

#### Scenario: Per-project override

- **WHEN** AI summaries are enabled globally but disabled for a specific project
- **THEN** sessions in that project SHALL NOT be summarized

### Requirement: User-selected inference backend (off by default, two mutually-exclusive opt-in modes)

The summarizer backend SHALL be one of three user-chosen states, surfaced in settings as two **mutually-exclusive** switches (enabling one disables the other); the default is neither enabled:

- **Off (default).** Neither switch enabled → no transcript read, no model invoked, no summary. This is a fully supported steady state, not a degraded one.
- **Agent CLI (subscription, key-free).** Invoke an agent CLI in headless mode (e.g. `claude -p <prompt>`) using the user's existing **subscription auth** — NO API key required (verified: a headless `claude -p` call authenticates via the Claude Max plan with no key). The runner SHALL resolve the binary + verified headless flags from the **agent marked default in Settings** (`config.default_agent`, falling back to Claude Code), via a per-adapter `headless_print_command()` so only agents with a *verified* non-interactive print mode are used; an optional per-summary command overrides it. An agent without a verified headless mode produces no summary (logged), prompting the user to switch the default agent or use API-key mode. Documented tradeoff: summarization consumes the user's subscription quota/rate-limits.
- **API key (provider-agnostic).** Call a **provider-agnostic** HTTP endpoint (base URL + model + key) — NOT Anthropic-locked (OpenAI-compatible / any provider / local). The key SHALL be stored in the **OS keyring**, never in `config.json` plaintext.

The two modes SHALL NOT be active simultaneously. The runner SHALL NOT silently reuse a session's own per-session agent process (that would conflate the user's active turn). There SHALL be a single summarization entrypoint that M4's post-session MOC summary can later reuse.

#### Scenario: Both switches off is a no-op

- **WHEN** neither summarizer switch is enabled
- **THEN** no transcript SHALL be read, no model invoked, the descriptor's `summary` SHALL be null, and this SHALL NOT be treated as an error

#### Scenario: Switches are mutually exclusive

- **WHEN** the user enables one summarizer mode while the other is already enabled
- **THEN** the previously-enabled mode SHALL be disabled so only one backend is ever active

#### Scenario: Agent CLI mode works on subscription with no API key

- **WHEN** the Agent CLI mode is enabled and no API key is configured
- **THEN** the runner SHALL summarize via the agent CLI headless path using the subscription, producing a non-empty summary without any key

#### Scenario: API key mode is keyring-stored

- **WHEN** the user enables the API key mode and configures a key
- **THEN** the key SHALL be persisted in the OS keyring (not `config.json`), and the runner SHALL use the configured endpoint/model

#### Scenario: Summary appears within a refresh cycle

- **WHEN** a backend is enabled and a session produces transcript activity
- **THEN** a non-empty summary SHALL appear in the session descriptor within one refresh cycle

#### Scenario: Token cost recorded when available

- **WHEN** a summary is generated and the backend reports usage
- **THEN** its token cost SHALL be recorded alongside the summary; when the backend does not report usage (e.g. headless CLI), `token_cost` MAY be null

#### Scenario: Backend unavailable handled

- **WHEN** a backend is enabled but it cannot run (agent CLI missing/unauthenticated, or key/endpoint unreachable)
- **THEN** the runner SHALL fail gracefully (no summary, logged), without blocking directory reads

### Requirement: SQLite-backed summary storage via migration

Summaries SHALL be persisted in SQLite via a migration under `src-tauri/migrations/` (registered in `db.rs`), in a table `session_summaries(session_id TEXT PRIMARY KEY, summary TEXT NOT NULL, model TEXT, token_cost INTEGER, updated_at INTEGER NOT NULL)`. A summary row SHALL exist only when summaries are enabled for that session's project.

#### Scenario: Summary persisted across restart

- **WHEN** a summary is generated and the app restarts
- **THEN** the persisted summary SHALL still be available in the directory

#### Scenario: No row when disabled

- **WHEN** AI summaries are disabled for a project
- **THEN** no `session_summaries` row SHALL be written for its sessions

### Requirement: Refresh policy and non-blocking reads

The system SHALL refresh summaries on the `Stop` hook (debounced to avoid mid-turn summarization) and on demand, SHALL cap refresh frequency, and SHALL timestamp each summary. A directory read SHALL never wait on summarization; if no summary exists yet, `summary` SHALL be null.

#### Scenario: Debounced on Stop

- **WHEN** multiple `Stop` events arrive in quick succession for a session
- **THEN** the summarizer SHALL run at most once per debounce window

#### Scenario: Read never blocks on summarization

- **WHEN** a directory read occurs while a newer summary is still being generated
- **THEN** the read SHALL return the last persisted summary (or null) with its timestamp, without blocking
