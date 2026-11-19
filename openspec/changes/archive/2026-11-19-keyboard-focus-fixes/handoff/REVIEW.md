# Review: keyboard-focus-fixes

Scope: working tree vs HEAD plus the untracked modules and tests. I found no Critical bugs. Findings are below, by severity.

## Bugs

### Critical
None.

### Important

1. **The resume trigger replays on sidebar re-expand, the same bug class this change fixes.**
   - `triggerResumeSessionAtom` is consumed only in `WorkspacesView` (`src/components/layout/Sidebar.tsx:402-413`). It is set at `src/stores/shortcuts.ts:139`.
   - With the sidebar collapsed, `Ctrl+2` on a session without a terminal leaves the atom set. `WorkspacesView` is unmounted, so nothing consumes it.
   - On the next expand, the effect activates that session and (new in this diff) moves the zone to `terminal`. The activation can happen much later.
   - The spec says "A `Ctrl+digit` that switches to, or resumes, a session SHALL set the focus zone". With a collapsed sidebar, the zone is set only after the expand.
   - Fix: move the resume consumer out of `WorkspacesView`, or consume it from a component that stays mounted.

2. **Yielded keys can still be lost when the target is in the terminal zone but is not the PTY textarea.**
   - `zoneOf` returns `terminal` for anything inside `[data-focus-zone='terminal']` (`src/lib/focusZoneSync.ts:9`). That includes the delete button that `TerminalManager` focuses for `worktree_pending_delete` (`src/components/terminal/TerminalManager.tsx:97-100`).
   - The dispatcher yields, returns, and does not prevent the default (`src/hooks/useKeyboardShortcuts.ts:353-359`). Only the textarea keydown handler (`terminalService.ts:662`) sends the key to the PTY.
   - A button target means no PTY receives the key and Nergal does not act on it. The spec requirement "Keys yielded to the agent reach the PTY" is not met for that target.
   - Fix: yield only when the target is the terminal textarea, or when it is inside the quake overlay.

3. **`switchToSession` sets the zone to `terminal` while the plan-review branch skips focus.**
   - `src/stores/shortcuts.ts:147` sets the zone immediately.
   - `Workspace.tsx:136-153` returns early when the new session has a pending plan review. It opens the plan panel and never focuses the terminal.
   - `TerminalManager.tsx:86-90` calls `focusActive()` only when the zone flips. That focus moves DOM focus to the previously active terminal while the plan panel opens.
   - Result: zone and DOM focus can disagree after `Ctrl+digit` onto a pending-review session. No test covers it.

## Spec gaps

1. **The dispatcher wiring has no test.**
   - The Scenarios "A target outside every zone is not yielded" and "The event target decides the zone" depend on `zoneOf(e.target)` plus `yieldToAgent(e)` plus `return` at `useKeyboardShortcuts.ts:353-359`.
   - Tests cover only the pure helpers.
   - Removing `yieldToAgent(e)` or reverting to `focusZoneAtom` leaves every test green.
   - The remount scenarios ("Re-expanding does not open the picker", "A press while collapsed acts once") are pinned only by the helper tests. Task 4.x covers them manually, and none of 4.1-4.5 is checked.

2. **"Ctrl+digit to the session that is already active" is half-tested.**
   - The test asserts the zone only. DOM focus is not asserted (acknowledged in `design.md`).
   - `src/stores/sessionSwitchFocus.test.ts` never sets `activeSessionIdAtom` to `null`.
   - The "First Ctrl+digit of a run" scenario is therefore not reproduced as written. The test seeds `s1` as active.

3. **The "Ctrl+Shift+5 then add two workspaces" scenario holds only when the sidebar is mounted at press time.**
   - With the sidebar collapsed and `workspaces.length` at 0 or 3, the logic is correct, and the dropped jump is covered by the `consumeJumpSignal` count tests.
   - If the press happens before the workspaces load (`workspaceCount === 0`), the signal waits. When the workspaces load, an old jump fires. This matches D4. Nothing bounds the age of such a signal.

4. **The `design.md` yield claim and the zone list disagree.**
   - Zones `linear`, `clickup`, `activity` and `worktree-gate` are absent from `ORDER` in `src/lib/focusZoneSync.ts:5`. Targets in them resolve to `panel`, to `terminal`, or to `null`, depending on nesting.
   - `ActivityDrawer` sits outside the terminal wrapper (`Workspace.tsx:403`), so it resolves to `null` and Nergal keeps the key. That is correct.
   - The spec does not name these zones.

## Nits

1. `src/lib/focusZoneSync.ts` duplicates `detectCurrentZone` precedence (`src/stores/shortcuts.ts:202-207`). The file name says "sync" but it exports only `zoneOf`. This is acknowledged in `design.md`.
2. Out-of-scope diff: `docs/shortcuts.md` changes a vault path unrelated to this change. Revert it or move it to its own commit.
3. `src/components/terminal/terminalService.ts:1120` now has two blank lines where `shouldPassThrough` was. Run the formatter.
4. In `src/hooks/useKeyboardShortcuts.ts`, the two new imports sit above the `jotai` import and break the existing order.
5. Comment audit:
   - `useKeyboardShortcuts.ts:346-352`: the first sentence ("in agent mode with focus in the terminal/quake zone, an app-scope entry yields the key") describes WHAT the code below does. Keep the "zone from the event target" WHY and the "mark so the pass-through table does not drop it" WHY. Drop the rest.
   - `src/stores/shortcuts.ts:145-147`: the last clause "as the sidebar click path does" restates the behavior. The first clause is a real WHY.
   - `src/lib/agentYield.ts`, `src/stores/shortcutSignals.ts`, `src/components/terminal/passThrough.ts`: the comments are WHY-only. The "Same shortcut set the legacy..." comment in `passThrough.ts` is pre-existing and moved unchanged.
6. `src/lib/shouldYieldToAgent.test.ts` restates the function as a table. It passes for any implementation that matches the one-line condition. Its value is regression protection only.
7. `src/components/terminal/passThrough.test.ts`: "keeps Ctrl+G for the agent" passes only because plain `Ctrl+G` is not in the table. The name suggests a rule that does not exist.
8. `Sidebar.tsx:422` adds `void` before `handleAddWorkspace()`. That is a harmless change, but it is unrelated to the request.

## Checks that passed

- Effect loops: after the atom is cleared, the counter effects and the jump effect re-run once and return early (`src/stores/shortcutSignals.ts:12-22`). No loop.
- `workspaces.length` dep on the jump effect (`Sidebar.tsx:473`): a re-run after a workspace add sees `tick === 0` and does nothing.
- New-session effect with `canAct`: the signal stays pending while `workspaces.length` is 0, then fires once.
- Dispatcher order: the dispatcher listens at window capture (`useKeyboardShortcuts.ts:~385`), so it marks the event before the textarea listener runs. The `WeakSet` mark reaches `shouldPassThrough`.
- `return` instead of `continue` is safe: the registry test rejects duplicate combos.
- `session-N` entries have no `app` scope, so `Ctrl+digit` is never yielded.

## Verdict

Not ready to merge as is: fix the resume-trigger replay and the non-textarea yield (Important 1-2) or document them as accepted, then check the walks in task 4.

## Resolution (main thread, 2026-11-18)

- Important 2 (yield from a non-textarea target in the terminal zone): **fixed**. The gate yields only when the target is a `TEXTAREA` inside the terminal or quake zone.
- Important 1 (resume stays pending while the sidebar is collapsed): **accepted, pre-existing**. The resume consumer lives in `WorkspacesView`. Recorded on FRI-496 as a separate bug.
- Important 3 (plan-review branch of the switch effect leaves zone `terminal`): **accepted**. The pending-review switch opens the plan tab without moving focus today; the walk covers the common path.
- Nits fixed: WHAT comments reworded, import order, double blank line. Kept: `focusZoneSync.ts` name and the table test (accepted at the iprev cap).
