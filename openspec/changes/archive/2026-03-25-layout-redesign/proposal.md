## Why

The right panel is too narrow for git operations, diffs, and staging — it was designed for document viewing (plans, specs) but now hosts tool-grade panels that need more space. The current fixed 15/42/43 split doesn't adapt to what the user is doing: reviewing a diff needs different proportions than reading a plan. The terminal — which IS the product (Claude Code CLI) — must always remain visible and dominant, but should yield space gracefully when the user enters review/tool workflows. Additionally, the activity log panel occupies permanent vertical space with low-value information during active development, and the current plan editing experience (MDXEditor toolbar) lacks the contextual annotation capabilities seen in tools like Plannotator and Google Antigravity.

## What Changes

- Replace fixed panel proportions with **adaptive layout presets** that adjust based on active context (terminal-only, document review, git/diff workflow)
- Terminal always visible and compressible, with a **Zen Mode** (overlay + blur) for full-screen diff review where the terminal fades to background
- **Contextual right sidebar** — always shows navigation relevant to active panel (plan files, git staged/unstaged, project tree), collapsible via shortcut
- **Git panel redesign** — history/timeline as main content + staged/unstaged/stashed sidebar + fixed commit bar at bottom + session commit graph
- **Zen Mode for diff review** — side-by-side diff overlay with blur background, git sidebar visible for file navigation without leaving the mode
- **Activity log → status bar expandible** — collapsed single-line in status bar, expandable drawer with visual timeline + thinking blocks, optional full tab with DAG graph
- **Tasks relocated** — from right panel to sidebar left as visually independent island below workspaces
- **File browser + CodeMirror 6 editor** — project tree in right sidebar + lightweight code viewer/editor as tab
- **Plan annotations** — Pinpoint mode (hover highlights elements with dashed outline + floating toolbar) and Selection mode (text selection → comment/replace popover), structured feedback export via hooks
- **Left sidebar auto-collapse** in tool-workspace preset for maximum horizontal space
- **BREAKING**: Activity log panel removed from center column. Tasks panel removed from right panel. Right panel default size becomes context-dependent.

## Capabilities

### New Capabilities
- `adaptive-layout`: Context-aware panel proportions with presets (terminal-focus, doc-review, tool-workspace), animated transitions, deterministic splits via shortcuts, sidebar auto-collapse
- `zen-mode`: Full-viewport overlay with blur background for immersive diff review, git sidebar navigation within overlay, Esc to exit
- `activity-timeline`: Status bar expandible with visual timeline, thinking block expansion, drawer mode, and optional DAG graph tab for tool-call chain visualization
- `plan-annotations`: Pinpoint mode (hover-based element targeting with dashed outline + contextual toolbar) and Selection mode (text selection → annotation popover) with structured feedback export (delete/insert/replace/comment)
- `file-browser`: Project tree view in right sidebar with CodeMirror 6 editor for viewing and editing files as tabs
- `git-panel-v2`: Redesigned git panel with history/timeline main area, staged/unstaged/stashed sidebar, session commit graph, fixed commit bar, and Zen Mode integration for diff review

### Modified Capabilities
<!-- No existing specs to modify — tasks panel and activity log are being replaced, not modified -->

## Impact

- **Code**: `Workspace.tsx` (layout orchestration), `RightPanel.tsx` (panel routing), `rightPanel.ts` store, `shortcuts.ts`, `StatusBar.tsx`, `Sidebar.tsx` (tasks island), new `stores/layout.ts`
- **New components**: `ZenMode.tsx`, `ActivityTimeline.tsx`, `ActivityDrawer.tsx`, `DagGraph.tsx`, `PlanAnnotations.tsx`, `FileBrowser.tsx`, `CodeEditor.tsx`, `GitPanel.tsx` (redesign), `CommitGraph.tsx`, `TasksIsland.tsx`
- **New dependencies**: `codemirror` + language packages, `reactflow` (for DAG graph)
- **Removed**: Activity log panel from center column, tasks panel from right panel
- **State**: New atoms for layout presets, zen mode, annotation store, file browser state, activity timeline
- **UX**: Significant workflow changes — users interact with diff via Zen Mode overlay, tasks move to sidebar, activity becomes status bar drawer
