---
status: archived
implemented: 2026-03-25
archived: 2026-04-04
files:
  - src/components/ui/TabBar.tsx
  - src/stores/rightPanel.ts
  - src/components/layout/TopBar.tsx
  - src/components/layout/RightPanel.tsx
  - src/components/layout/StatusBar.tsx
  - src/components/layout/Sidebar.tsx
---

## Purpose

Implement a session-scoped tab bar in the right panel with preview/pinned behavior, overflow handling, unsaved indicators, and TopBar icon integration.

## Implementation Notes

Core requirements implemented. Known deviations:
- **Preview tabs**: Mechanism exists (previewTabId, pinTabAction) but UI lacks italic styling and single-click trigger is not enforced in TabBar.
- **Pinned tabs**: pinTabAction exists but no double-click handler in TabBar. Auto-pin on edit not wired.
- **Unsaved confirmation dialog**: closeTabAction blocks closure but no modal dialog shown — tab just stays open.
- **Status bar git metadata**: Shows branch name but not from tabs context.
## Requirements
### Requirement: Tab bar renders in right panel header
The right panel SHALL display a horizontal tab bar below the panel header showing all open tabs for the active session. Each tab SHALL display a type icon, title text, and close button.

#### Scenario: Tab bar with multiple tabs
- **WHEN** the active session has 3 open tabs (plan, diff, tasks)
- **THEN** the tab bar shows 3 tab items with appropriate icons and titles in order of opening

#### Scenario: Empty tab state
- **WHEN** the active session has no open tabs
- **THEN** the right panel shows an empty state message with hint text

### Requirement: Preview tab behavior (single-click)
Single-clicking an item in file list, spec list, or plan list SHALL open it as a preview tab. Preview tabs SHALL render with italic title text. Only one preview tab SHALL exist at a time per session — opening a new preview replaces the previous one.

#### Scenario: Open preview tab
- **WHEN** user single-clicks a file in the file sidebar
- **THEN** a preview tab opens with italic title, showing that file's content

#### Scenario: Preview replaces preview
- **WHEN** a preview tab for "auth.rs" is open and user single-clicks "main.rs"
- **THEN** the "auth.rs" preview tab is replaced by "main.rs" preview tab

#### Scenario: Preview does not replace pinned
- **WHEN** a pinned tab for "auth.rs" and a preview tab for "main.rs" exist, and user single-clicks "config.rs"
- **THEN** the "main.rs" preview is replaced by "config.rs", the "auth.rs" pinned tab remains

### Requirement: Pinned tab behavior (double-click)
Double-clicking an item SHALL open it as a pinned tab with normal (non-italic) title. Pinned tabs persist until explicitly closed. Editing content in a preview tab SHALL auto-pin it.

#### Scenario: Double-click pins tab
- **WHEN** user double-clicks a spec in the spec list
- **THEN** a pinned tab opens with normal title text

#### Scenario: Editing auto-pins
- **WHEN** a preview tab is open for a plan and user modifies the content
- **THEN** the tab becomes pinned (title changes from italic to normal)

### Requirement: Session-scoped tab state
Each session SHALL maintain its own independent set of open tabs. Switching the active session SHALL switch the tab bar to show that session's tabs without destroying the previous session's tab state.

#### Scenario: Switch session preserves tabs
- **WHEN** session A has tabs [plan, diff] and session B has tabs [spec, tasks], and user switches from A to B
- **THEN** tab bar shows [spec, tasks] and session A's tabs remain in memory

#### Scenario: New session starts with no tabs
- **WHEN** a new session is created
- **THEN** the tab bar is empty for that session

### Requirement: Tab types support mixed content
The tab bar SHALL support these content types in the same bar: plan, diff, spec, tasks, git, transcript, file, conflict. Each type SHALL have a distinct icon. Singleton types (tasks, git) SHALL have at most one tab per session. `conflict` tabs SHALL be singleton per `(session, file path)` pair — opening a conflict tab for an already-open file focuses the existing tab.

#### Scenario: Singleton tab reuse
- **WHEN** tasks tab is open and user triggers "open tasks" again
- **THEN** the existing tasks tab is focused, no duplicate created

#### Scenario: Multiple file tabs
- **WHEN** user opens diff for "auth.rs" and then pins diff for "main.rs"
- **THEN** both file-specific tabs coexist in the tab bar

#### Scenario: Conflict tab reuse per file
- **WHEN** a conflict tab for `src/foo.ts` is open and user clicks Resolve on the same file again
- **THEN** the existing conflict tab is focused; no duplicate created

#### Scenario: Multiple conflict tabs for different files
- **WHEN** session has conflicts in `src/a.ts` and `src/b.ts` and user clicks Resolve on both
- **THEN** two conflict tabs coexist in the tab bar, one per file

### Requirement: Tab overflow handling
When tabs exceed the available width, the tab bar SHALL scroll horizontally. Navigation arrows SHALL appear at bar edges. A dropdown button SHALL show all open tabs for quick selection.

#### Scenario: Overflow with scroll
- **WHEN** 8 tabs are open and only 5 fit visually
- **THEN** left/right scroll arrows appear and a "..." dropdown button shows all 8 tabs

### Requirement: Unsaved changes indicator
Tabs with unsaved modifications SHALL display a dot indicator replacing the close button icon. Attempting to close a tab with unsaved changes SHALL show a confirmation dialog.

#### Scenario: Plan with edits shows dot
- **WHEN** user modifies a plan in the editor
- **THEN** the plan tab shows a filled dot instead of the × close icon

#### Scenario: Close unsaved tab confirms
- **WHEN** user clicks close on a tab with unsaved changes
- **THEN** a confirmation dialog appears with Save/Discard/Cancel options

### Requirement: TopBar icon buttons open panel tabs
The TopBar SHALL display icon buttons for panel types (Plan, Files, Diff, Spec, Tasks, Git). Clicking an icon SHALL open or focus a tab of that type in the right panel. If the right panel is collapsed, it SHALL expand.

#### Scenario: Icon opens new tab
- **WHEN** no plan tab is open and user clicks the Plan icon in TopBar
- **THEN** a new Plan tab opens in the right panel and becomes active

#### Scenario: Icon focuses existing tab
- **WHEN** a plan tab is already open and user clicks the Plan icon
- **THEN** the existing plan tab becomes active (focused)

#### Scenario: Icon expands collapsed panel
- **WHEN** the right panel is collapsed and user clicks any TopBar icon
- **THEN** the right panel expands and the corresponding tab opens

### Requirement: File sidebar within right panel
When a Files or Diff tab is active, a narrow sidebar SHALL appear on the right edge of the panel listing modified files for the active session. Clicking a file SHALL open its diff as a preview tab.

#### Scenario: File sidebar appears for file-related tabs
- **WHEN** user opens a Files tab
- **THEN** a narrow sidebar appears listing files from the active session's modified files atom

#### Scenario: File sidebar hidden for non-file tabs
- **WHEN** user switches to a Tasks or Plan tab
- **THEN** the file sidebar is hidden

### Requirement: Status bar shows git metadata
The status bar SHALL display the active session's git branch name, dirty indicator (dot), and commits-ahead count. This data SHALL refresh on session switch and after tool-use hook events.

#### Scenario: Status bar shows branch info
- **WHEN** active session is on branch "cluihud/fix-auth" with uncommitted changes and 3 commits ahead
- **THEN** status bar shows "⎇ cluihud/fix-auth ● +3"

#### Scenario: Main branch session shows no worktree info
- **WHEN** active session has no worktree (first session, uses repo root)
- **THEN** status bar shows "⎇ main" with no dirty or ahead indicators

### Requirement: Sidebar shows only workspaces and sessions
The left sidebar SHALL contain only the workspace/session tree. Tasks and Git tabs SHALL be removed from the sidebar. The sidebar header SHALL have no tab switcher.

#### Scenario: Sidebar has no tabs
- **WHEN** the app loads
- **THEN** the sidebar shows only the workspaces tree with expandable sessions, no tab bar at top

