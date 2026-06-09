# REVIEW — clickup-task-integration

## Pre-build: iterative-plan-review (Claude evaluator, adversarial, codebase-verified)
2 rounds → **APPROVED**. R1: 10 findings [4 high] (the evaluator read the real pty.rs and caught two anchors used wrong) → R2: APPROVED with residual cleanups, applied post-approval.

Round-1 highs (all resolved): (#1) send-as-prompt was modeled on `reinject_pinned_note`/`write_session_data` (raw write, no bracketed paste, no submit) which fragments multi-line content → switched to `terminal_paste` (`pty.rs:994`, bracketed `\x1b[200~`…`\x1b[201~`) + explicit `\r` submit; (#2) the mid-stream defer guard had no mechanism in any anchor → grounded on `SessionStatus` (`models.rs:8`): `Running` queues, `→ Idle` delivers (no dependency on the unimplemented cross-session-messaging mode-map); (#3) third-party ClickUp comments injected with vault-note trust → untrusted-data fence framing + explicit confirm-before-submit for the auto-submitting send-as-prompt; (#4) incoherent lock guidance ("release lock before composing" can't read the mirror) → compose inside the `db.lock()` closure per the `pty.rs:396-410` vault precedent. Mediums/lows: attrition order across all sections (not just comments), migration N+1-at-merge, spawn-worktree binds by default, bracketed-paste + defer unit tests.

Round-2 residuals fixed post-approval: scrubbed 3 stale `reinject_pinned_note` references (proposal ×2 + implementation execution-order #4) that contradicted Decision 5; specified the deferred-send flush wiring (drained by the status-transition site on `Running→Idle`, logged on enqueue + drain); documented queued-send-not-persisted and the attach residual injection risk as accepted.

## Post-build reviewers
_Placeholder — populated during Mode B execute (reviewer escalation for the spawn hot-path edit + live-PTY write + untrusted-content confirm)._
