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
- **Agent CLI (subscription, key-free).** Invoke an agent CLI in headless mode using the user's existing **subscription auth** — NO API key required. Verified key-free for all four adapters: `claude -p` / `pi -p` (clean stdout), `codex exec --output-last-message <file>` (banner on stdout, answer read from the file), `opencode run --format json` (JSONL, text parts concatenated + token totals summed). The runner SHALL resolve the binary + verified headless flags + output-extraction strategy from the **agent marked default in Settings** (`config.default_agent`, falling back to Claude Code), via a per-adapter `headless_print_command()`; an optional per-summary command overrides it. An agent without a verified headless mode produces no summary (logged). Documented tradeoff: summarization consumes the user's subscription quota/rate-limits.
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

### Requirement: SQLite-backed summary storage via migration, FK-cascaded

Summaries SHALL be persisted in SQLite via a migration under `src-tauri/migrations/` (registered in `db.rs`), in a table `session_summaries(session_id TEXT PRIMARY KEY REFERENCES sessions(id) ON DELETE CASCADE, summary TEXT NOT NULL, model TEXT, token_cost INTEGER, updated_at INTEGER NOT NULL)`. The `session_id` column SHALL carry a foreign key to `sessions(id)` with `ON DELETE CASCADE` so a deleted session leaves no orphan summary row. A summary row SHALL exist only when summaries are enabled for that session's project.

The durable pull-marker that drives lazy generation (see "Pull-based refresh, historical read, non-blocking") SHALL live in a companion table `session_transcripts(session_id TEXT PRIMARY KEY REFERENCES sessions(id) ON DELETE CASCADE, transcript_path TEXT NOT NULL, last_stop_at INTEGER NOT NULL)`, also FK-cascaded. Both tables SHALL be cleaned up by foreign-key cascade — directly on session deletion and transitively on workspace deletion (`workspaces → sessions → {summaries, transcripts}`) — never by an imperative `DELETE` in the deletion path.

#### Scenario: Summary persisted across restart

- **WHEN** a summary is generated and the app restarts
- **THEN** the persisted summary SHALL still be available in the directory

#### Scenario: No row when disabled

- **WHEN** AI summaries are disabled for a project
- **THEN** no `session_summaries` row SHALL be written for its sessions

#### Scenario: Deleting a session removes its summary and marker

- **WHEN** a session is deleted (directly or via its workspace deletion)
- **THEN** the foreign-key cascade SHALL remove its `session_summaries` and `session_transcripts` rows, leaving no orphaned data

### Requirement: Pull-based refresh, historical read, non-blocking

Summary generation SHALL be **pull-based**: triggered lazily by the read path, not eagerly on every `Stop`. The `Stop` hook SHALL NOT invoke the summarizer; it SHALL only perform a cheap, LLM-free upsert of the session's `(transcript_path, last_stop_at)` into the `session_transcripts` marker table. A session is **dirty** when it has no summary row, or when its `last_stop_at` is newer than its summary's `updated_at`.

Lazy generation SHALL be triggered **only** by `get_session` (the intentional single-session read), never by `list_sessions` (which serves cached summaries only, to avoid a directory poll fanning out into one LLM call per session). When `get_session` resolves a dirty session that is not already being summarized, it SHALL spawn generation detached and return the currently-stored summary (stale or null) — stale-while-revalidate. A read SHALL never wait on summarization.

Because the marker is durable, a **recently-dead** session (one whose `last_stop_at` is within the configured recency window) SHALL also be summarizable on demand: `get_session` SHALL generate from the persisted transcript path on disk even when the session is no longer live.

Concurrency SHALL be bounded so a read with a side effect cannot amplify cost: a per-session debounce window, a per-session single-flight guard, and a process-wide generation semaphore (small fixed cap) SHALL all apply. Each summary SHALL be timestamped.

#### Scenario: Stop does not generate, only marks

- **WHEN** a `Stop` event arrives for a session with a backend enabled
- **THEN** no model SHALL be invoked on the `Stop`; only the `session_transcripts` marker (`transcript_path`, `last_stop_at`) SHALL be upserted

#### Scenario: get_session triggers lazy generation for a dirty session

- **WHEN** `get_session` resolves a dirty session and no generation is in flight
- **THEN** it SHALL spawn generation detached, return the current (stale or null) summary without blocking, and a subsequent read SHALL observe the refreshed summary

#### Scenario: list_sessions never generates

- **WHEN** `list_sessions` is called with one or more dirty sessions in the result
- **THEN** it SHALL return their last-persisted (stale or null) summaries without triggering any generation

#### Scenario: Recently-dead session summarized on demand

- **WHEN** `get_session` resolves a non-live session whose `last_stop_at` is within the recency window and which is dirty
- **THEN** it SHALL generate from the persisted transcript path on disk, the same as for a live session

#### Scenario: Concurrent reads do not stampede

- **WHEN** multiple reads target the same dirty session in quick succession
- **THEN** the single-flight guard SHALL ensure at most one generation runs, and the debounce window SHALL cap how often a session re-summarizes

#### Scenario: Read never blocks on summarization

- **WHEN** a read occurs while a newer summary is still being generated
- **THEN** the read SHALL return the last persisted summary (or null) with its timestamp, without blocking
