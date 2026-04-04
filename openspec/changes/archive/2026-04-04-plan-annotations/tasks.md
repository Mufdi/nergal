## 1. Backend — SQLite persistence (done)

- [x] 1.1 Create migration `003_annotations.sql` with `annotations` table and index on session_id.
- [x] 1.2 Add `AnnotationRow` struct and Tauri commands: `save_annotation`, `get_annotations`, `delete_annotation`, `clear_annotations`. Wire in lib.rs invoke handler.
- [x] 1.3 Extend `HookState` with `pending_annotations: Option<String>` field + `set_pending_annotations()` / `take_pending_annotations()`.
- [x] 1.4 Extend `inject_edits()` to check both `pending_edit` and `pending_annotations` in a single read (race-safe).
- [x] 1.5 Add Tauri command `set_pending_annotations` with empty-string guard.

## 2. Backend — Schema revision for HighlightSource

- [x] 2.1 Create migration `004_annotation_highlight_source.sql`: add columns `start_meta TEXT DEFAULT '{}'` and `end_meta TEXT DEFAULT '{}'` to `annotations` table. Drop columns `position_start` and `position_end` (SQLite requires table rebuild for column drops — use CREATE new + INSERT SELECT + DROP old + ALTER RENAME pattern).
- [x] 2.2 Update `AnnotationRow` struct: replace `position_start: i64` / `position_end: i64` with `start_meta: String` / `end_meta: String`. Update `get_annotations` query to select new columns.
- [x] 2.3 Update `save_annotation` db method and Tauri command: replace position params with `start_meta: String` / `end_meta: String`.

## 3. Frontend — Install and initialize web-highlighter

- [x] 3.1 Install `web-highlighter` package via pnpm. Verify it works in Tauri/WebKitGTK environment (no Chrome-specific APIs).
- [x] 3.2 Create `src/lib/highlighter.ts` — singleton factory that creates and configures a Highlighter instance. Config: `$root` = plan container, `wrapTag` = 'mark', `style.className` = 'annotation-highlight'. Export `createHighlighter(container: HTMLElement): Highlighter`.
- [x] 3.3 Delete `src/lib/wrapRangeInMarks.ts` — replaced by web-highlighter.

## 4. Frontend — Rewrite AnnotatableMarkdownView with web-highlighter

- [x] (revised) 4.1 Rewrite `AnnotatableMarkdownView.tsx`: initialize highlighter via `createHighlighter(containerRef)` in useEffect. Wire `Highlighter.event.CREATE` to show toolbar with HighlightSource data. Dispose on unmount.
- [x] (revised) 4.2 Pinpoint mode: on click of `[data-annotatable]` element, create a Range via `createTextRange()`, call `highlighter.fromRange(range)` → triggers CREATE event → toolbar appears.
- [x] (revised) 4.3 Text selection: `highlighter.run()` auto-highlights on selection → CREATE event → toolbar. On close without action, `highlighter.remove(id)`.
- [x] (revised) 4.4 Hover: JS `mousemove` listener + `resolvePinpointTarget()` → `data-pinpoint-hover` attribute. Disabled when toolbar open.
- [x] (revised) 4.5 Annotation restore: load from SQLite → `highlighter.fromStore(startMeta, endMeta, target, id)` + `highlighter.addClass(typeClass, id)`.
- [x] (revised) 4.6 Escape handler: remove pending highlight, close toolbar, clear hover.

## 5. Frontend — Update Jotai store for HighlightSource shape

- [x] (revised) 5.1 Update `Annotation` interface: `startMeta: DomMeta` / `endMeta: DomMeta` replacing `position`.
- [x] (revised) 5.2 Update `addAnnotationAtom`: serialize `startMeta`/`endMeta` as JSON strings for `save_annotation`.
- [x] (revised) 5.3 Update load from SQLite: parse `start_meta`/`end_meta` JSON strings back to objects for `highlighter.fromStore()`.

## 6. Frontend — CSS cleanup

- [x] (revised) 6.1 Remove `body.has-pinpoint` / `body.has-selection` CSS rules (no longer needed).
- [x] (revised) 6.2 Replace `.annotatable-el:hover` CSS rules with `[data-pinpoint-hover]` attribute selector.
- [x] (revised) 6.3 Keep annotation mark type colors — applied via `highlighter.addClass()`.

## 7. Revise flow (done, verify compatibility)

- [x] 7.1 `handleRevise()` in PlanPanel.tsx uses `set_pending_annotations` + `reject_plan`. No changes needed.
- [x] 7.2 `serializeAnnotations()` uses `target` field which is unchanged — compatible.

## 8. Polish + edge cases

- [x] 8.1 Stale annotations toast: implemented, uses `content` + `annotations.length` deps — compatible.
- [x] 8.2 Escape: `closeToolbar()` removes pending highlight via `highlighter.remove()`, clears hover attr.
- [x] 8.3 Click outside: click handler checks `toolbarOpenRef` → closes toolbar → removes pending highlight.
- [x] 8.4 Tab switch: highlighter disposed on content change via useEffect cleanup, recreated on mount.
