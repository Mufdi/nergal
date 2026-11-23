# REVIEW: mouse-tracking-aux-buttons (FRI-506)

## Adversarial

---
type: review
feature: mouse-tracking-aux-buttons
reviewer: adversarial
reviewed_at: 2026-11-23T00:00:00Z
verdict: PASS-WITH-FOLLOWUPS
---

> Scope: `git diff mufdidev/fri-604-plan-approve-key -- src src-tauri` plus the three new `#[path]` test files. `docs/shortcuts.md` ignored. **Important** = would ship a defect, a vulnerability, or a divergence from spec/plan. **Nit** = everything else, max 5.

### Diff scope

- 8 tracked files (+65 / -11), 3 new Rust test files (143 lines).
- Re-ran locally: `cargo test --lib mouse_tracking` 9 passed; vitest `mouseButtons.test.ts` 13 passed.

### Pass 1 — Bugs

- Checked, clean: the gate matches wezterm-term's emission rule. `is_mouse_grabbed()` (`term/src/terminalstate/mod.rs:748-750`) is the exact guard in `mouse_button_press` (`mouse.rs:144`) and `mouse_button_release` (`mouse.rs:195`). Modes 1000/1002/1003 are independent flags there, so the snapshot reads them as wezterm does.
- Checked, clean: `GridUpdate` has one producer, `GridDiffer::compute_update` (`src-tauri/src/terminal/differ.rs:77-88`). The attach path (`src-tauri/src/pty.rs:2389-2396`) and the emitter loop (`src-tauri/src/terminal/emitter.rs:147-164`) both go through it. Quake shells share `showTerminal`, so they get the field too.
- Checked, clean: first emission, `invalidate()` and the unattached skip all resend the current value. The sync-output hold only delays it, as the plan accepts.
- Checked, clean: the alt screen has no scrollback, so `grid_snapshot` clamps `scroll_offset` to 0 there (`src-tauri/src/terminal/session.rs:491-495`). A stale offset cannot block vim or htop.
- Checked, clean: a release after tracking turns off still reaches wezterm-term. wezterm-term clears its held button and stays silent.
- [Nit] `src/components/terminal/terminalService.ts:744` — The gate reads the frontend `scrollOffset`, which lags the backend. A wheel-up then an instant right click forwards the press. The report then names a viewport row, not a live row. The window is one coalesce tick plus IPC. → Accept, or set `entry.scrollOffset` optimistically in the wheel handler.

### Pass 2 — Security

- No findings. No new IPC command, config key or trust boundary. `terminal_mouse_button` is unchanged.
- Observation: any program can now unlock aux reports on the primary screen with `ESC[?1000h`. A crashed app leaves the mode on, so a right click types `ESC[<2;x;yM` at the shell prompt. That matches xterm and wezterm, and the bytes are fixed-format digits with no CR.

### Pass 3 — Compliance

- [Important] `openspec/changes/mouse-tracking-aux-buttons/proposal.md:11` — The claim "wezterm and xterm do not report then either" is false for wezterm. wezterm's GUI scrolls to the bottom and reports (`wezterm-gui/src/termwindow/mouseevent.rs:931-938`). The issue asks for wezterm's rule, so the stated rationale is wrong. The chosen behaviour is safe, but the record misstates it. → Reword the proposal and the spec rationale as Nergal's own choice. Or follow wezterm: `scroll_to_bottom` and forward.
- [Important] `src/components/terminal/terminalService.ts:744`, `:867`, `:1321` — Three scenarios rest only on unchecked walks. "Mouse tracking on the primary screen" needs walk 3.3. "Alternate screen without mouse tracking" needs walk 3.4. "Scrolled-back primary screen does not forward" needs walk 3.5. The vitest covers the pure helper, not this wiring. → Run walks 3.3-3.5 before closing.
- [Important] `src/components/terminal/terminalService.ts:153` — Scenario "Release outside the canvas" has no test and no walk. The `mouseToCell(entry, e) ?? release` fallback is never exercised. Walks 3.3-3.5 never release off the pane. → Add a walk step: right-press over `cat -v`, release outside, expect a release at the press cell.
- [Important] `src/components/terminal/mouseButtons.test.ts:54-58` — Scenario "Focus loss while a button is held" has no real test. The case hand-sets `afterBlur = null`. It never calls the clearing path at `terminalService.ts:626`. No walk covers it. → Add a walk step, or extract the focus-loss clear into a tested helper.
- [Important] `openspec/changes/mouse-tracking-aux-buttons/tasks.md` — Tasks 3.1 and 3.2 stay unchecked. `evidence.md` records the 3.1 gates but no `work-gates.sh` run. → Run `work-gates.sh` and record it.
- [Important] `openspec/changes/mouse-tracking-aux-buttons/proposal.md:12` — "Claude Code enables mouse tracking" has no evidence yet. The user-visible effect depends on it: every right click in the agent pane pastes. Walk 3.5 alone checks it. → Record the observed DECSET modes during walk 3.5.
- [Nit] `src/components/terminal/terminalService.ts:744` — wezterm's rule also has a bypass modifier. Shift skips reporting (`mouseevent.rs:924-929`). Here a tracking app swallows every right click, and Shift only adds 4 to the code. The agent pane loses the context menu with no escape. → Skip forwarding when `e.shiftKey`, or record the gap as out of scope.
- [Nit] `openspec/changes/mouse-tracking-aux-buttons/implementation.md` ("Per-phase risk") — It says a held release goes out "at the press cell". `terminalService.ts:153` prefers the pointer cell and falls back to the press cell only off-canvas. → Fix the sentence.
- [Nit] `src-tauri/src/terminal/session.rs:403-407`, `src-tauri/src/terminal/types.rs:63-64` — Comments drift from the new gate. `mouse_button` lists `?1006h` as a mouse mode; that is an encoding, and `?1003h` is missing. The `GridUpdate` doc still says `scroll_offset` is always 0. The aux gate now depends on that offset. → Update both comments.
- [Nit] `src/components/terminal/terminalService.ts:1321` — The new assignment carries two extra spaces of indent. → Align it with `entry.isAltScreen` above.

Acceptance criteria (delta spec `specs/terminal-wezterm/spec.md`):

- [x] Right click with SGR mouse mode — `src-tauri/src/terminal/session.rs:408-446`; test `session.rs:993-1006`.
- [x] Middle click with SGR mouse mode — same anchor and test.
- [.] Release outside the canvas — `terminalService.ts:153`; no test, no walk.
- [.] Focus loss while a button is held — `terminalService.ts:626`; the vitest case is a tautology; no walk.
- [x] Second press while one is held — `src/components/terminal/mouseButtons.ts:24-27`; test `mouseButtons.test.ts` "ignores a second aux press".
- [.] Mouse tracking on the primary screen — `mouseButtons.ts:11-13`, `terminalService.ts:744`, `:867`; helper test only; walk 3.3 open.
- [x] Primary screen ignores middle and right — `mouseButtons.ts:16`; tests `reportsMouse(false, 0)`, `auxButtonToForward(1, false)`.
- [.] Alternate screen without mouse tracking — `terminalService.ts:744`, `:867`; walk 3.4 open.
- [.] Scrolled-back primary screen does not forward — `mouseButtons.ts:12`; test `reportsMouse(true, 30)`; walk 3.5 open.
- [x] Mouse tracking state in the snapshot — `session.rs:556`; tests `mouse_tracking_tests.rs`.
- [x] A mode switch without a redraw still updates the canvas — `differ.rs:66-76`; tests `differ_mouse_tracking_tests.rs`.
- [x] The update names the field mouseTracking — `types.rs:76` with `rename_all`; test `emitter_mouse_tracking_tests.rs`.

### Harness entropy check

- Speculative abstractions? No. WHAT-comments? No; the `reportsMouse` comment states a WHY. New deps? No.
- Out of scope but noted: left clicks and the wheel still follow `isAltScreen` (`terminalService.ts:788`, `:816`, `:854`; `pty.rs:2213`). A primary-screen tracking app gets right and middle but no left click. The proposal declares this out of scope.

**Verdict:** the code is correct against wezterm-term's emission rule. Fix the false wezterm rationale and close walks 3.3-3.5 before archiving.

## Frontend

Scope: `src/components/terminal/{mouseButtons.ts,mouseButtons.test.ts,terminalService.ts}` and `src/lib/types.ts`.

### 🔴 Blocking
None. The aux path, release path and snapshot wiring are correct.

### Important

1. **No Shift bypass: Claude Code panes lose the context menu entirely.**
   - `src/components/terminal/terminalService.ts:744` forwards every right or middle press while `mouseTracking` is on, whatever the modifiers.
   - `src/components/terminal/terminalService.ts:866-868` calls `preventDefault` on every `contextmenu` event under the same condition.
   - Claude Code always enables tracking, so the webview menu is now unreachable in every Claude Code pane.
   - xterm, wezterm, kitty and iTerm2 let Shift bypass mouse reporting. wezterm's default `bypass_mouse_reporting_modifiers` is `SHIFT`.
   - Nergal has no bypass and no custom menu (`grep` finds no other `contextmenu` handler in `src`). `sendMouseButton` instead forwards `shift` to the app (`terminalService.ts:608`).
   - Verdict: add the bypass in this change, since the menu was reachable on the primary screen before.
   - Proposal:
     - Add a pure helper `bypassesMouseReporting(e: { shiftKey: boolean }): boolean` in `mouseButtons.ts`, with a unit test.
     - In `mousedown` (line 744), compute `const aux = e.shiftKey ? null : auxButtonToForward(...)`.
     - In `contextmenu` (line 867), call `preventDefault` only when `reportsMouse(...) && !e.shiftKey`.
     - This also fixes the keyboard-invoked menu. Shift+F10 fires `contextmenu` with `shiftKey` true and passes through. The Menu key fires it with Shift false, so it stays suppressed. Say so in the spec.
     - Add a scenario to the `terminal-wezterm` spec: "Shift bypasses mouse reporting" (no `terminal_mouse_button`, menu not suppressed).
     - Add a note to `docs/shortcuts.md` or `docs/patterns.md`.
   - If you keep the behaviour as is, state in the proposal that the menu loss is intended and that no bypass exists.
   - Honest caveat: the native menu on a canvas offers no copy or paste (Reload/Inspect at most), so real loss is small. The convention is still worth keeping.

2. **Release can pair with a press that was never sent under Shift.** Only relevant if item 1 is adopted.
   - `auxHeld` is set only when a press was forwarded, so a bypassed press sets no `auxHeld` and its release is not sent. This is correct.
   - Keep it that way: do not reorder the `auxPress` call before the Shift check.

### Nits

1. `src/components/terminal/terminalService.ts:1321` has wrong indentation (4 spaces instead of 2). It will fail the repo formatter or lint.
2. `src/components/terminal/terminalService.ts:854` and `:816`: left click, move and wheel still gate on `isAltScreen`.
   - On the primary screen with tracking on, right and middle reach the app but left does not.
   - The proposal marks this out of scope, but the walk (tasks 3.3-3.5) may expose the asymmetry. Mention it in the walk notes.
3. `src/components/terminal/mouseButtons.ts:10` comment is accurate WHY text. The `reports` parameter name at `:15` is vague.
   - Consider `mouseReported` or `reports` plus a doc line, so callers do not pass `isAltScreen` again.
4. `src/components/terminal/terminalService.ts:146-148` still says "CC pastes on release". It is accurate, so no change is needed.
   - Stale-comment sweep result: no comment in the scoped files still describes the aux path as alt-screen only. The only matches are the left-button comments at `:737` and `:818`, which stay correct.
5. `src/components/terminal/terminalService.ts:866-868` comment says "forwarded to the app". After item 1, add the WHY for the Shift exception.

### Checked and fine
- `mousedown` aux branch returns before the left-button path, so left selection stays untouched.
- `scrollOffset === 0` gating applies to press and context menu consistently.
- A scroll change or a tracking-off update while a button is held still sends the release through the window-level `mouseup`.
- `mouseTracking` is initialised to `false` in `showTerminal` (`:443`) and set in `applyUpdate` (`:1321`).
- `GridUpdate.mouseTracking` in `src/lib/types.ts:152` matches the spec.
- `docs/patterns.md` and `docs/design.md` have no terminal context-menu rule.

## Orchestrator ruling

- Every finding is fixed (user preference, feedback memory): Revision 1 in design.md, tasks group 3.

## Adversarial re-review (Revision 1)

**Verdict:** PASS-WITH-FOLLOWUPS. Code findings are fixed; one new spec clause is unimplemented; walks and gates stay open.

> Scope: `git diff mufdidev/fri-604-plan-approve-key -- src src-tauri docs/patterns.md` plus the four untracked `*_tests.rs`. Re-ran: `cargo test --lib mouse` 12 passed; vitest `mouseButtons.test.ts` 16 passed; `tsc --noEmit` clean; `cargo fmt --check` clean.

### 1. Prior findings

- Adversarial Nit (stale frontend `scrollOffset`): fixed by the backend press gate, `src-tauri/src/terminal/session.rs:418-425`.
- Adversarial Important (false wezterm rationale): fixed, `proposal.md:11` and `design.md` D1 state Nergal's own choice.
- Adversarial Important (walks 3.3-3.5): still open, now tasks 4.3-4.5, unchecked.
- Adversarial Important (release outside the canvas): fixed, `mouseButtons.test.ts:69`; wiring `terminalService.ts:150`.
- Adversarial Important (focus-loss tautology): fixed, `mouseButtons.test.ts:83`, `:87`; wiring `terminalService.ts:624`.
- Adversarial Important (`work-gates.sh` unrecorded): still open, task 4.2 unchecked, no run in `evidence.md`.
- Adversarial Important (Claude Code enables tracking): still open, walk 4.5 only.
- Adversarial Nit (Shift bypass): fixed, `mouseButtons.ts:338-340`, `terminalService.ts:742`, `:866`.
- Adversarial Nit (press-cell wording): fixed, `implementation.md:26`.
- Adversarial Nit (stale comments): fixed, `session.rs:403-408`; the `types.rs` Phase 2 sentence is gone.
- Adversarial Nit and Frontend Nit 1 (indentation): fixed, `terminalService.ts:1320`.
- Frontend Important 1 (Shift bypass): fixed, plus the spec scenario and `docs/patterns.md:602-608`.
- Frontend Important 2 (release under Shift): holds. A bypassed press never reaches `auxReduce`, `terminalService.ts:742-749`.
- Frontend Nit 2 (left asymmetry): noted in task 4.5 and design Risks.
- Frontend Nit 3 (parameter name): fixed, `reportsMouseEvents`, `mouseButtons.ts:342`.
- Frontend Nit 5 (Shift WHY comment): fixed, `terminalService.ts:862-864`.

### 2. Regressions

- Checked, clean: every former `auxPress`/`auxRelease` path now calls `auxReduce` (`terminalService.ts:150`, `:156`, `:624`, `:748`).
- Checked, clean: a release keys off `held`, not the mode. Shift, scroll or tracking-off mid-hold still sends it.
- Checked, clean: tracking off mid-hold, wezterm-term clears the button and writes nothing (`mouse.rs:195-199`).
- Checked, clean: press and `contextmenu` read the same `reportsMouse` inputs and the same `shiftKey`.
- Checked, clean: the backend gate spares every left path. They all require `isAltScreen` (`terminalService.ts:784`, `:810`, `:851`).
- Checked, clean: the gate exempts the alt screen (`session.rs:420`). The snapshot also clamps its offset to 0 there.
- Checked, clean: `Move` events pass the gate, which matches `PrimaryKind::Press` only.
- Checked, clean: a dropped press then a release is silent. wezterm-term skips releases with no held button (`mouse.rs:197`).
- Checked, clean: focus-loss releases go through `focus_changed`, not `TerminalSession::mouse_button`, so the gate cannot drop them.

### 3. Findings

- [Important] `openspec/changes/mouse-tracking-aux-buttons/specs/terminal-wezterm/spec.md:9` — The keyboard context-menu clause has no implementation. The listener sits on the canvas (`terminalService.ts:865`). A keyboard `contextmenu` fires at the focused textarea. That textarea is a sibling (`terminalService.ts:387`), so the event never reaches the canvas. The Menu key is not suppressed under tracking. No test or walk covers it. → Move the listener to `entry.container`, or drop the clause from the spec.
- [Important] `openspec/changes/mouse-tracking-aux-buttons/tasks.md:18-22` — Tasks 4.1-4.5 stay unchecked. `evidence.md` records no gate run after the press-only fix. Three scenarios still depend only on walks. → Run 4.1-4.2, record them, then walk 4.3-4.5.
- [Nit] `src/components/terminal/mouseButtons.ts:369` — A release after a scroll uses the pointer's viewport cell. That names a history row, against design Goals. → Use the press cell when `entry.scrollOffset > 0`.
- [Nit] `src-tauri/src/terminal/mouse_scroll_gate_tests.rs` — No test pins the dropped-press-then-release case. Claude Code acts on release. Safety rests on wezterm `mouse.rs:197`. → Add a test: scrolled press, then release, writes no bytes.
- [Nit] `docs/patterns.md:607` — "never reports" overstates it. A release of a button pressed at the bottom still reports. → Say "never reports a press".
- [Nit] `openspec/changes/mouse-tracking-aux-buttons/implementation.md:27` — It still names the removed `auxRelease`. → Say `auxReduce`.
- [Nit] `src-tauri/src/terminal/session.rs:392-402` — The wheel doc block still sits on `mouse_button`. `mouse_wheel` (`:459`) has no doc. This block was edited in Revision 1. → Move those lines above `mouse_wheel`.

### 4. Spec conformance (`specs/terminal-wezterm/spec.md`)

- [ ] Requirement clause "keyboard context menu" — no anchor, test or walk (see Important above).
- [x] Right click with SGR mouse mode — `session.rs:409`; test `session.rs:1004`.
- [x] Middle click with SGR mouse mode — same anchor and test.
- [x] Release outside the canvas — `mouseButtons.ts:367-371`; test `mouseButtons.test.ts:69`.
- [x] Focus loss while a button is held — `terminalService.ts:624`; tests `mouseButtons.test.ts:83`, `:87`.
- [x] Second press while one is held — `mouseButtons.ts:362`; test `mouseButtons.test.ts:58`.
- [.] Mouse tracking on the primary screen — `terminalService.ts:742`, `:866`; walk 4.3 open.
- [x] Primary screen ignores middle and right — test `mouseButtons.test.ts:14`, `:30`.
- [.] Alternate screen without mouse tracking — `terminalService.ts:742`, `:866`; walk 4.4 open.
- [x] Scrolled-back primary screen does not forward — test `mouseButtons.test.ts:34`; walk 4.5 pending.
- [x] Shift bypasses mouse reporting — test `mouseButtons.test.ts:42`; walk 4.5 pending.
- [x] The backend drops a report from a scrolled-back view — `session.rs:418-425`; test `mouse_scroll_gate_tests.rs:51`.
- [x] A release after scrolling still reaches the app — test `mouse_scroll_gate_tests.rs:96`.
- [x] Mouse tracking state in the snapshot — `session.rs:566`; tests `mouse_tracking_tests.rs`.
- [x] A mode switch without a redraw still updates the canvas — `differ.rs:66-76`; tests `differ_mouse_tracking_tests.rs`.
- [x] The update names the field mouseTracking — `types.rs:73`; test `emitter_mouse_tracking_tests.rs`.
