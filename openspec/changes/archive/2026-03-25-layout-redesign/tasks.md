## 1. Panel Category System

- [x] 1.1 Define `PanelCategory` type (`"document" | "tool"`) and `PANEL_CATEGORY_MAP` mapping each panel type (plan, spec, transcript, file → document; git, diff → tool) in `rightPanel.ts`
- [x] 1.2 Add `category` field to tab/panel metadata — derive from `PANEL_CATEGORY_MAP` when creating tabs or panel views
- [x] 1.3 Create `getActivePanelCategory()` derived atom that returns the category of the currently active panel/tab (or `null` if no panel open)
- [x] 1.4 Extend contextual right sidebar to show content based on active panel type (plan files, spec files, git files, project tree)

## 2. Layout Preset Engine

- [x] 2.1 Create `stores/layout.ts` with `LayoutPreset` type and preset size constants for terminal-focus, doc-review, tool-workspace
- [x] 2.2 Create `layoutPresetAtom` derived from `getActivePanelCategory()`: `null` → terminal-focus, `document` → doc-review, `tool` → tool-workspace
- [x] 2.3 Create `sessionLayoutPresetAtom` for per-session persistence in `tabStateMapAtom`
- [x] 2.4 Add keyboard shortcut to cycle presets manually

## 3. Workspace Layout Integration

- [x] 3.1 Refactor `Workspace.tsx` to read `layoutPresetAtom` and apply preset sizes via imperative `panel.resize()` calls
- [x] 3.2 Add CSS transition (250ms ease) to panel containers with disable flag during active drag
- [x] 3.3 Enforce terminal minimum size (25%) via `minSize` prop on center panel
- [x] 3.4 Wire `onDragStart`/`onDragEnd` on resize handles to toggle transition animation
- [x] 3.5 Implement left sidebar auto-collapse in tool-workspace preset (40px icon strip)
- [x] 3.6 Restore sidebar on preset change away from tool-workspace or manual Ctrl+B
- [x] 3.7 Add layout mode label to StatusBar component

## 4. Remove Activity Log Panel

- [x] 4.1 Remove activity log ResizablePanel from center column vertical split
- [x] 4.2 Terminal occupies 100% of center column height
- [x] 4.3 Preserve activity event data pipeline (Jotai atoms, transcript parsing) for reuse by timeline

## 5. Activity Timeline — Status Bar

- [x] 5.1 Add activity summary line to StatusBar: last tool action, action count, elapsed time
- [x] 5.2 Make activity section clickable to expand/collapse the timeline drawer
- [x] 5.3 Add keyboard shortcut for drawer toggle

## 6. Activity Timeline — Drawer

- [x] 6.1 Create `ActivityDrawer.tsx` component — slides up from bottom, ~30% viewport height
- [x] 6.2 Implement visual timeline strip (horizontal dots on line, scrubable)
- [x] 6.3 Implement event list with timestamp, tool name, target per event
- [x] 6.4 Parse thinking/reasoning blocks from transcript and render as expandable `[thinking ▾]` sections
- [x] 6.5 Add `[↗ Tab]` button to open DAG graph as tab in right panel

## 7. Activity Timeline — DAG Graph

- [x] 7.1 Add `reactflow` dependency
- [x] 7.2 Create `DagGraph.tsx` component using React Flow — nodes = tool calls, edges = sequence
- [x] 7.3 Implement node click to expand details (tool name, target, duration, exit status, thinking block)
- [x] 7.4 Real-time updates as new tool calls arrive during active session
- [x] 7.5 Register DAG as openable tab in right panel

## 8. Tasks Island

- [x] 8.1 Create `TasksIsland.tsx` component with distinct visual identity (different background, own header)
- [x] 8.2 Position below workspaces in left sidebar with strong visual separation
- [x] 8.3 Make collapsible, auto-hide when no active tasks
- [x] 8.4 Scrollable for overflow, session-scoped task data
- [x] 8.5 Remove tasks panel from right panel registry

## 9. Git Panel v2 — Sidebar + History

- [x] 9.1 Redesign `GitPanel.tsx` with two-area layout: main content + sidebar + fixed commit bar
- [x] 9.2 Implement git sidebar sections: Staged, Unstaged, Stashed — each collapsible with stage/unstage/apply/drop actions
- [x] 9.3 Implement commit bar: message input + Commit, Push, PR buttons
- [x] 9.4 Implement history list view: commit messages + timestamps + short hashes
- [x] 9.5 Implement history graph view: visual commit tree for session branches/merges
- [x] 9.6 Add toggle button between graph and list view modes

## 10. Zen Mode

- [x] 10.1 Create `ZenMode.tsx` overlay component with CSS backdrop-filter blur on terminal
- [x] 10.2 Implement side-by-side diff rendering within the overlay (old/new columns)
- [x] 10.3 Keep git sidebar visible alongside diff overlay for file navigation
- [x] 10.4 Wire triggers: file click in git sidebar, expand button in inline diff, commit click in graph
- [x] 10.5 Implement Esc to exit, restore terminal from blur
- [x] 10.6 Add prev/next file navigation shortcuts + file position indicator ("auth.ts (2/5)")
- [x] 10.7 Handle z-index layering: above terminal, below modals/command palette/toasts

## 11. Plan Annotations — Pinpoint Mode

- [x] 11.1 Implement hover detection on markdown AST elements (paragraph, heading, list item, code block, table cell)
- [x] 11.2 Render dashed outline + label tooltip on hover with context-aware targeting (table edge vs cell, list gap vs item)
- [x] 11.3 Create floating toolbar component (shadcn Popover) with Comment, Replace, Delete, Insert actions
- [x] 11.4 Wire click on highlighted element → open toolbar

## 12. Plan Annotations — Selection Mode

- [x] 12.1 Detect text selection within plan content
- [x] 12.2 Show annotation popover near selection with Comment and Replace buttons
- [x] 12.3 Implement comment input attached to selected text range
- [x] 12.4 Implement replace input pre-filled with selected text

## 13. Plan Annotations — Store & Export

- [x] 13.1 Create annotation store atom: array of `{id, type, target, content, position}`
- [x] 13.2 Render gutter indicators per annotation (blue=comment, yellow=replace, red=delete, green=insert)
- [x] 13.3 Display annotation count in plan panel footer with clear-all button
- [x] 13.4 Implement "Revise" button: serialize annotations → structured instructions → inject via `UserPromptSubmit` hook
- [x] 13.5 Implement "Approve" button: clear annotations and accept plan

## 14. File Browser + CodeMirror 6

- [x] 14.1 Add `codemirror` + core language packages (TypeScript, JavaScript, Rust, JSON, Markdown, CSS, HTML) as dependencies
- [x] 14.2 Create `FileBrowser.tsx` — project tree sidebar component with lazy directory expansion, file icons, search/filter
- [x] 14.3 Create `CodeEditor.tsx` — CodeMirror 6 wrapper with syntax highlighting, line numbers, folding, search/replace, dark theme
- [x] 14.4 Wire double-click in tree → open file as tab in right panel with CodeMirror editor
- [x] 14.5 Implement Ctrl+S to save file changes to disk via Tauri filesystem API

## 15. Session Layout Persistence

- [x] 15.1 Save current layout preset to `tabStateMapAtom` on preset change
- [x] 15.2 On session switch, read stored preset and apply (default to terminal-focus for new sessions)

## 16. Verification

- [x] 16.1 Test: opening plan/spec/transcript/file applies doc-review proportions
- [x] 16.2 Test: opening git/diff applies tool-workspace proportions
- [x] 16.3 Test: closing all panels returns to terminal-focus
- [x] 16.4 Test: Git Full activates from git file click, inline diff expand, and commit graph click
- [x] 16.5 Test: Git Full sidebar navigation updates diff without exiting
- [x] 16.6 Test: Esc exits Git Full and restores terminal
- [x] 16.7 Test: activity status bar shows last action, drawer expands, DAG tab opens
- [x] 16.8 Test: tasks island renders below workspaces, collapses when empty
- [x] 16.9 Test: plan pinpoint mode highlights elements, toolbar appears, annotations serialize correctly
- [x] 16.10 Test: file browser tree loads, CodeMirror opens files, Ctrl+S saves
- [x] 16.11 Test: switching sessions restores correct layout preset
- [x] 16.12 Test: terminal never collapses below 25% in any preset
- [x] 16.13 Run `npx tsc --noEmit` (frontend) + `cd src-tauri && cargo check` (backend) + visual QA
