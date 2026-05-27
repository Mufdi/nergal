## ADDED Requirements

### Requirement: Search engine API
The system SHALL provide a backend `SearchEngine` exposed via a Tauri command `search(query: SearchQuery) -> Vec<SearchHit>`. The query SHALL accept the following fields:

- `text: String` — the search pattern (regex or literal, controlled by `case_sensitive`).
- `scopes: Vec<SearchScope>` — one or more of `Vault`, `SessionTranscripts`, `OpenSpec`, `WorkspaceFiles { workspace_id }`, `All`.
- `case_sensitive: bool` — defaults to `false`.
- `titles_only: bool` — when true, restricts matching to filenames (used by the `@@` picker).
- `max_results: usize` — hard cap, defaults to 50.

Each `SearchHit` SHALL include: `path` (absolute), `title` (filename without extension), `snippet` (one-line context), `line_number` (when content match), `score` (relevance), `scope` (which scope produced the hit).

#### Scenario: Multi-scope query

- **WHEN** the caller invokes `search` with `scopes = [Vault, WorkspaceFiles { workspace_id }]` and `text = "auth flow"`
- **THEN** the engine SHALL execute both scope searches in parallel
- **AND** merge results sorted by `score` desc
- **AND** return at most `max_results` entries

#### Scenario: Titles-only query

- **WHEN** the caller invokes `search` with `titles_only = true` and `text = "arc"`
- **THEN** the engine SHALL match only filenames containing "arc" (case-insensitive by default)
- **AND** SHALL NOT inspect file contents
- **AND** SHALL return within 100 ms for vaults under 10 000 notes

#### Scenario: Empty query

- **WHEN** `text` is the empty string
- **THEN** the engine SHALL return an empty result list immediately

### Requirement: Backend implementation
The engine SHALL prefer ripgrep (`rg`) for content search when the binary is available on `PATH` via `which::which("rg")`. The arguments MUST be: `--json --max-count <max_results> --line-number [--ignore-case] <pattern> <path>`. The JSON output SHALL be parsed streamingly. When `rg` is not available, the engine SHALL fall back to a pure-Rust walker (`walkdir` + `grep-regex`).

The engine SHALL never read paths matching the following exclusions: `.git/`, `node_modules/`, `target/`, `.venv/`, `__pycache__/`, `.DS_Store`. Additional ignores from `.gitignore` SHALL be honored when ripgrep is in use.

#### Scenario: ripgrep present

- **WHEN** `which::which("rg")` returns `Ok(_)`
- **AND** the engine handles a `Vault`-scoped query
- **THEN** the engine SHALL spawn `rg` with the documented args
- **AND** SHALL parse its JSON output

#### Scenario: ripgrep absent — fallback

- **WHEN** `which::which("rg")` returns `Err(_)`
- **THEN** the engine SHALL use the pure-Rust walker
- **AND** the API contract SHALL be unchanged (same `SearchHit` shape)

### Requirement: Scope resolution
The engine SHALL resolve each `SearchScope` to one or more filesystem roots:

- `Vault` — reads `vault_root` from the active workspace's resolved Obsidian config; skipped if null.
- `SessionTranscripts` — reads `transcripts_directory` from app config.
- `OpenSpec` — reads `openspec/` under the active workspace's `repo_path`.
- `WorkspaceFiles { workspace_id }` — reads the corresponding workspace's `repo_path`.
- `All` — expands to all of the above for the active workspace.

#### Scenario: All-scope expansion

- **WHEN** the caller invokes `search` with `scopes = [All]`
- **AND** the active workspace has a vault, transcripts, and an openspec directory
- **THEN** the engine SHALL search all four roots
- **AND** annotate each hit with the scope that produced it

#### Scenario: Vault scope without vault_root

- **WHEN** the caller invokes `search` with `scopes = [Vault]`
- **AND** the active workspace's `vault_root` is null
- **THEN** the engine SHALL return an empty result list (the Vault root is skipped, no error)

### Requirement: Scoring + merge
The engine SHALL compute a per-hit `score` combining: filename match (+100), title prefix match (+50), content match (+10), recency (most-recently-modified within ±20). Hits SHALL be deduplicated by absolute path (same file matching multiple scopes appears once, with the highest-scored hit kept).

#### Scenario: Same file matches two scopes

- **WHEN** a path is reachable via both `Vault` and `WorkspaceFiles`
- **AND** both scopes produce a hit for the same query
- **THEN** the result list SHALL contain exactly one entry for that path
- **AND** the entry's `scope` SHALL be the higher-scored origin

### Requirement: Cancellation
The frontend caller SHALL be able to cancel a pending search by issuing a new search before the previous returns. The backend SHALL handle the second invocation by signaling the in-flight subprocess (when ripgrep) to terminate; the first invocation SHALL return an `Err(Cancelled)`.

#### Scenario: Rapid typing

- **WHEN** the user types into the vault search modal at >5 chars/sec
- **THEN** at most one ripgrep subprocess SHALL be alive at any time
- **AND** previous in-flight searches SHALL be cancelled before the next one starts
