# REVIEW — clickup-sync

## Pre-build: iterative-plan-review (Claude evaluator, adversarial, codebase-verified)
2 rounds → **APPROVED**. R1: 18 findings [4 critical, 4 high] → R2: APPROVED with 2 medium + 2 low residuals, all fixed post-approval.

Round-1 criticals (all resolved): (C1) first-poll notification storm with no baseline → silent-first-sync gate (`clickup_sync_state.baseline_done`) + coalescing; (C2) absent-task semantics undefined → authoritative complete-Space-fetch tombstoning + un-tombstone-on-reappear; (C3) poll-scope ambiguity → resolved to all-tasks-per-Space, assigned-to-me as a local filter; (C4) FK insertion-ordering panic → ordered upsert in one transaction + placeholder-list synthesis. Highs: torn reads → fetch-all-then-commit atomic; pagination → `last_page` not row count; subtask tree → sole source `parent_id` from flat `parent`; rate-limit math corrected. Mediums/lows: sanitized markdown rendering (WebKitGTK XSS), lazy/gated thumbnails (SSRF), atomic `0600` token file (TOCTOU), custom-field defs derived from payloads, multi-team picker, token-leak guard, notification coalescing.

Round-2 residuals fixed post-approval: (1) un-assignment is NOT a tombstone case (task stays present with updated assignees, hidden by the local filter) — corrected in design/tasks/impl; (2) status caching vs freshness tension dissolved — statuses ride inline on the List objects in the hierarchy fetch, so no separate per-List call and no caching needed (rate-limit budget corrected down); (3) show-closed toggle does an on-demand `include_closed=true` fetch; (4) placeholder-list exempt from hierarchy-absent tombstoning.

## Post-build reviewers
_Placeholder — populated during Mode B execute (security escalation for auth/token phases; deps reviewer for the `keyring` addition)._
