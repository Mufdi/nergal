## 1. Keyboard Shortcuts Infrastructure

- [x] 1.1 Create `src/stores/shortcuts.ts` — Define `ShortcutAction` type: `{ id: string, label: string, keys: string, category: "navigation" | "session" | "panel" | "action", keywords: string[], handler: () => void }`. Export `shortcutRegistryAtom` as a writable Jotai atom holding an array of `ShortcutAction`. Export `focusZoneAtom: atom<"sidebar" | "terminal" | "panel">` with default `"terminal"`. Export `closedTabsStackAtom: atom<Tab[]>` for Ctrl+Shift+T reopen. Populate registry with ALL shortcuts listed in keyboard-shortcuts spec (navigation, session, tab, panel, action categories). Handlers can be stubs initially (just `() => {}`) — they get wired in later tasks.

- [x] 1.2 Create `src/hooks/useKeyboardShortcuts.ts` — Custom React hook that registers a single global `keydown` listener on `document`. On each keydown: (1) read `focusZoneAtom` via `useAtomValue`, (2) if zone is `"terminal"`, check ONLY for Ctrl+Ñ (check `event.code === "Semicolon"` + `event.ctrlKey` to handle both ES and US layouts) — if matched, set `focusZoneAtom` to last non-terminal zone and call `event.preventDefault()`, otherwise return early (let xterm.js handle it), (3) if zone is NOT terminal, iterate `shortcutRegistryAtom` entries, match `event.ctrlKey`/`event.shiftKey`/`event.altKey`/`event.key` against each action's `keys` string, execute first match's handler, call `event.preventDefault()`. The `keys` format should be parseable: e.g. `"ctrl+shift+p"`, `"ctrl+1"`, `"alt+left"`, `"ctrl+tab"`. Mount this hook in `Workspace.tsx` (top-level layout component).

- [x] 1.3 Wire focus zone tracking — In `TerminalManager.tsx`: when xterm.js terminal receives focus (xterm's `onFocus` event or a `mousedown` on the terminal container), set `focusZoneAtom` to `"terminal"`. In `Sidebar.tsx`: on `mousedown` on sidebar container, set `focusZoneAtom` to `"sidebar"`. In `RightPanel.tsx`: on `mousedown` on right panel container, set `focusZoneAtom` to `"panel"`. Use `useSetAtom(focusZoneAtom)` in each component. When Ctrl+Ñ is pressed in terminal zone, move focus to last active non-terminal zone — store `previousNonTerminalZoneAtom` that updates whenever zone changes from sidebar/panel.

## 2. Layout Restructure: Sidebar Simplification

- [x] 2.1 Refactor `src/components/layout/Sidebar.tsx` — Remove the `SidebarTab` type and the 3-tab switcher (`"workspaces" | "tasks" | "git"`). Remove the `<TabIcon>` component. Remove the `activeTab` state and the tab buttons in the header. The sidebar should ONLY render `<WorkspacesView />` directly inside the scrollable area. Keep the collapse/expand toggle button in the header. Remove the import of `TaskPanel` from sidebar. The header should show just "Workspaces" label + collapse button + add workspace "+" button. Keep ALL existing `WorkspacesView` logic intact (session CRUD, resume modal, merge modal, commit modal, expand/collapse workspaces).

## 3. Layout Restructure: TopBar Icon Buttons

- [x] 3.1 Refactor `src/components/layout/TopBar.tsx` — Add a row of icon buttons on the right side of the TopBar for panel types: Plan (document icon), Files (file-list icon), Diff (diff icon), Spec (clipboard icon), Tasks (check-square icon), Git (git-branch icon). Each button should call a `handleOpenPanel(type)` function that: (1) reads `openTabsAtom` to check if a tab of that type already exists, (2) if yes, set it as active via `setActiveTabId`, (3) if no, create a new pinned tab of that type via `openTabAtom` action, (4) if right panel is collapsed, trigger expand via the existing `expandRightPanelAtom` signal. Use Lucide icons (`FileText`, `Files`, `GitCompareArrows`, `ClipboardList`, `CheckSquare`, `GitBranch`). Buttons should be 28x28px with `hover:bg-secondary` transition, `text-muted-foreground` default, `text-foreground` when that type has an active tab. Add tooltips showing panel name + shortcut (e.g. "Plan (Ctrl+Shift+P)").

## 4. Right Panel: Tab Bar Component

- [x] 4.1 Refactor `src/stores/rightPanel.ts` — Replace current flat state with session-scoped tab maps. New types: `Tab = { id: string, type: "plan" | "diff" | "spec" | "tasks" | "git" | "transcript" | "file", label: string, pinned: boolean, dirty: boolean, data?: Record<string, unknown> }`, `TabState = { tabs: Tab[], activeTabId: string | null, previewTabId: string | null }`. New atoms: `tabStateMapAtom: atom<Record<string, TabState>>` (keyed by sessionId), `activeTabStateAtom` (derived: reads `activeSessionIdAtom` and returns `tabStateMapAtom[sessionId]`), `activeTabsAtom` (derived: returns `activeTabState.tabs`), `activeTabAtom` (derived: returns the tab matching `activeTabId`). Action atoms: `openTabAction` (params: tab partial + isPinned boolean — if singleton type and already exists, focus it; if preview, replace existing preview; if pinned, add new), `closeTabAction` (params: tabId — if dirty, don't close (let UI handle confirm); remove from tabs, push to `closedTabsStackAtom`, activate adjacent tab), `pinTabAction` (params: tabId — set `pinned: true`, clear `previewTabId` if matches), `setDirtyAction` (params: tabId, dirty boolean), `reopenTabAction` (pop from `closedTabsStackAtom`, re-add to tabs). Remove old `openTabsAtom`, `activeTabIdAtom`, `activeTabAtom`, `openTabAtom`, `closeTabAtom`. Keep `expandRightPanelAtom`.

- [x] 4.2 Create `src/components/ui/TabBar.tsx` — React component rendering the tab bar. Props: none (reads from `activeTabsAtom`, `activeTabAtom`). Renders a horizontal scrollable container with tab items. Each tab item: (1) type icon (use `PanelTabIcon` function, extend to support all types), (2) title text — italic via `className="italic"` if `!tab.pinned`, (3) close button or dirty dot — if `tab.dirty` show filled white dot (4px circle), else show `×` icon that calls `closeTabAction(tab.id)`. Tab click: `setActiveTabId(tab.id)`. Tab double-click: `pinTabAction(tab.id)`. Active tab has `bg-secondary text-foreground` style, inactive has `text-muted-foreground hover:text-foreground/80`. Overflow: container has `overflow-x-auto scrollbar-none` (hide scrollbar, scroll with mousewheel). If tabs overflow, render a "..." dropdown button at the right edge that opens a list of all tabs for quick selection (use a simple absolute-positioned dropdown). Tab max-width: 160px with truncated text. Tab min-width: 80px.

- [x] 4.3 Refactor `src/components/layout/RightPanel.tsx` — Replace current implementation. Structure: (1) when collapsed, show icon bar (keep existing collapsed view but update to use new atoms), (2) when expanded, render: `<div flex-col>` → `<PanelHeader>` (collapse button + active tab label) → `<TabBar />` → `<div flex-1>` → `<PanelContent tab={activeTab} />` + optional `<FileSidebar />`. Update `PanelContent` switch to support all tab types (plan, diff, spec, tasks, git, transcript, file). For types not yet implemented (diff, spec, git, file), render a placeholder with the type name. Move `PlanFileSidebar` to only render when active tab type is "plan". Add `<FileSidebar />` that renders when active tab type is "diff" or "file" — reads from `activeSessionFilesAtom` and renders a narrow list of file names. Clicking a file calls `openTabAction({ type: "file", label: filename, data: { path } }, false)` (preview mode).

- [x] 4.4 Create `src/components/panel/FileSidebar.tsx` — Narrow sidebar (width: 160px) showing modified files for the active session. Reads from `activeSessionFilesAtom` (already exists in `stores/files.ts`). Each file entry: icon (based on file extension), filename only (not full path), tooltip with full path. Single-click opens preview tab of type "file" with that file's path in data. Double-click opens pinned tab. Empty state: "No modified files" text. Only visible when `activeTab.type` is "diff" or "file" or "files".

## 5. Right Panel: Tab Content Wiring

- [x] 5.1 Move `TaskPanel` to right panel — Update import in `RightPanel.tsx` `PanelContent` to render `<TaskPanel />` when `tab.type === "tasks"`. Remove TaskPanel import from `Sidebar.tsx` (already done in task 2.1). The TaskPanel component itself (`src/components/tasks/TaskPanel.tsx`) needs no changes — it already reads from `activeSessionTasksAtom`.

- [x] 5.2 Wire existing plan panel — In `PanelContent`, `case "plan"` already renders `<PlanPanel />`. Verify it still works with the new tab store. When plan content changes (user edits), call `setDirtyAction(tabId, true)` from PlanPanel or PlanEditor. When plan is saved, call `setDirtyAction(tabId, false)`.

- [x] 5.3 Wire existing transcript viewer — In `PanelContent`, `case "transcript"` already renders `<TranscriptViewer sessionId={tab.sessionId} />`. Update to read `sessionId` from `tab.data?.sessionId` instead of `tab.sessionId` (new Tab type uses `data` Record).

## 6. Status Bar: Git Metadata

- [x] 6.1 Add `get_session_git_info` Tauri command — In `src-tauri/src/commands.rs`, add a new command: `get_session_git_info(db: State<SharedDb>, session_id: String) -> Result<GitInfo, String>` where `GitInfo = { branch: String, dirty: bool, ahead: u32 }`. Implementation: (1) find session by id, (2) if session has `worktree_path`, get branch from `worktree_branch` field, check dirty via `crate::worktree::is_worktree_dirty`, check ahead via `crate::worktree::has_commits_ahead`, (3) if no worktree (first session), get current branch of repo via `git rev-parse --abbrev-ref HEAD`, check dirty on repo root, ahead=0. Register command in `src-tauri/src/lib.rs` invoke_handler.

- [x] 6.2 Create `src/stores/git.ts` — New store: `gitInfoMapAtom: atom<Record<string, { branch: string, dirty: boolean, ahead: number }>>` keyed by sessionId. `activeGitInfoAtom` derived from `activeSessionIdAtom`. Export `refreshGitInfoAction` that calls `invoke("get_session_git_info", { sessionId })` and updates the map. Call refresh: (1) on session switch (when `activeSessionIdAtom` changes), (2) from `stores/hooks.ts` after `post_tool_use` events (tools may modify files), (3) after commit/merge operations.

- [x] 6.3 Update `src/components/layout/StatusBar.tsx` — Add git info section on the left side of the status bar. Read from `activeGitInfoAtom`. Display format: branch icon (GitBranch from Lucide, 12px) + branch name (truncated to 20 chars with tooltip for full name) + dirty indicator (filled orange dot, 6px, only if dirty) + ahead count ("+N" text, only if > 0). If no git info (no active session or loading), show nothing. Existing content (mode badge, session ID, tokens, cost) stays on the right side.

## 7. Command Palette

- [x] 7.1 Create `src/components/CommandPalette.tsx` — Overlay component. State: `isOpenAtom` (boolean atom in shortcuts store), `searchQuery` (local state), `selectedIndex` (local state for arrow key navigation). Render: (1) backdrop div with `fixed inset-0 bg-black/50 z-50`, click to close, (2) centered modal `max-w-lg w-full bg-card rounded-lg shadow-2xl border border-border` positioned at top-third of screen, (3) search input with magnifying glass icon, `autoFocus`, `text-sm`, placeholder "Type a command...", (4) scrollable results list max-height 320px. Each result row: action label (left), key badges (right, e.g. `<kbd>Ctrl</kbd> <kbd>Shift</kbd> <kbd>P</kbd>`), highlighted with `bg-secondary` when selected. Group results by category with small header labels ("Navigation", "Session", "Panel", "Action"). Filter: `action.label.toLowerCase().includes(query) || action.keywords.some(k => k.includes(query))`. Keyboard: Up/Down changes `selectedIndex`, Enter executes selected action and closes, Escape closes. Mount in `Workspace.tsx` as a portal.

- [x] 7.2 Wire Ctrl+K to command palette — In `stores/shortcuts.ts`, add a special entry for Ctrl+K that sets `commandPaletteOpenAtom` to true. In `useKeyboardShortcuts.ts`, handle Ctrl+K specially before the registry loop (since the palette itself needs to handle keys while open). When palette is open, the global handler should be suppressed (palette handles its own keys). Add `commandPaletteOpenAtom` to shortcuts store.

## 8. Hook Wiring: Connect Shortcut Handlers

- [x] 8.1 Wire navigation handlers in shortcuts registry — In `stores/shortcuts.ts`, the handler functions need access to Jotai store. Use `getDefaultStore()` from Jotai to imperatively read/write atoms. Wire: (1) Ctrl+B handler: toggle sidebar by calling `sidebarPanelRef.current?.collapse()/expand()` — since refs aren't accessible from store, instead create `sidebarCollapsedAtom` (boolean) + `toggleSidebarAtom` (action that flips it), and have `Sidebar.tsx` react to it via `useEffect`. Same pattern for right panel: create `rightPanelCollapsedAtom` + `toggleRightPanelAtom`, wire in `Workspace.tsx`. (2) Alt+Left/Right: cycle `focusZoneAtom` between zones, then call `.focus()` on the appropriate DOM element (sidebar container, terminal element, panel container) — store refs in a `focusTargetsAtom: Record<zone, HTMLElement | null>` set by each component on mount.

- [x] 8.2 Wire session handlers — Ctrl+1-9: read `workspacesAtom`, get active workspace, index into its sessions array by number-1, set `activeSessionIdAtom`. Ctrl+N: trigger the same "add session" flow as the "+" button in sidebar (set `addingSessionFor` state — since this is in sidebar component state, create a `triggerNewSessionAtom` that Sidebar listens to).

- [x] 8.3 Wire tab handlers — Ctrl+Tab: read `activeTabsAtom`, find current activeTabId index, advance to next (wrap around), set new `activeTabId`. Ctrl+Shift+Tab: same but go backwards. Ctrl+W: call `closeTabAction` with current `activeTabId`. Ctrl+Shift+T: call `reopenTabAction`.

- [x] 8.4 Wire panel handlers — Ctrl+Shift+P/F/D/S/G/K: each calls `openTabAction` with the corresponding type as a singleton pinned tab. If right panel is collapsed, also increment `expandRightPanelAtom`. Ctrl+Shift+L: toggle the activity log panel collapse state — create `activityLogCollapsedAtom` + `toggleActivityLogAtom`.

- [x] 8.5 Wire action handlers — Ctrl+Shift+E: invoke `open in IDE` (for now, just log or show toast "Not implemented yet" — actual implementation is separate spec). Ctrl+Shift+M: trigger merge modal (create `triggerMergeAtom`). Ctrl+Shift+C: trigger commit modal (create `triggerCommitAtom`). Sidebar listens to these atoms.

## 9. Update Stores: hooks.ts Wiring

- [x] 9.1 Update `src/stores/hooks.ts` — Add git info refresh after tool-use events. In the `post_tool_use` handler (which already updates mode and activity), add a call to `refreshGitInfoAction` for the session_id from the event payload. Also add git refresh in the `stop` handler (after cost update). This ensures the status bar git metadata stays current as Claude modifies files.

## 10. Integration & Verification

- [x] 10.1 `npx tsc --noEmit` passes with no errors
- [x] 10.2 `pnpm vite build` succeeds
- [x] 10.3 `cd src-tauri && cargo check` passes
- [x] 10.4 Manual test: sidebar shows only workspaces/sessions (no tasks/git tabs)
- [x] 10.5 Manual test: TopBar icon buttons open/focus tabs in right panel
- [x] 10.6 Manual test: tab bar shows open tabs, single-click=preview (italic), double-click=pin
- [x] 10.7 Manual test: switching sessions changes tab bar to that session's tabs
- [x] 10.8 Manual test: Ctrl+Ñ toggles focus between terminal and panels
- [x] 10.9 Manual test: Ctrl+1-9 switches sessions
- [x] 10.10 Manual test: Ctrl+K opens command palette, search works, Enter executes
- [x] 10.11 Manual test: status bar shows git branch and dirty indicator
- [x] 10.12 Manual test: Ctrl+Shift+P/F/D/S/G/K open corresponding panel tabs
- [x] 10.13 Manual test: Ctrl+Tab/Shift+Tab navigates between tabs
- [x] 10.14 Manual test: Ctrl+W closes active tab
