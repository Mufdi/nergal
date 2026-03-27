## Context

cluihud is a Tauri v2 + React 19 desktop app wrapping Claude Code CLI on Linux. Current layout:
- Left sidebar: 3 tabs (workspaces, tasks, git) — overloaded, slows session switching
- Center: terminal (xterm.js) + activity log (vertical split)
- Right panel: single active tab with icon bar when collapsed. Tab types declared but only plan and transcript implemented
- Status bar: mode indicator, session ID, tokens, cost

The right panel store (`rightPanel.ts`) has `openTabsAtom` and `activeTabIdAtom` but no session scoping, no preview/pin distinction, and no tab lifecycle management.

## Goals / Non-Goals

**Goals:**
- Sidebar = fast session switching only (workspaces + sessions tree)
- Right panel = tabbed document viewer with VS Code-like tab behavior
- Full keyboard-driven workflow with discoverable command palette
- Session-scoped panel state (switching sessions = switching tab context)
- Git metadata visibility in status bar for active session

**Non-Goals:**
- Content implementation of new panel types (diff viewer, spec viewer, git panel, IDE integration) — separate specs
- MDXEditor integration for plan editing — separate spec
- Drag-and-drop tab reordering (future enhancement)
- Tab groups/splits within the right panel (VS Code split editors)
- Configurable/remappable shortcuts (hardcoded first, configurable later)
- Multi-window support

## Decisions

### 1. Tab bar inside right panel (not below TopBar)

The tab bar lives inside the right panel component, not as a full-width bar below TopBar.

**Why**: The terminal never has tabs. A full-width tab bar would create visual noise over the terminal area. Tabs are content of the right panel — they belong there. The TopBar has icon buttons for quick panel access (different concern: "open this type" vs "switch between open items").

**Alternative considered**: Full-width tab bar like a browser. Rejected because it implies all 3 columns share the tab context, which is misleading.

### 2. Session-scoped tab state via Jotai atom map

```typescript
// stores/rightPanel.ts
type TabState = {
  tabs: Tab[];
  activeTabId: string | null;
  previewTabId: string | null;
};
const tabStateMapAtom = atom<Record<string, TabState>>({});
const activeTabStateAtom = atom((get) => {
  const sessionId = get(activeSessionIdAtom);
  if (!sessionId) return { tabs: [], activeTabId: null, previewTabId: null };
  return get(tabStateMapAtom)[sessionId] ?? { tabs: [], activeTabId: null, previewTabId: null };
});
```

**Why**: Same pattern as `planStateMapAtom`, `taskMapAtom`, etc. Proven in v2-multi-session. Switching sessions = derived atom automatically resolves to different tab set. No explicit save/restore logic needed.

### 3. Focus zones with terminal bypass

Three focus zones: `sidebar`, `terminal`, `panel`. A Jotai atom tracks current zone.

```typescript
const focusZoneAtom = atom<"sidebar" | "terminal" | "panel">("terminal");
```

When `focusZone === "terminal"`, the global keydown handler is a no-op (all keys pass to PTY via xterm.js). Only `Ctrl+Ñ` is intercepted at the Tauri webview level to escape terminal focus.

**Why**: xterm.js captures keyboard events in its own DOM element. By checking focus zone before handling shortcuts, we avoid conflicts with Claude Code's own keybindings (Ctrl+R, Ctrl+T, Ctrl+O, etc.).

**Alternative considered**: Intercepting at Tauri's native level with global shortcuts. Rejected because Tauri global shortcuts can't distinguish focus context and would always intercept.

### 4. Command palette as React component (not library)

Custom `<CommandPalette />` component with:
- Fixed action registry array (not dynamic discovery)
- Simple `String.toLowerCase().includes()` filtering (not fuzzy library)
- Rendered as portal overlay with backdrop

**Why**: The action list is finite (~25 actions) and static. A fuzzy search library (fuse.js, etc.) is overkill. Simple substring matching on action name + keywords is sufficient and zero-dependency.

### 5. Icon buttons in TopBar (not icon bar in panel)

Panel type icons go in the TopBar's right section. Clicking an icon opens/focuses a tab of that type in the right panel. If the right panel is collapsed, it expands.

**Why**: TopBar is always visible regardless of panel state. User requested icons in TopBar specifically. When panel is collapsed, the collapsed icon bar still shows open tabs — these are different concerns.

### 6. Git metadata via new backend command

New Tauri command `get_session_git_info(session_id)` returns `{ branch: String, dirty: bool, ahead: u32 }`. Called on session switch and on `post_tool_use` hook events (since tools may modify files).

**Why**: Can't get git info from frontend. Backend already has `check_session_has_commits` which does similar work — extend pattern. Polling not needed because hook events trigger refresh.

## Risks / Trade-offs

- **[Tab state memory]** → Each session accumulates tab state in memory. Mitigation: cap at 20 tabs per session, close oldest preview tabs first.
- **[Ctrl+Ñ on non-Spanish keyboards]** → `Ñ` key doesn't exist on US/EN layouts. Mitigation: bind to the physical key position (`;` on US layout) using `event.code` instead of `event.key`. Document both mappings.
- **[Shortcut collisions with future features]** → Hardcoded shortcuts may conflict with features added later. Mitigation: reserve Ctrl+Shift+{letter} namespace for panel shortcuts, document all bindings in a central registry.
- **[Performance of session switch]** → Switching sessions re-renders tab bar + panel content. Mitigation: tab content components already exist as separate React components, only active tab renders. React.memo on inactive tab bar items.
