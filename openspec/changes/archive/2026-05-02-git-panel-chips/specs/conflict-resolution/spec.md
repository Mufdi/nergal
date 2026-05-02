## ADDED Requirements

### Requirement: Conflict surface is a chip, not a tab
The conflict resolution surface SHALL be reached as the `Conflicts` chip inside the GitPanel rather than as a singleton document tab. The `ConflictsPanel` component is unchanged in body but is mounted by `ConflictsChip` instead of `RightPanel.tsx` `DocumentContent`.

#### Scenario: Conflict transition routes to chip
- **WHEN** a session's `activeConflictedFilesAtom` count transitions from 0 to >0
- **THEN** `Workspace.tsx` sets the workspace's chipMode to `conflicts`, pre-selects the first file via `selectedConflictFileMapAtom`, sets `activePanelViewAtom` to `git`, and expands the right panel

#### Scenario: PrViewer failed-merge routes to chip
- **WHEN** `gh_pr_merge` returns `mergeable=false` and an owning session exists
- **THEN** PrViewer flips the workspace's chipMode to `conflicts` and clears the per-session `selectedConflictFileMapAtom` entry so the panel falls back to the first conflicted file

### Requirement: ConflictsPanel onResolved callback
`ConflictsPanel` SHALL accept an optional `onResolved` callback that MUST fire 1.5s after both `files.length` and `pendingMerge` go falsy, gated on prior activity and disabled in Zen mode. The chip wires this to switch to the `PRs` chip.

#### Scenario: Resolve drains, panel routes
- **WHEN** the panel saw activity and now has 0 conflicts and no pending merge
- **AND** is not in Zen mode
- **THEN** after 1500ms the `onResolved` callback fires; the chip translates this to a `chipMode = "prs"` for the workspace
