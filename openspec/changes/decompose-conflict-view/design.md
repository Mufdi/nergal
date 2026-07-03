## Context

`ConflictView` (`ConflictsPanel.tsx:358-~1314`) owns: three CodeMirror `EditorView`s held as **state** (404-406 — state, not refs, because `ConnectorStrip` re-renders off them; comment at 401), region parsing/choice logic (module-level helpers at 68-70, 192, 212 — already outside the component), keyboard navigation, accept/reject application, scroll-sync via rAF tick, and the 3-pane + 2-connector JSX. `CodePane` (~1422) and `ConnectorStrip` (~1315) are already extracted, proving the render seam; the logic seam is what's missing.

## Goals / Non-Goals

**Goals:**
- `ConflictView` readable as composition; conflict logic callable/testable without mounting CodeMirror.
- Zero observable behavior change (shortcuts, region choices, auto-resolve, Zen interplay).

**Non-Goals:**
- New features, styling, or shortcut changes; touching the outer panel's list/chip/auto-resolve code (owned by the pending `scope-detail-views-to-selection` change).

## Decisions

### D1: one hook (`useConflictResolution`), not several

The keyboard nav, region cursor, and accept/reject logic are one cohesive state machine (nav selects a region; accept mutates the merged doc and advances). Splitting into `useRegionNav` + `useRegionApply` would force the shared region-list state up into the component again. One hook returns `{ regions, activeRegion, navigate, applyChoice, … }`.

**Alternatives considered:**
- *Multiple micro-hooks*: cleaner names, but shared state lifts back to the component — recreates the problem. Rejected.
- *Reducer + context*: appropriate if grandchildren needed dispatch; here only `ConflictView` consumes it. Overkill. Rejected.
- *Move logic to module functions only*: the pure parts already are (`parseRegions` etc. at 192/212); what remains is stateful/effectful — needs a hook. Rejected as complete answer.

### D2: `EditorView`s stay state, owned by the component, passed into the hook

The hook receives `{ ours, merged, theirs }: EditorView | null` as arguments rather than owning creation — pane creation is `CodePane`'s job, and `ConnectorStrip` needs re-renders when views mount (401-406 constraint). The hook treats them as capabilities (scroll-to-region, doc mutation), null-safe before mount.

### D3: extraction in review-sized steps

(1) keyboard-nav effect → hook; walk. (2) region cursor + applyChoice → hook; walk. (3) JSX recomposition (pane/connector layout groups); walk. Each step compiles and passes the manual conflict walk before the next. If after extraction the file still exceeds ~800 lines, move `CodePane`/`ConnectorStrip` to `src/components/git/conflict/` siblings (mechanical, optional step 4).

## Risks / Trade-offs

- [rAF tick + scroll-sync coupling breaks subtly] → the tick stays where it lives today (ConnectorStrip side, ~1305-1314); the hook doesn't touch it.
- [Keyboard shortcuts double-fire or die during the move] → keyboard tier rules in `docs/patterns.md` §keyboard; verify against `shortcuts.ts` collisions; manual walk per step (D3).
- [Actively-touched file → merge conflicts] → sequence after `scope-detail-views-to-selection` (surgical, same file); do the extraction in a quiet window.

## Open Questions

- None blocking. Whether `useConflictResolution` lands in `src/hooks/` or file-local is a naming/placement call at implementation (default: `src/hooks/useConflictResolution.ts`).
