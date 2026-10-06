# Review: quake-always-mounted

## Adversarial (opus)

### Important
1. Stale `focusZoneAtom === "quake"` after a session switch routes raw leader keys to the hidden shell (`useKeyboardShortcuts.ts:80`). **Fixed**: closing resets the zone to `terminal`.
2. The `isActiveShell` skip dropped the quake re-focus on switch-back. **Fixed**: the skip still calls `focusActive("quake")` when the zone is `quake`.
3. The ResizeObserver now fits the active quake shell while closed, even from another session. **Accepted**, recorded in design.md Risks.
4. The WebKitGTK premise (pinned layer keeps a painted backing under `visibility: hidden`) is unverified. **Open**: manual walk tasks 3.2 and 3.3.
5. Doc drift (prevSidRef, `.quake-shown`, unchecked tasks). **Fixed**.

### Nit
1. Test name "slides out instead of snapping". **Fixed** (renamed); the one-frame jump is recorded in design.md.
2. Layout-effect comment overclaimed. **Fixed**.
3. Selector and specificity verified correct. No action.
4. Spec did not state the A closed → B open enter slide. **Fixed**.
5. Pinned layer cost not stated. **Fixed** (design.md Risks).

## Frontend
No new visual language: same classes, colors, shadow, height and timing. Only the animation mechanics changed.
