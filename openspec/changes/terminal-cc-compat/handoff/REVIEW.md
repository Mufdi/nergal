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
