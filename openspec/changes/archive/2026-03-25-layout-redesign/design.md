## Context

cluihud is a Tauri v2 + React desktop wrapper for Claude Code CLI. The current layout uses `react-resizable-panels` with a fixed 3-column split: sidebar (15%), center/terminal (42%), right panel (43%). The center column has a vertical split: terminal (75%) + activity log (25%).

The right panel hosts both lightweight document viewers (plan, spec, transcript) and space-hungry tool panels (git staging, side-by-side diff). All get the same 43% width. The activity log occupies permanent vertical space with low-frequency information. The plan editor uses a standard MDXEditor toolbar lacking contextual annotation UX.

**Competitors' patterns**: Plannotator uses hover-based pinpoint annotations with dashed outlines and floating toolbars. Google Antigravity uses Google Docs-style inline comments on artifacts. Tamux has an Execution Canvas (React Flow DAG) for tool-call visualization and a Reasoning Stream for agent thinking. T3 Code has unified+split diff views and one-click git flow.

## Goals / Non-Goals

**Goals:**
- Panel proportions adapt based on active context via deterministic presets
- Terminal always visible — compressible but never hidden; Zen Mode blurs it for focused review
- Immersive diff review via overlay (Zen Mode) with git sidebar for file navigation
- Activity log replaced by expandable status bar with visual timeline and optional DAG graph
- Tasks relocated to sidebar as independent island
- Plan annotations with pinpoint + selection modes producing structured agent feedback
- File browser with CodeMirror 6 editor integrated as right panel tab
- Left sidebar auto-collapses in tool-workspace preset

**Non-Goals:**
- Drag-and-drop panel reordering — deterministic shortcuts preferred
- Floating/detachable panels
- Full IDE editor experience (Monaco-level) — CodeMirror 6 for lightweight editing only
- Multiple simultaneous diff views
- Responsive/mobile layout — desktop-only

## Decisions

### 1. Layout presets over continuous resize

**Decision**: 3 discrete layout presets with deterministic keyboard-driven splits.

| Preset | Left Sidebar | Center (Terminal) | Right Panel | Trigger |
|--------|-------------|-------------------|-------------|---------|
| `terminal-focus` | 15% | 85% | collapsed (40px) | No panel open |
| `doc-review` | 15% | 50% | 35% | Document panel (plan, spec, transcript, file) |
| `tool-workspace` | auto-collapsed | 30% | 55% | Tool panel (git, diff) |

In `tool-workspace`, left sidebar auto-collapses to icon strip (40px) to maximize horizontal space. User can re-expand with Ctrl+B.

**Why over free resize**: Presets are predictable, keyboard-friendly, and encode design intent. Users can still manually drag to adjust within a preset, but opening a different-category panel resets to preset.

### 2. Zen Mode for immersive diff review

**Decision**: Full-viewport overlay with CSS backdrop-filter blur on the terminal. Git sidebar visible within the overlay for file navigation.

```
┌─────────────────────────────────────┬──────────────┐
│ ░░░░░░░ terminal (blur) ░░░░░░░░░░ │              │
│ ┌─────────────┬───────────────┐     │  Staged (2)  │
│ │  old version│  new version  │     │  ● auth.ts   │
│ │  (before)   │  (after)      │     │    config.ts  │
│ │             │               │     │──────────────│
│ │ - removed   │ + added       │     │  Unstaged (3)│
│ └─────────────┴───────────────┘     │    index.ts   │
│ ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ │    utils.ts   │
│                             [Esc]   │──────────────│
│                                     │  Stashed (1) │
└─────────────────────────────────────┴──────────────┘
```

**Triggers**: Click on file in git panel sidebar, click "expand" on inline diff, click commit in git history graph.

**Exit**: Esc key or dedicated shortcut. Terminal immediately returns from blur.

**Why overlay not panel split**: Side-by-side diff needs maximum horizontal space. Even at 55% right panel, two code columns are cramped. Overlay gives ~85% viewport width for the diff content while keeping git sidebar for navigation without exiting the mode.

**Alternative considered**: Dedicate right panel to side-by-side. Rejected — 55% width isn't enough for two readable code columns.

### 3. Contextual right sidebar

**Decision**: The right sidebar always shows navigation relevant to the active panel, following the existing pattern (specs sidebar, plan files sidebar). Collapsible via shortcut.

| Active panel | Sidebar shows |
|---|---|
| Plan | Plan files list |
| Spec | Spec files list |
| Diff | Modified files |
| Git | Staged/unstaged/stashed files |
| File browser | Project tree |
| Editor | Project tree |
| Transcript | Session transcripts |

**Why**: Navigation is always accessible. The sidebar acts as contextual index. Same pattern already implemented for specs/plans — extending it to all panels.

### 4. Git panel redesign

**Decision**: Two-area layout — main content area (left) + file sidebar (right) + fixed commit bar (bottom).

**Main area**: History view with two modes togglable via header button:
- **Graph view**: Visual commit graph (branches, merges) for session commits
- **List view**: Simple list with commit message + timestamp

**Sidebar**: Staged / Unstaged / Stashed sections, each collapsible. Stage/unstage actions per file.

**Commit bar**: Fixed at bottom — commit message input + action buttons (Commit, Push, PR, Merge).

**Diff integration**: Click any file in sidebar or any commit in graph → opens Zen Mode with side-by-side diff. Git sidebar remains visible inside Zen Mode for continuous file navigation.

### 5. Activity log → Status bar expandable + timeline drawer + DAG tab

**Decision**: Replace the permanent activity log panel with a 3-tier system:

**Tier 1 — Status bar line (always visible):**
Single line showing last action + action count + elapsed time. Example: `⚡ Write src/auth.ts │ 12 actions │ 2m 34s`

**Tier 2 — Timeline drawer (expandable, ~30% height):**
Click status bar or shortcut → drawer slides up from bottom. Contains:
- Visual timeline strip (dots on line, scrubable)
- Event list with expandable thinking blocks (`[thinking ▾]`)
- Button to open as full tab `[↗ Tab]`

**Tier 3 — DAG graph tab (full right panel):**
Opens as tab in right panel. React Flow DAG visualization of tool-call chains. Nodes = tool calls, edges = sequence. Expandable thinking blocks on each node.

**Why remove permanent panel**: During active development, the activity log has low information density for the vertical space it consumes. The status bar line provides glanceable info. The drawer provides detail on demand. The DAG provides deep analysis when needed.

### 6. Tasks island in left sidebar

**Decision**: Tasks move from right panel to a visually independent section at the bottom of the left sidebar, below workspaces.

Visual separation: different background shade, own header with "Tasks" label, not just a divider. Collapsible. Auto-hides when no active tasks. Scrollable for overflow.

**Why sidebar not status bar**: Tasks are few (typically 3-7) and benefit from persistent visibility. Status bar is too constrained for task names. The sidebar already shows session context — tasks are a natural extension.

### 7. Plan annotations (Plannotator-inspired)

**Decision**: Two interaction modes built on top of existing markdown renderer:

**Pinpoint mode**: Hover over plan elements → dashed outline + label tooltip (paragraph, heading, list item, code block, table cell). Context-aware targeting (table edge → whole table, inner → cell; list gap → whole list, item → individual). Click → floating toolbar appears with 4 actions: Comment, Replace, Delete, Insert.

**Selection mode**: Select text freely → popover appears with Comment and Replace options.

**Annotation store**: Array of `{id, type, target, content, position}`. Rendered as colored markers/gutter indicators on the plan.

**Export on "Revise"**: Annotations serialize to structured instructions injected via `UserPromptSubmit` hook. Format: "Re-read plan at <path>. Address these annotations: [1] DELETE section 'X' — reason: Y. [2] REPLACE 'A' with 'B' in section Z. [3] COMMENT on step N: feedback."

**Why not integrate plannotator directly**: Plannotator opens in browser — fragmented UX. Its core concepts (pinpoint targeting, typed annotations, structured export) are implementable in-app without the dependency. The experience must be native to cluihud.

### 8. File browser + CodeMirror 6

**Decision**: Project tree in right sidebar (when file browser panel active) + CodeMirror 6 editor as tab in right panel.

**CodeMirror 6 over Monaco**: 124KB gzipped vs 2MB+. Modular — import only needed languages. First-class performance on lower-end hardware. Sufficient for our use case (view + light edit, not IDE).

**Features needed**: Syntax highlighting (via language packages), line numbers, folding, search/replace, dark theme matching cluihud palette.

**File tree**: Lazy-loaded directory expansion. File icons by extension. Search/filter at top. Double-click to open in CodeMirror tab.

### 9. Left sidebar auto-collapse

**Decision**: In `tool-workspace` preset, left sidebar auto-collapses to 40px icon strip (same as current collapse behavior). Restores on preset change or manual Ctrl+B.

**Why**: Tool panels (git, diff) need maximum horizontal space. The sidebar content (workspaces, tasks) is less relevant during review workflows. The icon strip keeps quick-access available.

## Risks / Trade-offs

**[Zen Mode z-index complexity]** → The overlay must sit above the terminal but below modals/command palette. Use a dedicated z-index layer. Test with all overlay combinations (settings, command palette, toasts).

**[CodeMirror 6 bundle growth]** → Language packages add up. Start with core languages (TypeScript, JavaScript, Rust, JSON, Markdown, CSS, HTML). Load others on demand.

**[DAG graph performance with many nodes]** → React Flow handles hundreds of nodes well. For very long sessions (500+ tool calls), implement viewport culling or aggregation of sequential similar calls.

**[Plan annotation complexity]** → Pinpoint targeting requires mapping markdown AST positions to DOM elements. MDXEditor already parses to AST — leverage its node positions for hover targeting.

**[Activity log data loss during transition]** → The status bar + drawer shows the same data, just differently. No information is lost — only the permanent panel is removed.

**[Git panel + Zen Mode navigation state]** → When navigating files in Zen Mode, the git sidebar selection must stay in sync. Use shared atom for selected file.
