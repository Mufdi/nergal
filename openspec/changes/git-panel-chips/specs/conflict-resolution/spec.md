## MODIFIED Requirements

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

## REMOVED Requirements

### Requirement: Ctrl+Alt+Q toggles the Conflicts panel tab
**Reason**: The shortcut becomes obsolete with chip-strip navigation. `Shift+←/→` reaches the Conflicts chip in 1–4 keystrokes from any chip; the conflicts pulse + auto-route already drive the user's attention.
**Migration**: The `open-conflicts` shortcut is removed from `stores/shortcuts.ts`. The "Conflicts" panel button is removed from `TopBar.tsx` `PANEL_BUTTONS`.

### Requirement: openConflictsTabAction
**Reason**: No tab to open. Replaced by `setChipModeMap` + `selectedConflictFileMapAtom` writes at every former call site (Workspace auto-open, PrViewer failed-merge, TopBar conflict badge, Ctrl+Shift+R conflict fallback in shortcuts.ts).
**Migration**: The action is deleted from `stores/conflict.ts`. Each consumer rewires to atom-based chip routing.

### Requirement: ConflictsFilesDrawer
**Reason**: Files picker is now the chip's primary navigation; the drawer was redundant chrome for the tab-mode lifecycle.
**Migration**: `ConflictsFilesDrawer` export is removed from `ConflictsPanel.tsx`; its sole consumer (`ConflictsFilesDrawerWrapper` in `RightPanel.tsx`) is deleted.
