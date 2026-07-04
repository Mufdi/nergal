# REVIEW — decompose-conflict-view

## Reviewer: code-quality + zero-behavior-change (single, sonnet) · 2026-07-04

**Verdict: PASS** — zero behavior-change findings. tsc clean, 94 vitest.

- **Callback reordering (the flagged risk) SAFE**: traced the dep arrays of the 6 moved
  callbacks (updateMerged/resetMerged/askClaude/saveResolution/toggleSyncScroll/
  handleAcceptAll) against everything between old and new position — none depend on
  `regions`/`sideMapping`/highlight+connector memos/`scrollNonce`. All stay unconditional
  `useCallback`s called every render → hook-call sequence unchanged, no stale closure, no
  use-before-def.
- **Extracted logic verbatim**: the keyboard-nav effect matches `git show HEAD:`
  line-for-line — same `event.code` values, picker j/k/Enter/Esc, Ctrl+←/→, region
  j/k/o/t/b/s, Ctrl+Shift+O/T/Z/Enter, the `nergal:resolve-conflict-active-tab` listener,
  capture=true. `clampRegionIndex`/`wrapPickerIndex` = drop-in equivalents of the inline
  arithmetic (same output every branch).
- **D2 judgment accepted**: the hook has ZERO EditorView references (only a comment); the
  3 views stay `useState` in the component (not refs — D2's real intent). The builder
  correctly did NOT thread unused view params through the hook.
- **JSX/composition byte-identical**: the diff has no hunk past the extraction point → the
  entire return JSX (3× CodePane, 2× ConnectorStrip, toolbars, picker overlay) + the
  ConnectorStrip/CodePane bodies are unchanged.
- **rAF + scrollNonce**: moved verbatim (same body, `[focusedRegion]` dep); scroll-target
  memos + ignoreUntil effect untouched, now reading off the hook return.
- **13 tests real** (parseRegions boundaries, applyChoice resulting text, marker detection,
  clamp/wrap) — not tautological; effectful parts walk-only (no renderer in repo).

## Orchestrator vet

ConflictsPanel.tsx 1626→1461 lines; the 2.4 optional CodePane/ConnectorStrip file-move was
skipped (still > 800 lines) — recorded as a follow-up candidate, not a finding.

## Gates

- Gate 1-3: PASS (tsc clean, 94 vitest incl. 13 new).
- Gate 6 (scope): 1 file shrunk + 1 hook (+test); structural, no behavior change.
- Task 4.2 (manual conflict walk: keyboard nav, accept ours/theirs/both, connector
  scroll-track, merge complete, Zen) — pending, inexecutable here.
