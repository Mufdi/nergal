# REVIEW — scope-detail-views-to-selection

## Reviewer: code-quality + correctness (single, sonnet) · 2026-07-04

**Verdict: FAIL → FIXED.** All fixes verified: tsc clean, 74 vitest.

### What the reviewer validated as solid (traced each)

- D1 contract (capture at request / check at apply) correctly implemented + used in all
  6 ClickUp write handlers + both PrViewer fetches; no effect-scheduling hazard (the
  key-change bump flushes before any network `.then()` resolves).
- D2 busy: stale-aware `finally { if (fresh()) setBusy(null) }` — a stale completion
  can't clear a newer task's lock; no stuck-busy path. `setBusy(null)` on task switch.
- D4 key = `prAnnotationsKey(workspaceId, prNumber)`, byte-identical to the diff-cache key.
- `clearOverlayEntry` stale-branch is id-keyed → safe. `makeStaleChecker` unit tests
  prove capture-before-bump=fresh / post-bump=stale / independence.

### Findings → all fixed by orchestrator

1. **MEDIUM (real race, in the orchestrator-added handleVerifyComment gate)**: only
   `setDetail` was gated; `setUncertainComment(null)` + `setCommentDraft("")` ran
   unconditionally after the first await, and both are controller-level (not task-keyed)
   — a task A→B switch mid-verify wiped B's draft/banner. → Fixed: `if (!fresh()) return`
   immediately after the first await, before any `set*` (the standard pattern the other
   handlers use).
2. **LOW (convention)**: handleVerifyComment used live `taskId` in both invokes instead
   of snapshotting like every sibling. → Fixed: `const id = taskId` at entry, used in both.
3. **LOW (edge case)**: ConflictsPanel's auto-resolve effect lacked `sessionId` in deps,
   so a session switch to a numerically-coinciding conflict count could skip the
   mark-activity branch and never route a genuine resolution. → Fixed: added `sessionId`
   to the dep array (unread in the body; forces re-eval per switch).
4. **NIT**: `useRef(makeStaleChecker())` allocated per render. → Fixed: lazy `??=` init.

## Scope note

`handleVerifyComment` was NOT in the proposal's enumerated sites — the builder flagged it
as the same unguarded-setDetail shape; the orchestrator gated it in-change (root-cause
discipline), and the reviewer's Medium caught that the first pass gated it incompletely.
Now fully gated. Linear panel confirmed clean (not touched).

## Gates

- Gate 1-3: PASS post-fix (tsc clean, 74 vitest incl. 3 makeStaleChecker units).
- Gate 6 (scope): 3 components + 1 new hook (+test); handleVerifyComment expansion stayed
  in-file.
