1. **High: the root cause of bug 3 is asserted, not verified, and the code contradicts it (Why §3, tasks 3.x).**
   - `Workspace.tsx:161-165` already runs `setFocusZone("terminal"); terminalService.focusActive()` after every session switch.
   - So "Ctrl+<number> from the sidebar leaves the halo on the sidebar" can only happen on the paths that skip that block:
     - the `isFirstMount` return at `Workspace.tsx:130`;
     - the plan-review early `return` at `Workspace.tsx:146`, which focuses nothing;
     - the case where `activeSessionId` doesn't change because session 2 is already active.
   - The plan has no repro and no A/B showing which path fails. A global `focusin` listener may paper over a missing explicit call in one path.
   - It also contradicts the repo's root-cause rule. Name the failing path and fix it, or justify why a global sync is the right design.

2. **High: global focus-follow can regress BUG-09 (spec "Focus zone tracking", task 3.2).**
   - `focusIfPanelZone` (`panelFocus.ts:9`) depends on `focusZoneAtom` staying `terminal` when panel content mounts on a session restore.
   - Under focus-follow, any ungated programmatic `.focus()` into a panel flips the atom to `panel`. That also un-gates every other `focusIfPanelZone` caller, and keystrokes then go to the panel instead of the prompt.
   - Candidates: `PlanPanel.tsx:190`, `SpecPanel.tsx:281`, `RightPanel.tsx:112`, `ClickUpTaskDetail.tsx:140` and `LinearTaskDetail.tsx:117`. The plan doesn't audit any of them.
   - The `focusZone !== "sidebar"` cleanup effect at `Sidebar.tsx:389` also reacts to the new flips. `Workspace.tsx:295-340` reads zones too.
   - Needed: an audit of programmatic `.focus()` call sites and a test that a restored panel view does not steal the zone.

3. **High: the yield decision uses the stale atom, not the event target (task 2.2).**
   - The dispatcher gate (`useKeyboardShortcuts.ts:348-351`) uses `focusZoneAtom`, the same atom that bug 3 says goes stale.
   - If the atom says `terminal` but DOM focus is in a panel, the key is marked as yielded and never reaches a terminal listener. The key is lost again, in the same way as bug 2.
   - If the atom says `sidebar` but the terminal textarea has focus, Nergal steals the key in agent mode.
   - Gate on `e.target.closest("[data-focus-zone='terminal'|'quake']")`, or at least make that the second condition.

4. **Medium: `yieldToAgent(e)` followed by `continue` is inconsistent (task 2.2).**
   - After marking the event, the `for (const action of registry)` loop keeps scanning at `useKeyboardShortcuts.ts:304`.
   - A later entry with the same combo, or a non-app-scope duplicate, will match, call `preventDefault` and run its handler. The event is then both marked and consumed.
   - Mark and `return`, or break out of the loop.

5. **Medium: `takeSignal` ordering is not specified against the existing early returns (task 1.2).**
   - The current new-session guard sits after `workspaces.length === 0` (`Sidebar.tsx:416-418`).
   - The jump effect returns early on `!ws` (`Sidebar.tsx:453-454`).
   - If `takeSignal` is called first, it burns the press without acting. Workspaces load async via `get_workspaces`, so this is reachable.
   - Specify "consume only after the preconditions pass", and add a test for it.

6. **Medium: a module-level global is a patch, and the repo already has a root-fix precedent.**
   - `triggerResumeSessionAtom` is consumed and cleared (`Sidebar.tsx:408`).
   - A consume-and-clear atom, or a watermark held in an atom or the `appStore`, fixes the cause. It also makes tests resettable and avoids hidden state surviving HMR or tests.
   - `takeSignal(name, value)` keyed by string is hidden global state.
   - The jump signal is `{tick: Date.now(), index}`. The plan never says to pass `tick`, and a wall-clock value is not a strictly monotonic counter if the system clock steps backwards.

7. **Medium: the spec doesn't define a press made while `WorkspacesView` is unmounted.**
   - With the sidebar collapsed, `leader shift+w` or `leader n` bumps the counter with no consumer.
   - On re-expand, the watermark is below the counter, so it fires then. That is arguably desirable, but "once per press" and the scenarios don't say.
   - Decide whether such a press should expand the sidebar and act, or be dropped. Add a scenario for it.

8. **Medium: test coverage doesn't touch the real defects (tasks 1.1, 2.1, 3.1).**
   - The tests are pure-helper tests, with `zoneOf` using a fake `closest`.
   - Nothing tests:
     - the dispatcher in agent versus nergal mode;
     - the keydown to `shouldPassThrough` integration;
     - the `focusin` listener setting the atom, including idempotence when the zone is unchanged;
     - a Sidebar unmount/remount that does not replay.
   - The yield marker's lifetime is untested too. A `WeakSet` is fine, but a property on the event object is not.
   - Tasks 4.2-4.4 are all manual, and nothing manual covers quake, Ctrl+W/S, Alt+↑/↓ in agent mode, or the jump replay (Ctrl+Shift+N). Add a mount-based test with a fresh `appStore` for the Sidebar.

9. **Medium: the keys that stay dropped are not enumerated (Impact, task 2.2).**
   - The `shouldPassThrough` list (`terminalService.ts:1120-1150`) also covers combos with no registry entry, or entries of non-app scope.
   - Examples: Ctrl+K, Ctrl+Shift+X/J/O/U, and Ctrl+; or Ctrl+Tab when unbound.
   - Those are still lost whenever the dispatcher doesn't mark them. The plan fixes only "app-scope yielded" keys, and the Why claims the full list.
   - Either scope the claim to app-scope entries, or decide the policy for unmarked keys.

10. **Low: the focus-sync edge cases are underspecified (task 3.2).**
    - Zones `activity`, `clickup`, `linear` and `worktree-gate` exist. `zoneOf` returns `null` for non-nested ones such as activity and worktree-gate, which is OK. But whether `worktree-gate` sits inside the `terminal` zone is unchecked.
    - The terminal textarea reclaims focus after the auto-copy toast (`terminalService.ts:691-700`). That would flip the zone back to `terminal` mid-interaction.
    - Radix portal dialogs sit outside every zone, so the zone is untouched on open. On close, focus restore sets it, which is fine but should be tested.
    - Specify "mounted once" as a separate `[]`-deps effect, not inside the existing `[registry, overrides, …]` effect, which re-subscribes.

11. **Low: rollback and ordering.**
    - There is no flag, but the pieces are independent. The plan should land bugs 1 and 2 separately from the focus-follow change, since the latter has the widest blast radius.
    - Moving `shouldPassThrough` "unchanged" into a new module while also adding the yield check is two changes in one step.
    - Security: no new surface.

VERDICT: REVISE

=== Round 2 (sonnet) ===
Re-review of round 2. Most round-1 items are addressed. Two defects remain, and I'm returning REVISE.

**Round-1 status**
- **Bug 3 (root cause):** addressed. The `isFirstMount` early return at `Workspace.tsx:130` is a verified path, and `switchToSession` (`shortcuts.ts:130-145`) never sets the zone.
- **Global `focusin` sync:** dropped in favour of a targeted hand-off. This resolves the BUG-09 concern.
- **Yield by event target, mark and `return`, `WeakSet`:** addressed.
- **Consume-and-clear instead of a module global:** addressed.
- **Land the fixes separately:** not addressed, though it is minor.

**New and remaining issues**

1. **High: setting the zone atom without moving DOM focus recreates the desync bug (D2, task 3.2).**
   - Today `switchToSession` returns early on the resume branch and never touches focus.
   - If session 2 is already active, `Workspace.tsx:127` returns early, so the `focusActive()` block at `:161-165` never runs. The atom becomes `terminal` while DOM focus stays on the sidebar zone element.
   - On the resume branch the atom flips before the session exists. `triggerResumeSessionAtom` has no consumer when the sidebar is collapsed, and its consumer silently clears the trigger when the session is not found (`Sidebar.tsx:412`). Both leave the zone at `terminal` with nothing focused.
   - The design assumes "the new terminal takes DOM focus" without checking it.
   - Fix: on the live branch set the zone and call `terminalService.focusActive()`, as `togglePanel` does at `shortcuts.ts:162-163`. On the resume branch set the zone in the consumer, next to `setActiveSessionId`.
   - Add a test for the already-active case.

2. **Medium: the "unknown workspace index" rule leaves a stale jump signal (One-shot signals requirement, D1).**
   - Pressing `Ctrl+Shift+5` with 3 workspaces leaves the signal unconsumed by design.
   - It stays in the atom and fires on a later remount, once a fifth workspace exists. That is the same replay bug in a new form.
   - Fix: clear the signal when the index is out of range once the workspaces have loaded. Keep it only while `workspaces.length === 0`.

3. **Medium: the fallback to the stored zone contradicts the spec, and it can lose the key (D3 versus the "Keys yielded" requirement).**
   - The spec yields only when the target is inside the terminal or quake zone. D3 falls back to `focusZoneAtom` when the target is in no zone.
   - If focus orphans to `<body>` (the documented panel-collapse case) while the atom says `terminal`, the key is yielded to a PTY that never receives it. That is bug 2 again.
   - Fix: yield only when `zoneOf(target)` is `terminal` or `quake`. Drop the fallback.

4. **Medium: the early-return ordering the design claims is untested (D1, task 1.1).**
   - The tests cover the consume helpers only.
   - "Clear only when it acts" lives in the `Sidebar` effects: workspaces loaded, index valid, `handleAddWorkspace`.
   - Nothing prevents a regression where the consume happens first and burns the press.
   - Suggested shape: `consume(store, atom, canAct)`, so the helper test encodes the precondition.

5. **Medium: the dispatcher change has no automated coverage.**
   - Three spec scenarios cover agent versus nergal mode and "the event target decides the zone". The only coverage for them is a manual walk, task 4.4.
   - Extract the yield decision into a pure function, e.g. `shouldYield(ownership, scope, targetZone)`. Test it with the table of ownership, scope and zone.
   - `focusZoneSync.test.ts` uses a fake `closest` that matches on the exact selector string. That couples the test to the implementation's selector format.

6. **Low: `return` instead of `continue` changes behaviour.**
   - Only six entries have `scope: "app"`. The remaining check is whether any non-app entry shares a combo with them, for example `alt+arrow`. Such an entry used to be reachable after `continue`. Grep the duplicates and state the result in the design.

7. **Low: "a press while collapsed acts once on expand" has no scenario or test.**
   - A stale `leader n` or `Ctrl+Shift+N` made while collapsed will open the picker or steal focus whenever the user expands the sidebar, possibly much later.
   - Either decide to drop presses made while unmounted (for example, clear on collapse) or add the scenario.

8. **Low: naming.**
   - `focusZoneSync.ts` now only exports `zoneOf`, so the name is misleading. Rename it to `zoneOf.ts` or `focusZone.ts`.

VERDICT: REVISE

=== Round 3 (sonnet) ===
Round 3: the round-2 defects are mostly fixed, but one medium test gap and a few lows remain.

1. **Round-2 #1 (zone set without DOM focus): resolved.** D2 now sets the zone and calls `focusActive()` on the live branch, including the already-active session. The resume consumer sets the zone only when it finds the session. `Workspace.tsx:161-165` re-asserts focus on later switches, so the extra rAF focus is harmless. The "already active" scenario and task 3.1 cover it.
   - **Low:** 3.1 only asserts the zone. The `focusActive` call is the actual fix for the desync, so mock `terminalService` (or `requestAnimationFrame`) and assert it is called. The spec scenario already claims "terminal has DOM focus".
   - **Low:** the resume branch relies on "the terminal takes DOM focus when it mounts". `TerminalManager.tsx:89` and `:120` call `focusActive`, which plausibly supports that. Walk 4.5 covers it, so no action is needed beyond keeping that walk.

2. **Round-2 #2 (stale out-of-range jump): resolved.** D4 and the spec scenario "A jump past the last workspace is dropped" cover it, and the `workspaces.length === 0` wait is kept. Task 1.2 implements it. Nothing automated tests it, but that is the same effect-level gap as item 3.

3. **Round-2 #3 (zone fallback): resolved.** D3 drops the fallback, and the "target outside every zone is not yielded" scenario is added. The spec and design now agree.

4. **Round-2 #4 (early-return ordering untested): Medium, only partly addressed.**
   - The design accepts this as a risk, with the rationale that Vitest has no DOM. That rationale is weak. The ordering bug is "consume before the precondition check", and it does not need a DOM.
   - Put the precondition in the helper: `consumeCounterSignal(store, atom, canAct)` and `consumeJumpSignal(store, atom, {loaded, count})`. The unit test can then cover three cases: signal pending with `canAct=false` is not cleared, an out-of-range index is cleared, and a valid index is returned and cleared.
   - The Sidebar effect then becomes a one-line call, and the "wait for workspaces" and out-of-range rules (D1/D4) get automated coverage. Without this, those two rules rest on review and the manual walk.

5. **Round-2 #5 (dispatcher coverage): resolved.** Task 2.1b adds the pure `shouldYieldToAgent` table test.
   - **Low:** `focusZoneSync.test.ts` is still described in the plan as using a fake `closest` keyed on the exact selector string. That couples the test to the implementation. Have `zoneOf` take a small `{closest(sel)}` interface and assert on the returned zone for nested targets. Avoid asserting the selector strings.

6. **Round-2 #6 (`return` vs `continue`): resolved.** I checked `shortcuts.test.ts:17`. The "no duplicate combo" test runs over the whole registry, and leader continuations are namespaced via `comboSignature`. D3's claim holds.

7. **Round-2 #7 (press while collapsed): resolved.** The spec scenario and the risk entry are present. A test would need the Sidebar effect, which is the same gap as item 4.

8. **Round-2 #8 (naming): not addressed. Low.** `focusZoneSync.ts` and its test now only export `zoneOf`, and the "Sync" name is misleading. The plan removed the sync and kept the file name. Rename the file to `focusZone.ts` before it lands, since renaming later costs more.

9. **Round-1 "land separately": not addressed. Low.** Four fixes are bundled. The three causes are independent, so ordering the tasks so each group (1, 2, 3) is its own commit is enough for easy rollback.

10. **New, Low:** task 2.1 is marked `[x]` for tests that are "written; lock at Green start", and `focusZoneSync.test.ts` is on that list. If item 5 or 8 changes `zoneOf`, that test changes too. Resolve this before treating 2.1 as locked.

The only item worth another round is the testability of the precondition (item 4), and it is cheap. The rest are low.

VERDICT: REVISE
