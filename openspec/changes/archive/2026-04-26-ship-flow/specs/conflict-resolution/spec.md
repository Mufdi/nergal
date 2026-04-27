## ADDED Requirements

### Requirement: Conflict tab type with three-panel layout
The right panel tab system SHALL support a `conflict` tab type that renders three panels side-by-side for a single conflicted file: ours (read-only, base branch version with conflict markers highlighted), theirs (read-only, incoming branch version with conflict markers highlighted), and merged (editable, the working copy the user will commit). Conflict tabs SHALL be singleton per `(session, file path)` pair.

#### Scenario: Open conflict tab for a file
- **WHEN** user clicks "Resolve" on a conflicted file in the git panel conflict list
- **THEN** a `conflict` tab opens in the right panel with three panels (ours | theirs | merged) loaded with the file's current versions

#### Scenario: Reopening same file focuses existing tab
- **WHEN** a conflict tab for `src/foo.ts` is already open and user clicks "Resolve" on the same file
- **THEN** the existing tab is focused; no duplicate tab is created

#### Scenario: Different session has independent conflict tabs
- **WHEN** session A has a conflict tab for `src/foo.ts` open and user switches to session B which has a different conflicted file
- **THEN** session B's tab bar reflects its own conflict tabs; session A's tab remains in memory per existing `tabStateMapAtom` behavior

### Requirement: Conflict tab toolbar actions
The conflict tab SHALL expose four toolbar actions: Accept Ours, Accept Theirs, Ask Claude to Resolve, Save Resolution.

#### Scenario: Accept ours copies ours to merged
- **WHEN** user clicks "Accept Ours"
- **THEN** the merged panel contents are replaced with the ours panel contents

#### Scenario: Accept theirs copies theirs to merged
- **WHEN** user clicks "Accept Theirs"
- **THEN** the merged panel contents are replaced with the theirs panel contents

#### Scenario: Ask Claude to resolve injects context and switches focus
- **WHEN** user clicks "Ask Claude to Resolve"
- **THEN** the backend records a pending annotation-style payload containing `{ path, ours, theirs, merged, instruction }` to be appended to the next user prompt via the existing `inject-edits` hook, and focus switches to the terminal

#### Scenario: Save resolution writes, stages, closes, re-checks
- **WHEN** user clicks "Save Resolution"
- **THEN** the merged contents are written to the file on disk, the file is staged, the conflict tab is closed, and the git panel's conflict list is refreshed; if no conflicts remain, the conflict list section disappears

### Requirement: Git panel conflict list
The git panel SHALL render a conflict list section at the top of the panel (above History) when the active session has one or more conflicted files. Each row SHALL show the file path, a status indicator (C), and a Resolve button that opens or focuses the conflict tab for that file.

#### Scenario: List appears after conflicting merge
- **WHEN** `merge_session` returns `conflict: true` for the active session
- **THEN** the git panel fetches the list of conflicted files and renders a "Conflicts (N)" section at the top of the panel

#### Scenario: List reflects disk state on refresh
- **WHEN** the git panel refresh is triggered (e.g., `files:modified` event) and the session has conflicted files on disk (files with `UU`, `AA`, `DD` status in `git status`)
- **THEN** the conflict list renders those files

#### Scenario: List hidden when no conflicts
- **WHEN** the session has zero conflicted files
- **THEN** the conflict list section is not rendered (no empty "Conflicts (0)" header)

### Requirement: Conflict tab expandable to Zen Mode
The conflict tab SHALL render an Expand button that opens Zen Mode with the same three-panel layout at full-screen dimensions. Exiting Zen Mode SHALL return the user to the tab, preserving any unsaved edits in the merged panel.

#### Scenario: Expand opens Zen Mode conflict view
- **WHEN** user clicks the Expand button in a conflict tab
- **THEN** Zen Mode overlay opens rendering the same three-panel conflict layout at full-screen; the underlying tab remains mounted

#### Scenario: Escape in Zen returns to tab
- **WHEN** user presses Escape in Zen Mode conflict view
- **THEN** Zen Mode closes and the conflict tab is visible with the current edit state preserved

### Requirement: Conflicted files detection endpoint
The system SHALL expose a `get_conflicted_files` Tauri command that returns the list of files with merge conflicts in the active session's working tree, along with `ours`, `theirs`, and current `merged` contents on demand per file.

#### Scenario: List returns UU/AA/DD files
- **WHEN** the session has files in a conflicted state according to `git status --porcelain`
- **THEN** `get_conflicted_files` returns those paths

#### Scenario: Per-file versions endpoint
- **WHEN** the frontend requests versions for a specific conflicted file
- **THEN** the backend returns `{ ours, theirs, merged }` read from `:1:<path>`, `:2:<path>`, `:3:<path>` (git index stages) and the working copy

### Requirement: Contextual resolve-conflict shortcut dispatch
The shortcut currently bound to revise-plan SHALL be converted to a contextual dispatcher that emits one of two events depending on the active context, without introducing a new key binding.

#### Scenario: Active tab is conflict type
- **WHEN** user presses the contextual shortcut while a `conflict` tab is active
- **THEN** the system emits `cluihud:resolve-conflict-active-tab` (triggers "Ask Claude to resolve" on the active tab)

#### Scenario: Git panel with conflicts but no conflict tab active
- **WHEN** user presses the contextual shortcut while the git panel is the active tab and the session has one or more conflicted files
- **THEN** the system opens/focuses the conflict tab for the first conflicted file

#### Scenario: Plan panel with pending review falls back to revise-plan
- **WHEN** user presses the contextual shortcut while a plan panel is active and a plan review is pending
- **THEN** the system emits `cluihud:revise-plan` (current behavior)

#### Scenario: Neither context active is a no-op
- **WHEN** none of the above conditions apply
- **THEN** the system emits no event and shows no toast (silent no-op)
