# REVIEW — prune-session-scoped-state

## Reviewer: code-quality + correctness (single, sonnet) · 2026-07-04

**Verdict: PASS** — zero findings. tsc clean, 81 vitest.

- **Classification (the crux)**: reviewer independently traced ALL ~46 conversions +
  every excluded `Record<string,` atom to their REAL call sites (not the diff's
  comments). No wrongly-registered non-session map; no wrongly-excluded session map.
  Notable confirmations: `crossSessionUnreadMapAtom`'s `to_session` IS the sessionId key
  (backend `HashMap<sessionId,u32>`); `annotationMapAtom` uses `scope.sessionId` (bare);
  scratchpad's 4 atoms are tab_id-keyed (correctly untouched); the excluded git.ts
  caches are workspace/PR-keyed (spot-checked); `sessionToWorkspaceMapAtom` is derived
  (structurally ineligible).
- **conflict.ts composite prune**: `CONFLICT_KEY_SEP = fromCharCode(0)` identical to
  `conflictKey`'s; NUL boundary makes `session-1` vs `session-10` prefix-safe (tested).
  Holds file contents (ours/theirs/merged) — the highest-value leak, covered.
- **Immutability**: rest-spread drops exactly one key, new object, siblings by ref
  intact (tested).
- **onPrune**: correctly unused — grepped all 46 call sites; no registered atom's value
  holds an unlisten fn/timer. `terminalService.destroy` (before the prune) already tears
  down aux-shell PTYs/entries with their `unlisten()`, independent of the Jotai maps.
- **Wiring**: all 3 teardown sites call destroy THEN prune, synchronously; soft-close
  (`softCloseSessionAction`) never prunes (D2 — restore keeps state). Confirmed.
- **Type preservation**: tsc clean, no `any` leak beyond the registry's necessary
  internal erasure; callers keep `PrimitiveAtom<Record<string,T>>`.

## Scope note

Builder verified each atom by call site, catching 2 proposal mis-assumptions (scratchpad
tab_id, git.ts workspace caches) and adding an ad-hoc prune for the composite-key
conflict maps (file-contents leak) NOT in the proposal — in-scope expansion of the same
memory-reclaim goal. Non-blocking test-hygiene note (factory registers into the shared
registry from `it()` blocks) — harmless (distinct atom objects).

## Gates

- Gate 1-3: PASS (tsc clean, 81 vitest incl. 7 new prune tests).
- Gate 6 (scope): 22 files vs files_estimate 10 — the sweep touched more stores than
  estimated (each is a one-line factory swap); no logic risk, reviewer verified each.
