## ADDED Requirements

### Requirement: Opt-in AI session summaries

The system SHALL generate short AI summaries of sessions only when explicitly enabled. Summaries SHALL be off by default, configurable globally with a per-project override. When disabled, no transcript content SHALL be read for summarization and no summary SHALL be produced, stored, or sent.

#### Scenario: Disabled by default

- **WHEN** a session runs and AI summaries have never been enabled
- **THEN** no summary SHALL be generated and the descriptor's `summary` SHALL be null

#### Scenario: Per-project override

- **WHEN** AI summaries are enabled globally but disabled for a specific project
- **THEN** sessions in that project SHALL NOT be summarized

### Requirement: Detached cheap-model summarizer

When enabled, the system SHALL produce summaries via a detached runner using a cheap model (haiku), reading the session transcript and emitting a short rolling summary (a few sentences). The summarizer entrypoint SHALL be shared with the post-session MOC summary (M4) so there is a single summarization path.

#### Scenario: Summary appears within a refresh cycle

- **WHEN** AI summaries are enabled and a session produces transcript activity
- **THEN** a non-empty summary SHALL appear in the session descriptor within one refresh cycle

#### Scenario: Summarization runs detached

- **WHEN** a summary is generated
- **THEN** it SHALL run in a detached runner and SHALL NOT block any MCP directory read

### Requirement: Refresh policy and freshness marking

The system SHALL refresh summaries on the `Stop` hook (debounced to avoid mid-turn summarization) and on demand, SHALL cap refresh frequency, and SHALL timestamp each summary. A directory read SHALL never wait on summarization; if no summary exists yet, `summary` SHALL be null.

#### Scenario: Debounced on Stop

- **WHEN** multiple `Stop` events arrive in quick succession for a session
- **THEN** the summarizer SHALL run at most once per debounce window

#### Scenario: Stale summary is timestamped, not blocking

- **WHEN** a directory read occurs while a newer summary is still being generated
- **THEN** the read SHALL return the last available summary with its timestamp, without blocking
