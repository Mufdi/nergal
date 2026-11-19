## 1. One-shot signals

- [x] 1.1 Test (Red): `src/stores/shortcutSignals.test.ts` on `appStore` (or a `createStore`), with the `@/lib/tauri` mock from `shortcuts.test.ts`: `consumeCounterSignal` returns false at 0; true once after the add-workspace handler bumps it, leaving the atom at 0; false again (remount); true after a second press. `consumeJumpSignal` returns null at rest, the index once after a `project-N` handler runs, then null. Preconditions: a pending counter with `canAct=false` stays pending; a jump with `workspaceCount=0` stays pending; a jump past `workspaceCount` is reset and returns null.
- [x] 1.2 Add `src/stores/shortcutSignals.ts`. In `Sidebar.tsx`, consume the new-session, add-workspace and jump-to-project signals only after each effect's preconditions pass (jump: clear without acting when workspaces are loaded and the index has no workspace), and remove `lastNewSessionConsumed`.

## 2. Keys yielded to the agent

- [x] 2.1 Test (Red): `src/lib/agentYield.test.ts`, `src/components/terminal/passThrough.test.ts`, `src/lib/focusZoneSync.test.ts` (written; lock at Green start).
- [x] 2.1b Test (Red): `src/lib/shouldYieldToAgent.test.ts`: the ownership × scope × target-zone table.
- [x] 2.2 Add `src/lib/agentYield.ts` (`yieldToAgent`, `isYieldedToAgent`, `shouldYieldToAgent`) and `src/lib/focusZoneSync.ts`. Move `shouldPassThrough` to `src/components/terminal/passThrough.ts` with the yield check first. The dispatcher's app-scope gate uses `shouldYieldToAgent` with `zoneOf(e.target)`, and on yield calls `yieldToAgent(e)` and returns.

## 3. Session switch hands the zone to the terminal

- [x] 3.1 Test (Red): `src/stores/sessionSwitchFocus.test.ts`: with a workspace of two sessions and the zone `sidebar`, the `session-2` handler sets the zone to `terminal` for a fresh session 2 and for an already-active session 2; a non-fresh session 2 still sets `triggerResumeSessionAtom`.
- [x] 3.2 Live branch of `switchToSession`: set the zone to `terminal`, and focus the terminal when the session was already active. The resume consumer in `Sidebar.tsx` sets the zone to `terminal` when it activates the session.

## 4. Verification

- [x] 4.1 `npx tsc --noEmit && pnpm test`; `~/.claude/skills/work/scripts/work-gates.sh main keyboard-focus-fixes`.
- [x] 4.2 Manual walk: press `leader shift+w` (cancel the picker), then collapse and re-expand the sidebar with `leader shift+b` twice. The folder picker does not open again.
- [x] 4.3 Manual walk: jump with `Ctrl+Shift+1`, click the terminal, then collapse and re-expand the sidebar. The focus stays in the terminal.
- [x] 4.4 Manual walk: with `keyboard_ownership: "agent"`, `Ctrl+B` in the terminal does not toggle the sidebar and reaches Claude Code; with `"nergal"`, it toggles the sidebar.
- [x] 4.5 Manual walk: right after starting Nergal, with the focus in the sidebar, `Ctrl+1` opens session 1 and the halo moves to the terminal.
