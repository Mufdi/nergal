> Checkboxes marked retroactively on 2026-06-05 (post-archive audit): shipped with one rename — the queue command landed as `set_pending_annotations` (consumed by inject-edits) and annotations persist via `save/get/delete/clear_spec_annotations`. Tasks 4.4/4.5 were never implemented (queued feedback is fire-and-forget with a toast; no pending indicator/discard shipped) and stay unchecked.

## 1. State and store refactor

- [x] 1.1 Extend `annotationsAtom` in `src/stores/annotations.ts` to key annotations by a composite `targetId` string (`plan:<sessionId>` | `spec:<relativePath>`)
- [x] 1.2 Add selector atoms `annotationsForTargetAtom(targetId)` and `annotationCountForTargetAtom(targetId)`
- [x] 1.3 Update existing plan-annotations call sites to pass `plan:<sessionId>` as target (no behavior change)
- [x] 1.4 Add new atom `specAnnotationModeAtom: { [specPath: string]: boolean }` for per-spec mode toggle
- [x] 1.5 Extend `serializeAnnotations()` to accept `targetId` and include a context header with the spec path when target is a spec

## 2. SpecViewer refactor to read-only default

- [x] 2.1 Refactor `src/components/spec/SpecViewer.tsx` to render `AnnotatableMarkdownView` with `interactive={false}` as default
- [x] 2.2 Add toolbar with buttons: "Annotate" (primary), "Edit directly" (secondary), "Send to Claude" (visible only when annotations exist)
- [x] 2.3 Wire "Annotate" button to toggle `specAnnotationModeAtom[currentSpecPath]`
- [x] 2.4 When annotation mode is on, pass `interactive={true}` and `targetId={"spec:" + relativePath}` to `AnnotatableMarkdownView`
- [x] 2.5 Preserve `scrollTop` across toggles to avoid losing position on re-render
- [x] 2.6 Move existing direct edit code path behind "Edit directly" button and show inline warning banner

## 3. Keyboard shortcut

- [x] 3.1 Audit `src/stores/shortcuts.ts` for collisions with the proposed `a` shortcut in the spec panel context
- [x] 3.2 Register context-scoped shortcut `toggleSpecAnnotationMode` active only when `SpecViewer` has focus
- [x] 3.3 Ensure shortcut also exits annotation mode on second press
- [x] 3.4 Handle Escape: clear active interactions without exiting annotation mode when interactions exist; exit mode otherwise

## 4. Send-to-Claude pipeline

- [x] 4.1 Add Tauri command `queue_spec_annotations(spec_path: String, serialized: String)` in `src-tauri/src/commands.rs` that writes to `HookState.pending_annotations`
- [x] 4.2 Verify `inject-edits` hook in `src-tauri/src/hooks/` already consumes `pending_annotations` — no change needed; add test coverage for spec-originated payloads
- [x] 4.3 Implement "Send to Claude" button handler: call `serializeAnnotations(targetId)`, invoke `queue_spec_annotations`, clear annotations for that target, show success toast
- [ ] 4.4 Add `pendingSpecAnnotationsCountAtom` reading from backend state (polled or event-driven) and render indicator in `SpecViewer` header while > 0
- [ ] 4.5 Add "Discard" action next to the pending indicator that calls a new `clear_pending_annotations` command

## 5. Integration and verification

- [x] 5.1 Manual test: open a spec, confirm read-only default, no editor visible
- [x] 5.2 Manual test: toggle annotate mode via button and shortcut, add Comment/Replace/Delete/Insert annotations
- [x] 5.3 Manual test: switch between two specs, confirm annotations are isolated per target
- [x] 5.4 Manual test: simultaneously have plan annotations pending and spec annotations, confirm no cross-contamination
- [x] 5.5 Manual test: click "Send to Claude", verify toast, verify pending indicator appears, send next prompt in terminal, verify Claude receives the serialized feedback _(verified except the pending-indicator clause — no indicator shipped, see 4.4)_
- [x] 5.6 Manual test: click "Edit directly", verify warning banner, verify edits still persist to disk
- [x] 5.7 Run `cd src-tauri && cargo clippy -- -D warnings && cargo test && cargo fmt --check`
- [x] 5.8 Run `npx tsc --noEmit` and `pnpm build` to confirm no regressions
- [x] 5.9 Update `openspec/specs/plan-annotations/spec.md` cross-reference if applicable, or add link from `spec-annotations` spec to `plan-annotations` spec
