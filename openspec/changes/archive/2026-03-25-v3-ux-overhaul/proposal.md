## Why

cluihud's current layout mixes concerns in the sidebar (workspaces + tasks + git tabs), lacks a proper tab system for managing multiple open documents, and has no keyboard shortcut infrastructure. Session switching is slower than it should be because the sidebar is overloaded. The right panel can only show one thing at a time with no way to keep multiple items open. Power users need keyboard-driven workflows with a command palette for discoverability. These structural issues block all future feature development (diff viewer, spec viewer, git panel, open in IDE).

## What Changes

- **Sidebar simplification**: Remove tabs (tasks/git) from sidebar, keep only workspaces/sessions tree for fast navigation
- **TopBar icon buttons**: Add panel type icons (Plan, Files, Diff, Spec, Tasks, Git) in TopBar for quick access
- **Tab system in right panel**: VS Code-style tabs with preview (single-click, italic) and pinned (double-click, normal) modes. Session-scoped tab state. Mixed content types in same tab bar. Overflow handling with scroll + dropdown
- **File list sidebar**: Narrow sidebar within right panel showing modified files when Files/Diff content is active
- **Status bar git metadata**: Show active branch, dirty indicator, commits ahead count
- **Keyboard shortcuts system**: Focus-aware global handler. Terminal focus passes all keys to PTY except escape key (Ctrl+Ñ). Full shortcut map for navigation, sessions, tabs, panels, and actions
- **Command palette**: Ctrl+K overlay with fuzzy search across all available actions, showing keybindings inline
- **BREAKING**: `rightPanel.ts` store completely refactored — session-scoped tab maps replace flat tab state. Components consuming `openTabsAtom`/`activeTabAtom` must update

## Capabilities

### New Capabilities
- `tab-system`: VS Code-style tab bar in right panel with preview/pin modes, session-scoped state, mixed content types, overflow handling, unsaved indicators
- `keyboard-shortcuts`: Focus-aware global shortcut handler with terminal bypass, full keybinding map, configurable shortcuts store
- `command-palette`: Ctrl+K overlay modal with fuzzy search, action registry, keybinding display, category grouping

### Modified Capabilities

## Impact

- **Frontend (major refactor)**: `Sidebar.tsx` (remove tabs), `RightPanel.tsx` (add tab bar + file sidebar), `TopBar.tsx` (add icon buttons), `StatusBar.tsx` (add git metadata), `stores/rightPanel.ts` (session-scoped tabs)
- **Frontend (new files)**: `TabBar.tsx`, `CommandPalette.tsx`, `useKeyboardShortcuts.ts`, `stores/shortcuts.ts`, `FileSidebar.tsx`
- **Backend**: New command `get_session_git_status(session_id)` for branch/dirty/ahead info in status bar
- **Dependencies**: None new (custom implementations for tabs, shortcuts, command palette)
