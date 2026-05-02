## ADDED Requirements

### Requirement: Chip-strip navigation in the GitPanel
The GitPanel SHALL render a 5-chip strip immediately below the branch header: `Files`, `History`, `Stashes`, `PRs`, `Conflicts`. Each chip owns a dedicated full-width body that replaces the previous two-column layout.

#### Scenario: Default chip is Files
- **WHEN** the GitPanel mounts for a workspace with no persisted chip
- **THEN** the active chip is `Files`

#### Scenario: Active chip persists per workspace
- **WHEN** the user activates a non-default chip in workspace A, switches to workspace B, and returns to A
- **THEN** workspace A's chip restores to whatever was last active there

#### Scenario: Chip cycle via Shift+Arrow
- **WHEN** the user presses `Shift+ArrowRight`
- **THEN** the next chip in `[files, history, stashes, prs, conflicts]` becomes active, wrapping at the end
- **AND** `Shift+ArrowLeft` cycles in the opposite direction
- **AND** the listener no-ops when an input/textarea/contenteditable holds focus

### Requirement: Conflicts chip pulses when count > 0
The `Conflicts` chip SHALL render with a red ring + pulse animation whenever the active session has unresolved conflicts. When the count is zero and the chip is not active, it SHALL render dimmed (opacity-50) to deprioritize it visually.

#### Scenario: Pulse appears at first conflict
- **WHEN** `activeConflictedFilesAtom.length` transitions from 0 to a positive number
- **THEN** the Conflicts chip in the strip pulses + glows red and shows the count next to its label

#### Scenario: Auto-route to Conflicts chip on first conflict
- **WHEN** the active session has 0 conflicts and a poll surfaces ≥1 conflict
- **THEN** the workspace's chipMode flips to `conflicts`, the first conflicted file is pre-selected, and the right panel expands

### Requirement: Conflicts chip auto-resolves to PRs chip
When the user resolves all conflicts in the Conflicts chip, the GitPanel SHALL switch to the PRs chip 1.5s after activity drains, **provided** the panel had recorded activity (was non-empty at some point) and is not in Zen mode.

#### Scenario: Resolve last conflict in chip mode
- **WHEN** the user finalizes the last conflict resolution in the Conflicts chip
- **AND** files.length transitions from >0 to 0
- **THEN** after 1500ms the workspace's chipMode is set to `prs`

#### Scenario: Zen mode skips auto-resolve
- **WHEN** the user resolves the last conflict while ConflictsPanel is rendered in Zen mode (`inZen=true`)
- **THEN** no chip transition fires

### Requirement: Stashes chip exposes the git stash stack
The `Stashes` chip SHALL provide a list of `git stash list` entries (index, branch, message, age) plus a create-stash input box at the bottom. Each row supports apply (Enter), pop (`p` or `Shift+Enter`), drop (`d`, two-press confirm with 3s timeout), branch (`b` opens a name input), and expand (Space, lazy-loads files via `git_stash_show`).

#### Scenario: Empty state
- **WHEN** the chip is active and the repo has no stashes
- **THEN** the body shows "No stashes yet" + a hint pointing to the create input

#### Scenario: Drop requires double-press
- **WHEN** the user presses `d` on a focused stash row
- **THEN** the row enters confirm state for 3 seconds; pressing `d` again within that window drops the stash; otherwise it auto-cancels

#### Scenario: Branch from stash
- **WHEN** the user presses `b` on a focused row
- **THEN** an inline branch-name input appears; submitting with Enter calls `git stash branch <name> stash@{N}`

### Requirement: PRs chip is a picker → embedded viewer
The `PRs` chip SHALL render a file-picker-style PR list (j/k/Enter to select). Selecting a PR replaces the chip body with the `PrViewer` embedded in place plus a "All PRs" back-strip. `Backspace` returns to the picker.

#### Scenario: Backspace returns to picker
- **WHEN** the user is viewing a PR (`selected !== null`)
- **AND** presses `Backspace` outside an input/textarea
- **THEN** the chip returns to picker mode

#### Scenario: Apply with Claude tooltip on disabled
- **WHEN** the Apply-with-Claude button is disabled (no owning session, or owning session not active)
- **AND** the user hovers it
- **THEN** a Tooltip surfaces the specific gating reason (no matching branch / wrong active session)
- **AND** the button label includes the annotation count: `Apply with Claude (N)`
