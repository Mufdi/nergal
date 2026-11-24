Changes after round 1:
- #1 (high) Rule is now mouseTracking && scrollOffset === 0 via pure reportsMouse(); requirement text + scenario "Scrolled-back primary screen does not forward"; vitest cases; implementation facts (mouseToCell viewport row, session.rs:438-446).
- #2 (medium) Proposal states the Claude Code behaviour change; walk 3.5 covers CC right/middle click (paste once, no menu) and the scrolled-back case.
- #3 (low) suppressContextMenu identity helper dropped; reportsMouse carries the real rule and gates both forwarding and the context menu.
- #4 (low) Emitter test asserting "mouseTracking":true in the channel JSON (task 1.1) + spec scenario.
- #5 (low) Differ tests h->l->h and invalidate(); implementation says where last_mouse_tracking updates.
- #6 (low) Synchronized-output delay noted in Per-phase risk.
- #7 (low) Walk 3.3 reordered (enable, click, Ctrl+C, disable, click).
- #8 (low) docs/shortcuts.md is never committed (standing rule).
