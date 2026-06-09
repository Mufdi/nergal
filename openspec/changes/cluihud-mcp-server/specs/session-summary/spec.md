## ADDED Requirements

### Requirement: Opt-in AI session summaries (net-new machinery)

AI summaries SHALL be net-new inference machinery (no LLM-invocation path exists in the backend today). The capability SHALL be off by default, configurable globally with a per-project override. When disabled, no transcript SHALL be read, no model SHALL be invoked, and no summary SHALL be produced, stored, or sent.

#### Scenario: Disabled by default

- **WHEN** a session runs and AI summaries have never been enabled
- **THEN** no transcript SHALL be read, no model invoked, and the descriptor's `summary` SHALL be null

#### Scenario: Per-project override

- **WHEN** AI summaries are enabled globally but disabled for a specific project
- **THEN** sessions in that project SHALL NOT be summarized

### Requirement: Inference path with credential resolution and token accounting

When enabled, the system SHALL invoke a cheap model (e.g. haiku) via a detached runner using a **dedicated configured API key** (a `config.rs` setting) — it SHALL NOT reuse the session's own agent auth (fragile, conflates billing). It reads the session transcript, accounts the token cost, and produces a short rolling summary. There SHALL be a single summarization entrypoint that M4's post-session MOC summary can later reuse.

#### Scenario: Summary appears within a refresh cycle

- **WHEN** AI summaries are enabled and a session produces transcript activity
- **THEN** a non-empty summary SHALL appear in the session descriptor within one refresh cycle

#### Scenario: Token cost recorded

- **WHEN** a summary is generated
- **THEN** its token cost SHALL be recorded alongside the summary

#### Scenario: Missing credentials handled

- **WHEN** AI summaries are enabled but no usable credentials resolve
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
