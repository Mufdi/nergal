# REVIEW — terminal-cc-compat

## adversarial-reviewer (opus)

Verdict: no blocker. No deadlock: `focus_changed` only sends to wezterm's writer channel; lock order matches `terminal_mouse_button`. Spec matches code. `strip_paste_markers` covers both Rust paste paths and terminates.

### Important

1. **Aux-button release lost off the canvas** (`terminalService.ts` mouseup). A release outside the pane never reached wezterm, so CC saw a press without release. **Fixed**: release now rides a window-level `mouseup` listener, using the press cell as fallback (`terminalService.ts:660`). Spec scenario "Release outside the canvas" added.
2. **Focus flapping on auto-copy** (focus/blur listeners). The toast steals focus for a frame, which would send `CSI O`/`CSI I` on every copy. **Fixed**: blur is re-checked after 100 ms with `hasInputFocus` (`terminalService.ts:536`, `:634`). Spec scenario added.

### Nits

1. Aux mousedown did not focus the textarea. **Fixed**: `focusCanvas` is called first.
2. wezterm starts `focused: true`. **Fixed**: the real focus state is sent once after attach (`terminalService.ts:424`).
3. Drag motion is forwarded only with the left button held. Not changed (no CC need).
4. Context menu is suppressed on every alt-screen app, even without mouse mode. Not changed; matches spec.
5. DOM wiring has no automated test (vitest has no DOM env). Covered by manual checks 6.2–6.4.

## post-hoc iterative plan review (opus, orchestrator) — VERDICT: APPROVED

Ran after the build, at orchestrator request. Findings and resolution (follow-up commit):

1. **Important — aux release closures leak and can double-release.** Per-press window `mouseup` closures stacked and could send a stale release. wezterm's `focus_changed(false)` also synthesizes releases. **Fixed**: one `entry.auxHeld` slot with pure `auxPress` / `auxRelease` (`mouseButtons.ts`). One module-level window `mouseup`. The slot clears on window `blur` and on `sendFocus(false)`. Tests: `mouseButtons.test.ts` "aux held slot" (5 cases).
2. **Important — spec contradiction on kitty encoding.** **Fixed**: MODIFIED "Input encoding via wezterm" states the current truth. Legacy/CSI-u bytes, Ctrl+Backspace → Ctrl+W, Shift+Enter → LF, kitty flags ignored, FRI-503 owns the gap. Tests `kitty_flags_do_not_change_ctrl_enter_yet`, `modify_other_keys_distinguishes_ctrl_enter`.
3. **Important — FRI-503 grounding.** **Done**: Linear comment on FRI-503 with the modifyOtherKeys evidence and the option-B first step. Not implemented (user decision).
4. **Nit — C1 wording.** **Fixed** in the spec: "code point U+009B followed by 200~/201~ (UTF-8 `C2 9B`)".
5. **Nit — focus on tab switch / quake hide.** **Fixed**: `reportUnfocused` on a `showTerminal` switch, `hideAll` and `setHost(null)`. Manual check 1 gained a tab-switch step.
6. **Nit — destroyed entry in the settle timeout.** **Fixed**: guard `entries.get(entry.sessionId) === entry`.
7. **Nit — silent writer death; async command.** **Fixed**: `tracing::warn!` before returning a write error; test `shared_writer_adapter_propagates_write_errors`. `terminal_focus` stays sync: per-event commands (`terminal_input`, `terminal_mouse_button`, `terminal_scroll`) are all sync. Only `terminal_clipboard_write` is async, because arboard blocks on Wayland.
8. **Follow-ups created**: FRI-506 (forward by mouse tracking, not alt screen), FRI-507 (bracketed paste only with DECSET 2004).
