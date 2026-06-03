## ADDED Requirements

### Requirement: Agent-agnostic context-injection contract
The system SHALL inject assembled vault-note context into an agent session at spawn through the agent-adapter contract, never by bypassing it. `SpawnContext` SHALL carry an optional `injected_context: Option<&str>` (the assembled pinned-note block). The `AgentAdapter` trait SHALL declare a `context_injection() -> ContextInjection` capability whose variant decides how `injected_context` is folded into the launch command:

- `AppendSystemPromptFile` — write the block to an ephemeral per-session file and pass it via a system-prompt-file flag (Claude Code: `--append-system-prompt-file`).
- `PromptPreamble` — prepend the block, fenced and labeled, to the agent's launch prompt (Codex positional `PROMPT`, OpenCode `--prompt`).
- `Unsupported` — the agent exposes no clean per-invocation channel; the block is dropped and the UI says so (no silent loss, no PTY-typed fallback).

The default trait impl SHALL be `Unsupported` so adapters that do not opt in are unaffected. When `injected_context` is `None`, `spawn()` SHALL behave exactly as before for every adapter.

#### Scenario: Claude Code injects via system-prompt file

- **WHEN** a Claude Code session spawns with a non-empty `injected_context`
- **THEN** the adapter SHALL write the block to a per-session file under `~/.config/cluihud/spawn-context/`
- **AND** push `--append-system-prompt-file <path>` into the launch args
- **AND** the user's `initial_prompt`, if any, SHALL remain a separate positional argument

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

### Requirement: Pin vault notes to a session
The system SHALL let the user pin zero or more vault notes to a session, persisted across restarts. Pinned notes SHALL be stored as a JSON array of absolute paths in a `pinned_note_paths` column on the `sessions` table (migration `010`). The system SHALL expose Tauri commands to pin, unpin, and list a session's pinned notes. Pinning SHALL be idempotent (re-pinning the same path is a no-op) and SHALL preserve insertion order.

#### Scenario: Pin persists across restart

- **WHEN** the user pins a note to a session
- **AND** Nergal restarts
- **THEN** `list_pinned_notes` for that session SHALL still return the pinned path

#### Scenario: Unpin removes the note

- **WHEN** the user unpins a previously pinned note
- **THEN** subsequent spawns SHALL NOT include that note in the injected context

### Requirement: Context assembled from pinned notes at spawn and resume
The system SHALL assemble the bodies of a session's pinned notes into a single labeled context block and set it as `injected_context` on every spawn — both fresh starts and resumes — so resuming a session re-injects the current pinned context without a separate code path. Missing note files SHALL be skipped. The assembled block SHALL be capped at a fixed byte budget; on overflow the system SHALL include notes until the budget is reached, append a truncation marker, and log what was dropped (no silent truncation).

#### Scenario: Resume re-injects current pinned context

- **WHEN** a session with pinned notes is resumed
- **THEN** the system SHALL re-read the pinned notes and inject the current context
- **AND** edits made to those notes since the last spawn SHALL be reflected

#### Scenario: Oversize pinned set is truncated visibly

- **WHEN** the assembled pinned-note bodies exceed the byte budget
- **THEN** the system SHALL include notes up to the budget
- **AND** append a truncation marker naming how many notes were omitted
- **AND** log the omission

### Requirement: Hot reload of pinned notes
The system SHALL watch the union of all sessions' pinned note paths with a debounced filesystem watcher. On a change to a pinned note, the system SHALL emit an event identifying the session and path, and the UI SHALL offer an explicit "re-inject updated version" action. The system SHALL NOT auto-re-inject — a change SHALL never silently alter a running agent's context.

#### Scenario: Edited pinned note offers re-injection

- **WHEN** a pinned note is modified on disk while its session is running
- **THEN** the watcher SHALL emit a `vault:pinned-note-changed` event for that session + path
- **AND** the UI SHALL surface an explicit re-inject action
- **AND** no re-injection SHALL happen until the user invokes it

#### Scenario: Re-inject writes the refreshed note to the live session

- **WHEN** the user invokes the re-inject action for a changed note
- **THEN** the system SHALL re-read the note and write a single labeled block into the session's live PTY

### Requirement: Obsidian reading panel
The system SHALL provide a read-only right-panel for reading vault notes inside Nergal. The panel SHALL be registered as a singleton right-panel view, openable from a TopBar icon and the keyboard shortcut Ctrl+Shift+Q, and its entry point SHALL be shown only when a vault is configured. The panel SHALL present (a) the active session's pinned notes and (b) a query-driven vault finder, and SHALL render a selected note's body through the existing markdown + wikilink pipeline. The panel SHALL be read-only: it SHALL NOT offer editing, a graph view, or a backlinks view; editing SHALL be delegated to the "Open in Obsidian" affordance. The panel SHALL share the session's `pinned_note_paths` with the pinning feature — it SHALL NOT introduce separate storage.

#### Scenario: Open the panel and read a pinned note

- **WHEN** the user opens the Obsidian panel for a session that has pinned notes
- **THEN** the panel SHALL list those pinned notes
- **AND** selecting one SHALL render its body via the markdown + wikilink pipeline
- **AND** the body SHALL be loaded through a vault-read command guarded to paths under the configured vault root

#### Scenario: Finder scope defaults to whole vault and toggles to the configured subdir

- **WHEN** the user types a query in the panel's vault finder
- **THEN** the search SHALL default to the whole vault
- **AND** pressing Ctrl+D SHALL toggle the scope to the configured `search_subdir`
- **AND** when no `search_subdir` is configured Ctrl+D SHALL be a no-op
- **AND** this behavior SHALL match the existing vault search modal

#### Scenario: Wikilink navigates within the panel

- **WHEN** the user clicks a `[[wikilink]]` in a note rendered in the panel
- **THEN** the system SHALL resolve the link to a vault note path and load that note in the panel
- **AND** holding Ctrl or Cmd while clicking SHALL open the note in Obsidian instead
- **AND** when the link cannot be resolved to an existing note the system SHALL fall back to opening Obsidian

#### Scenario: Pinning from the panel feeds the injection set

- **WHEN** the user pins a note from the panel's finder
- **THEN** the note SHALL be added to the session's `pinned_note_paths`
- **AND** subsequent spawns SHALL include it in the injected context (subject to the agent's injection tier)

#### Scenario: Panel entry point hidden without a vault

- **WHEN** no vault is configured for the active session
- **THEN** the panel's TopBar entry point SHALL NOT be shown
