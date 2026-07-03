## 1. Sequencing

- [ ] 1.1 Confirm `scope-detail-views-to-selection` (same-file surgical edit at ConflictsPanel.tsx:259-269) has landed or coordinate order.

## 2. Hook extraction (one walk per step)

- [ ] 2.1 Create `src/hooks/useConflictResolution.ts`; move the keyboard-nav effect out of `ConflictView` (ConflictsPanel.tsx:358+); manual conflict walk.
- [ ] 2.2 Move region cursor state + accept/reject application into the hook (EditorViews passed in as null-safe args per design D2); walk.
- [ ] 2.3 Recompose `ConflictView` JSX as CodePane ×3 + ConnectorStrip ×2 + toolbar groups; walk.
- [ ] 2.4 (Optional, if file still > ~800 lines) move `CodePane` (~1422) and `ConnectorStrip` (~1315) to `src/components/git/conflict/`; update imports.

## 3. Tests

- [ ] 3.1 If the frontend test runner exists by then (pending `critical-path-test-coverage`), add headless hook tests for region navigation + choice transitions; otherwise record the manual walk checklist in the PR.

## 4. Verification

- [ ] 4.1 `npx tsc --noEmit`
- [ ] 4.2 Manual: full conflict walk — open a conflicted session, navigate regions by keyboard, accept ours/theirs/both, verify connectors track scrolling, complete the merge; repeat in Zen mode.
