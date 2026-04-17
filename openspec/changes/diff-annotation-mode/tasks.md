## 1. Store and data model

- [ ] 1.1 Create `src/stores/diffAnnotations.ts` with `diffAnnotationsAtom: Record<string, DiffAnnotation[]>` keyed by `diff:<filePath>`
- [ ] 1.2 Define `DiffAnnotation` type `{ id, filePath, side, startLine, endLine, kind, body, suggestionReplacement?, createdAt }`
- [ ] 1.3 Add selectors: `diffAnnotationsForFileAtom(filePath)`, `diffAnnotationsTotalCountAtom`, `diffAnnotationsGroupedByFileAtom`
- [ ] 1.4 Add `addDiffAnnotationAtom`, `removeDiffAnnotationAtom`, `clearDiffAnnotationsAtom` write-only atoms
- [ ] 1.5 Implement `serializeDiffReview()` helper: groups annotations by file, orders by startLine, produces structured markdown matching the design doc format

## 2. Diff panel refactor to read-only default

- [ ] 2.1 Locate the current diff panel component (likely under `src/components/` — check `git` or `file-browser` modules)
- [ ] 2.2 Refactor to render read-only by default; gate existing edit path behind an "Edit directly" toolbar button with inline warning banner
- [ ] 2.3 Add toolbar: "Annotate" toggle button, "Edit directly" button, "Submit review" button (enabled when batch > 0), "Discard review" button
- [ ] 2.4 Introduce `diffAnnotationModeAtom: boolean` for the panel-wide mode toggle

## 3. Line focus and keyboard navigation

- [ ] 3.1 Add `focusedLineAtom: { filePath, side, line } | null` and `selectionRangeAtom: { start, end } | null` to track active focus and range
- [ ] 3.2 When annotation mode activates, set focus to first visible diff line
- [ ] 3.3 Implement context-scoped key handlers in the diff panel (active only when annotation mode on and panel focused):
  - `j`/`ArrowDown` → next line
  - `k`/`ArrowUp` → prev line
  - `Shift+j`/`Shift+k` → extend selection
  - `c` → open comment input
  - `s` → open suggestion input
  - `Ctrl+Enter` → submit review batch
  - `Escape` → cancel input / clear selection / exit mode (cascade)
- [ ] 3.4 Ensure focused line visually highlighted (lateral border) and `scrollIntoView({ block: 'nearest' })` on every focus change
- [ ] 3.5 Render range indicator "lines N-M selected" when range > 1

## 4. Comment and suggestion inputs

- [ ] 4.1 Build `DiffCommentInput` component anchored below the focused line/range with a textarea and Submit/Cancel buttons
- [ ] 4.2 Build `DiffSuggestionInput` component with description textarea + code replacement textarea + inline preview showing original vs proposed lines
- [ ] 4.3 On submit (Enter or button), call `addDiffAnnotationAtom` and close input; keep focus on original line
- [ ] 4.4 On Escape, close input without committing

## 5. Mouse interaction

- [ ] 5.1 Make gutter line numbers clickable: click → set focus + open comment input
- [ ] 5.2 Support `Shift+click` on a second line to extend the range from current focus
- [ ] 5.3 Add hover state on gutter showing cursor pointer + subtle outline

## 6. Annotation display on gutter

- [ ] 6.1 Render marker icon on gutter for every line that has at least one annotation
- [ ] 6.2 Click on marker shows a popover with the annotation body, type, and a "Remove" action calling `removeDiffAnnotationAtom`
- [ ] 6.3 For multi-line annotations, render markers on all lines in the range (or on the first line with a range indicator)

## 7. Review batch and submission pipeline

- [ ] 7.1 Render pending count in panel footer: `${count} pending comments` when > 0
- [ ] 7.2 Wire "Submit review" button: call `serializeDiffReview()`, invoke new Tauri command `queue_diff_review(serialized: String)` that writes to `HookState.pending_annotations` with source marker `diff-review`, then clear batch and show toast
- [ ] 7.3 Add Tauri command `queue_diff_review` in `src-tauri/src/commands.rs`
- [ ] 7.4 Verify `inject-edits` hook appends `pending_annotations` to the next user prompt regardless of source marker (test with a `diff-review` payload)
- [ ] 7.5 Wire "Discard review" button: call `clearDiffAnnotationsAtom`

## 8. Large diff virtualization

- [ ] 8.1 Identify existing virtualization library in the project (check `package.json`); if none, add `@tanstack/react-virtual`
- [ ] 8.2 Wrap diff line list in a virtualizer with overscan 10 for files > 500 lines
- [ ] 8.3 Ensure focused line scroll-into-view still works across virtualized boundaries

## 9. Shortcuts registration

- [ ] 9.1 Audit `src/stores/shortcuts.ts` for collisions with proposed `j`, `k`, `c`, `s`, `Ctrl+Enter` in the diff annotation context
- [ ] 9.2 Register shortcuts as context-scoped (active only when `diffAnnotationMode && diffPanelFocused`)
- [ ] 9.3 Add panel-level shortcut `toggleDiffAnnotationMode` (propose `a`; confirm no collision with spec-annotation-mode shortcut scope)

## 10. Integration and verification

- [ ] 10.1 Manual test: open a diff, confirm read-only default, no editor visible
- [ ] 10.2 Manual test: toggle annotation mode, navigate with `j`/`k`, confirm focus border and scroll
- [ ] 10.3 Manual test: extend selection with `Shift+j`, confirm range highlight and indicator
- [ ] 10.4 Manual test: add a comment with `c`, submit with Enter, confirm gutter marker appears
- [ ] 10.5 Manual test: add a suggestion with `s`, write replacement code, submit, confirm marker
- [ ] 10.6 Manual test: add 3+ annotations across 2 files, click "Submit review", verify `pending_annotations` populated, send next prompt in terminal, confirm Claude receives the structured review grouped by file
- [ ] 10.7 Manual test: click gutter line number with mouse, confirm parity with `c` shortcut
- [ ] 10.8 Manual test: open diff with >500 lines, verify virtualization and keyboard nav remain smooth
- [ ] 10.9 Manual test: "Discard review" clears batch without sending
- [ ] 10.10 Manual test: "Edit directly" warning banner appears, direct edits still work
- [ ] 10.11 Run `cd src-tauri && cargo clippy -- -D warnings && cargo test && cargo fmt --check`
- [ ] 10.12 Run `npx tsc --noEmit` and `pnpm build` to confirm no regressions
