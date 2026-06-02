# obsidian-session-moc Specification

## Purpose
TBD - created by archiving change obsidian-bridge. Update Purpose after archive.
## Requirements
### Requirement: MOC builder
The system SHALL provide an `obsidian::moc::MocBuilder` that, given a `session_id` and a resolved `ObsidianConfig`, builds a per-session MOC note. The builder MUST: (a) read the session's block from the workspace's continuous `session_log_path`, (b) pull cost / tasks / agent status from SQLite, (c) compute git diff stats for the worktree if present, (d) render the result through a markdown template, (e) write the file atomically (tmp + rename) at `<moc_path>/<slug>-<YYYY-MM-DD>.md`. Re-runs over the same `session_id` SHALL overwrite the existing file (idempotent regeneration).

The session name SHALL be slugified: diacritics stripped (using a deterministic mapping), lowercase, non-alphanumeric → dash, collapsed dashes, trimmed.

#### Scenario: Build MOC for completed session

- **WHEN** the runner picks up a marker for session "Refactor auth (auth-ref-#3)" in workspace W with `moc_path = ~/Obsidian/Projects/cluihud/MOCs/`
- **THEN** the builder SHALL produce `~/Obsidian/Projects/cluihud/MOCs/refactor-auth-auth-ref-3-2026-05-26.md`
- **AND** the file SHALL contain the template populated with session-specific data

#### Scenario: Idempotent overwrite

- **WHEN** the same session marker is processed twice (e.g. recovery path after a crash)
- **THEN** the second build SHALL overwrite the first file
- **AND** the file SHALL contain the most-recent data

#### Scenario: Missing session_log entry

- **WHEN** the marker is for a session that produced no events (no header in session_log)
- **THEN** the builder SHALL still write a minimal MOC noting "No activity recorded"
- **AND** the marker SHALL be considered processed (deleted)

### Requirement: MOC template
The MOC file SHALL contain (in order):

1. YAML frontmatter:
   - `session_id`, `session_name`, `workspace`, `agent` (id), `model`, `started_at` (ISO), `ended_at` (ISO), `cost_usd`, `files_count`, `tasks_count`, `trigger`.
2. `# <session name>` heading.
3. `## Activity` section: chronological list of event lines copied from the session_log block.
4. `## Files touched` section: bullet list. Each file is rendered as `[[<vault-relative path>]]` if it resolves inside the vault, else as a plain `\`<workspace-relative path>\``. Each bullet shows the tool that touched it (e.g. `Edit · ` prefix).
5. `## Decisions` section: bullets from `db.get_annotations(session_id)` filtered to type=`global-comment` or `revision-feedback`.
6. `## Links` section: PR URLs from `db` (if any recorded), workspace cluihud URI `cluihud://open-workspace?path=<abs>`.
7. `## Git diff stats` section: output of `git diff --stat <merge-base>..HEAD` for the worktree.

The template SHALL be a constant string in `obsidian/moc.rs`, with substitution via `format!` calls — no external templating dependency.

#### Scenario: Files inside vault rendered as wikilinks

- **WHEN** the session edited `<vault_root>/Projects/foo/notes.md`
- **THEN** the "Files touched" section SHALL render that path as `[[Projects/foo/notes]]`
- **AND** the wikilink SHALL be detected by N1 backlink propagation

#### Scenario: Files outside vault rendered plain

- **WHEN** the session edited `<workspace_repo>/src/foo.rs`
- **AND** that path is outside `vault_root`
- **THEN** the section SHALL render `\`src/foo.rs\`` (no wikilink)

#### Scenario: No git diff available

- **WHEN** the session's worktree was removed before the runner executes
- **THEN** the "Git diff stats" section SHALL be omitted entirely (no empty section)

### Requirement: Reverse backlink propagation (N1)
After writing the MOC, the system SHALL parse the MOC for `[[Wikilinks]]` and propagate a backlink to each linked vault note. The propagation MUST: (a) be skipped entirely when `cfg.backlinks_enabled == false`, (b) only target wikilinks resolvable to actual files inside the vault (skip dangling links — Obsidian will handle them when clicked), (c) write to each target note an opt-in `## Referenced in Nergal sessions` section delimited by HTML comment markers `<!-- nergal-backlinks-start -->` / `<!-- nergal-backlinks-end -->`, (d) be idempotent (re-running over the same MOC produces no duplicate entries).

The backlinks section in each target note SHALL be ordered most-recent first and SHALL be capped at 50 entries. Older entries beyond 50 SHALL be moved into a `<details><summary>Older references</summary>…</details>` block inside the same section.

Each entry SHALL have the form: `- [[<MOC slug>]] — <session name> (<ISO date>)`.

#### Scenario: New target note gains the section

- **WHEN** the MOC contains `[[Architecture]]`
- **AND** `<vault_root>/Architecture.md` exists but has no backlinks section yet
- **THEN** the updater SHALL append the section with the bracket markers and one entry pointing at this MOC

#### Scenario: Existing section gets a new entry

- **WHEN** `<vault_root>/Architecture.md` already has a `<!-- nergal-backlinks-start -->...<!-- nergal-backlinks-end -->` section with 3 entries
- **AND** a new MOC referencing `[[Architecture]]` is generated
- **THEN** the new entry SHALL be inserted at the top of the list
- **AND** the section markers SHALL be preserved exactly

#### Scenario: Cap rolls oldest into details

- **WHEN** the section already has 50 entries
- **AND** a new MOC references it
- **THEN** the oldest entry SHALL be moved into the `<details>` block
- **AND** the visible list SHALL still contain 50 entries (newest at top)

#### Scenario: Dangling wikilink skipped

- **WHEN** the MOC contains `[[NonExistentNote]]`
- **AND** no file matching `<vault_root>/**/NonExistentNote.md` exists
- **THEN** the propagation SHALL skip that wikilink
- **AND** no new file SHALL be created (Obsidian creates dangling notes when clicked)

#### Scenario: backlinks_enabled = false

- **WHEN** the workspace's `backlinks_enabled` is false
- **THEN** the propagation step SHALL be entirely skipped
- **AND** the MOC SHALL still be written (only N1 is gated, not #11)

### Requirement: Idempotent re-runs
The MocBuilder + BacklinkUpdater pair SHALL produce the same vault state on every re-run for the same `session_id`, regardless of how many times the marker is processed. Reruns SHALL NOT duplicate backlink entries (the updater detects existing entries pointing at the same MOC slug and replaces them, not appends).

#### Scenario: Marker processed twice

- **WHEN** the runner processes session S's marker
- **AND** later (after recovery) the same marker is re-introduced (e.g. a manual replay)
- **AND** the runner processes it again
- **THEN** the vault SHALL end in the same state as after the first run (one MOC file, no duplicate backlink entries)

