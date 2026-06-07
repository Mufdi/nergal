# REVIEW — quake-terminal (2026-06-07)

Single-reviewer sequential (code-quality), per `.work-modules.json` gating.

## Reviewer: code-quality

VERDICT: PASS

Trace evidence: center/agent path of the `showTerminal` refactor traced line-by-line against the old `show()` — behavior-identical. Lock ordering audit (`session_ptys → instances → writer`) found no cycle. Migrations additive + gated. No `unwrap()`/`expect()` outside tests.

Findings → disposition (fixes applied post-review, re-verified):

- 🟡 Stale quake host after last-tab soft-close + restore (`QuakeTerminal.tsx` setHost deps `[open]` miss the render-null path) → **FIXED**: deps `[open, activeSessionId]`.
- 🟡 `e.key === "}"` branch could false-positive on layouts where AltGr reports as Ctrl+Alt → **FIXED**: `!e.altKey` guard added (keeps the Linux AltGr case, where AltGr never sets `ctrlKey`).
- 🟡 Tab inside a quake shell fell to the unreliable bubble route (capture-phase fix only matched the terminal zone) → **FIXED**: `sendSpecialKeyToActive` takes a region; quake zone routed through it.
- 🟡 Closed env-shell tabs resurrected on session-object churn (`spawnEnvShells` treated `[]` as never-seeded) → **FIXED**: presence-of-key tombstone.
- 🟡 `spawn_aux_shell` check-then-act TOCTOU could orphan a PTY on concurrent invokes → **FIXED**: key reserved under the same `session_ptys` lock as the check, rolled back on spawn error.
- 🟢 `list_aux_shells` has no frontend caller → kept (explicit in tasks.md 1.2 contract; trivial surface).
- 🟢 `dropShellEntry` doc contradicted the shell:exited contract → **FIXED**: comment aligned.
- 🟢 `startResize` listeners leak on mid-drag unmount until next mouseup (self-removing) → accepted.
- 🟢 `BracketRight` physical-position tradeoff on non-US layouts → documented, deliberate (`event.code` convention).
- 🟢 `destroy` leaves stale `quakeShellsAtom` keys for deleted sessions → accepted (per-run growth, same pattern as other session maps; clearing from terminalService would create a circular import).
