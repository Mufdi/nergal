## ADDED Requirements

### Requirement: Visible local-merge action with dedicated shortcut
The GitPanel SHALL render a visible "Merge into… (local)" action in the commit bar alongside Commit/Push/Ship. The action SHALL have a dedicated keyboard shortcut (`Ctrl+Shift+M`). The action SHALL open the MergeModal for branch selection. The action SHALL NOT be hidden inside an overflow/kebab menu and SHALL be reachable without Tab navigation discovery.

#### Scenario: Merge action visible in commit bar
- **WHEN** the GitPanel commit bar renders for a session with a worktree branch
- **THEN** the row of action buttons includes "Merge into… (local)" alongside Commit, Push, and Ship, each with its visible `<kbd>` chip showing the shortcut

#### Scenario: Ctrl+Shift+M opens the MergeModal
- **WHEN** the user presses Ctrl+Shift+M while the GitPanel is the focus zone
- **THEN** the MergeModal opens with the branch list focused

#### Scenario: Sidebar no longer hosts a Merge entry
- **WHEN** the user inspects any Sidebar context menu or hover-action for a session
- **THEN** no "Merge" entry appears (the Sidebar is no longer an entry point for local merge)

### Requirement: Visible kbd chips on every git action
Each interactive action surface in the GitPanel commit bar, ShipDialog footer, and ConflictsPanel action row SHALL render its keyboard shortcut as a visible `<kbd>`-styled chip alongside the action label. The chip SHALL display modifier glyphs appropriate to the OS (e.g., `⌃⇧Y` on macOS, `Ctrl+Shift+Y` on Linux/Windows). The `title` attribute MAY remain as a fallback but SHALL NOT be the sole means of shortcut discovery.

#### Scenario: Commit/Push/Ship/Merge buttons render kbd chips
- **WHEN** the GitPanel commit bar renders
- **THEN** each of Commit, Push, Ship, and Merge buttons displays a small `<kbd>` chip with its shortcut text in muted styling

#### Scenario: ConflictsPanel O/T/B/Reset/Save actions render kbd chips
- **WHEN** the ConflictsPanel action row renders
- **THEN** each of Ours (O), Theirs (T), Both (B), Reset, and Save buttons displays a `<kbd>` chip with its shortcut

#### Scenario: ShipDialog footer Ship/Cancel render kbd chips
- **WHEN** Step 2 of the ShipDialog renders
- **THEN** Ship displays `Ctrl+Enter` chip and Cancel displays `Esc` chip

## MODIFIED Requirements

### Requirement: Single source of git actions in GitPanel
The GitPanel SHALL host all git action buttons (Commit, Push, Ship, Merge) in the commit bar at the bottom of the panel. The header bar SHALL NOT duplicate Ship or Push buttons. The header SHALL display only passive, informational elements: branch name, ahead count, PR badge, CI checks status, and the ExternalLink to the PR URL.

#### Scenario: Commit bar is the only place with action buttons
- **WHEN** the GitPanel renders for a session with `ahead > 0 && !prInfo`
- **THEN** the action buttons (Commit, Push, Ship, Merge) appear only in the commit bar; the header shows only branch name, `+N ahead` indicator, and any PR/CI info

#### Scenario: Header has no clickable Ship/Push buttons
- **WHEN** the user inspects the GitPanel header
- **THEN** no Ship-it badge button or Push button appears in the header (the prior duplicates from v1 are removed)

#### Scenario: Ship-it surface remains discoverable via shortcuts
- **WHEN** the user has not yet looked at the commit bar but presses `Ctrl+Shift+Y`
- **THEN** the Ship dialog still opens (shortcut works regardless of GitPanel scroll position; the commit bar is `shrink-0` and stays pinned at the bottom of the panel)

### Requirement: Inline conflicts list and Complete-merge surface
The GitPanel SHALL display an inline conflicts section above the commit bar when the active session has conflicted files OR when a pending merge awaits a final commit. Each conflicted file SHALL show its name and a "Resolve" button that opens the Conflicts tab for that file. When all conflicts are resolved and a pending merge exists, the panel SHALL show a "Complete merge" affordance with its keyboard shortcut.

#### Scenario: Conflicts section appears when files have conflicts
- **WHEN** the active session has 3 conflicted files after a merge attempt
- **THEN** the GitPanel renders the conflicts section listing each file with a "Resolve" button

#### Scenario: Complete-merge surface appears after all conflicts resolved
- **WHEN** all conflict markers are resolved and saved, a pending merge state exists
- **THEN** the GitPanel renders a green "All conflicts resolved — ready to finish merge" banner with a "Complete merge" button and its `<kbd>` chip

#### Scenario: Auto-merge conflict alert appears in same session
- **WHEN** an auto-merged PR is detected as blocked by conflict (per `conflict-resolution`)
- **THEN** the GitPanel renders an additional inline alert "Auto-merge blocked by conflict — review and resolve" with a button that opens the Conflicts tab
