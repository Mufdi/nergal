Linear: FRI-496 (plus a related bug reported on 2026-11-18, tracked as a comment on FRI-496)

## Why

Four keyboard bugs make the keyboard do something other than what the focus shows.

1. **Sidebar re-expand replays old shortcuts.** `WorkspacesView` unmounts when the sidebar collapses. On re-expand, its effects see the last counter value of `triggerAddWorkspaceAtom` and `triggerJumpToProjectAtom` and run again. The folder picker opens (FRI-496), or the sidebar takes the focus. Only the new-session signal has a guard (`Sidebar.tsx:327-330`).
2. **Keys yielded to the agent are lost.** With `keyboard_ownership: "agent"` and the focus in the terminal, the dispatcher yields app-scope keys (`Ctrl+B`, `Ctrl+W`, `Ctrl+S`, `Alt+↑/↓`) to the PTY. Then `shouldPassThrough` (`terminalService.ts:1120`) drops the ones on its fixed list. The key reaches neither Nergal nor Claude Code. This breaks the spec scenario "Agent mode passes disputed keys through".
3. **The first keyboard session switch keeps the halo on the sidebar.** `switchToSession` (`shortcuts.ts:130`) changes the active session but never sets `focusZoneAtom`; the click path does (`Sidebar.tsx:613-615`). Later switches are rescued by `Workspace.tsx:161-165`, but the first activation of a run returns early on `isFirstMount` (`Workspace.tsx:130`). `activeSessionIdAtom` starts `null` (`workspace.ts:83`), so the first `Ctrl+<number>` after launch leaves the halo on the sidebar while the new terminal takes DOM focus.

## What Changes

- The add-workspace, jump-to-project and new-session signals are consumed and cleared in their atom, like `triggerResumeSessionAtom`. A remount sees a cleared signal and does nothing.
- An app-scope key that the dispatcher yields to the agent is marked on the event and reaches the PTY. The yield decision uses the zone of the event target, not the stored zone.
- The keyboard session switch hands the focus zone to the terminal, as the click path does.

## Capabilities

### New Capabilities

### Modified Capabilities
- `keyboard-shortcuts`: "Session switching shortcuts" adds the zone hand-off; new requirements cover one-shot signals and yielded keys reaching the PTY.

## Impact

- `src/components/layout/Sidebar.tsx`, `src/components/terminal/terminalService.ts`, `src/hooks/useKeyboardShortcuts.ts`.
- `src/stores/shortcuts.ts` (`switchToSession`).
- New modules: `src/stores/shortcutSignals.ts`, `src/lib/agentYield.ts`, `src/lib/focusZoneSync.ts`, `src/components/terminal/passThrough.ts`.
- Only app-scope keys change. The combos in the terminal's fixed pass-through list that no registry entry claims stay as they are (some of them are webview defaults such as copy).
- No backend, schema or config change. `keyboard_ownership` keeps its meaning: in agent mode `Ctrl+B` goes to Claude Code, and `leader shift+b` toggles the sidebar.
