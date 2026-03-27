# Plan Annotations — Implementation Map (Revision 1: web-highlighter)

## Execution Order

```
1. Schema migration 004 (no dependencies)
2. DB method + command updates for new columns (depends on 1)
3. Install web-highlighter (no dependencies, parallel with 1-2)
4. Create highlighter.ts factory (depends on 3)
5. Delete wrapRangeInMarks.ts (depends on 4 being ready)
6. Update Annotation interface in stores/annotations.ts (no dependencies)
7. Rewrite AnnotatableMarkdownView.tsx (depends on 2, 4, 6)
8. CSS cleanup — hover rules (depends on 7)
9. Verify Jotai sync with new shape (depends on 6, 7)
10. Verify revise flow compatibility (depends on 7)
11. End-to-end verification (depends on all)
```

---

## Files to Create

### 1. `src-tauri/migrations/004_annotation_highlight_source.sql`

SQLite doesn't support DROP COLUMN cleanly. Use table rebuild:

```sql
CREATE TABLE annotations_new (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    type TEXT NOT NULL CHECK(type IN ('comment', 'replace', 'delete', 'insert')),
    target TEXT NOT NULL,
    content TEXT NOT NULL DEFAULT '',
    start_meta TEXT NOT NULL DEFAULT '{}',
    end_meta TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

INSERT INTO annotations_new (id, session_id, type, target, content, start_meta, end_meta, created_at)
SELECT id, session_id, type, target, content, '{}', '{}', created_at
FROM annotations;

DROP TABLE annotations;
ALTER TABLE annotations_new RENAME TO annotations;

CREATE INDEX IF NOT EXISTS idx_annotations_session ON annotations(session_id);
```

### 2. `src/lib/highlighter.ts`

Factory for web-highlighter instance. Follows plannotator's pattern.

**Key decisions**:
- `$root` = the container element (passed at creation time, not document-level)
- `wrapTag` = `'mark'` (matches existing CSS)
- Don't call `highlighter.run()` immediately — we control when to activate auto-highlight vs programmatic
- Export both the factory and event constants

**Approximate shape**:
```ts
import Highlighter from 'web-highlighter';

export function createHighlighter(container: HTMLElement): Highlighter {
  return new Highlighter({
    $root: container,
    wrapTag: 'mark',
    exceptSelectors: ['[data-annotation-toolbar]', 'button', 'input', 'textarea'],
    style: { className: 'annotation-highlight' },
  });
}

export { Highlighter };
export const HighlightEvent = Highlighter.event;
```

### 3. Delete `src/lib/wrapRangeInMarks.ts`

No longer needed — web-highlighter handles all wrapping.

---

## Files to Modify

### 4. `src-tauri/src/db.rs`

**Migration array** (~line 62): Add `include_str!("../migrations/004_annotation_highlight_source.sql")`.

**`AnnotationRow` struct**: Replace `position_start: i64` / `position_end: i64` with `start_meta: String` / `end_meta: String`.

**`save_annotation` method**: Replace `position_start`/`position_end` params with `start_meta: &str`/`end_meta: &str`. Update INSERT SQL.

**`get_annotations` method**: Update SELECT columns and row mapping.

**Pattern**: Same CRUD pattern, just different column names/types.

### 5. `src-tauri/src/commands.rs`

**`save_annotation` command**: Replace `position_start: i64, position_end: i64` with `start_meta: String, end_meta: String`.

**`get_annotations` command**: Return type uses updated `AnnotationRow` (automatic).

### 6. `src/stores/annotations.ts`

**`Annotation` interface**: Replace `position: { start: number; end: number }` with:
```ts
startMeta: Record<string, unknown>;
endMeta: Record<string, unknown>;
```

**`addAnnotationAtom`**: Update invoke call — serialize `startMeta`/`endMeta` to JSON strings:
```ts
invoke("save_annotation", {
  id, sessionId, annType: annotation.type,
  target: annotation.target, content: annotation.content,
  startMeta: JSON.stringify(annotation.startMeta),
  endMeta: JSON.stringify(annotation.endMeta),
}).catch(console.error);
```

**`serializeAnnotations()`**: No changes — only uses `type`, `target`, `content`.

### 7. `src/components/plan/AnnotatableMarkdownView.tsx` — FULL REWRITE of interaction logic

This is the main change. The rendering (Markdown components) stays identical. The interaction logic is replaced:

**Remove**:
- `wrapRangeInMarks` import
- Manual click handler for pinpoint
- Manual mouseup handler for selection
- Manual annotation restore with `findTextInContainer` + `surroundContents`
- `clearDomState()` body class manipulation
- `findTextInContainer()` function

**Add**:
- `createHighlighter` import
- Highlighter initialization in useEffect (create on mount, dispose on unmount)
- `CREATE` event handler → distinguish pinpoint vs selection, show toolbar
- `CLICK` event handler → select annotation in sidebar
- `mousemove` handler for hover → `resolvePinpointTarget()` + `data-pinpoint-hover`
- Click handler for pinpoint → create Range via TreeWalker, call `highlighter.fromRange(range)`
- Load from SQLite → `highlighter.fromStore()` for each annotation
- Escape → `highlighter.remove()` for pending, close toolbar

**Key pattern from plannotator**:
```ts
// Pinpoint click
function handlePinpointClick(element: HTMLElement) {
  const range = createTextRange(element);  // TreeWalker to find text nodes
  highlighter.fromRange(range);  // triggers CREATE event
}

// CREATE event
highlighter.on(HighlightEvent.CREATE, ({ sources }, type) => {
  if (type === 'from-input') {
    // User just selected/clicked — show toolbar
    const source = sources[0];
    setPendingHighlight(source);
    setToolbar({ ... });
  }
});
```

**`createTextRange(element)`** helper (from plannotator's usePinpoint):
```ts
function createTextRange(element: HTMLElement): Range {
  const range = document.createRange();
  const walker = document.createTreeWalker(element, NodeFilter.SHOW_TEXT);
  const firstNode = walker.nextNode();
  if (!firstNode) return range;
  range.setStart(firstNode, 0);
  let lastNode = firstNode;
  while (walker.nextNode()) lastNode = walker.currentNode;
  range.setEnd(lastNode, lastNode.textContent?.length ?? 0);
  return range;
}
```

**`resolvePinpointTarget(target)`** helper (from plannotator's usePinpoint):
```ts
function resolvePinpointTarget(target: EventTarget | null): HTMLElement | null {
  const el = target as HTMLElement;
  return el?.closest?.('[data-annotatable]') as HTMLElement | null;
}
```

### 8. `src/styles/globals.css`

**Remove**:
- `body.has-pinpoint` / `body.has-selection` rules (lines ~249-258)

**Replace hover rules**:
```css
/* Before: .annotatable-el:hover */
/* After: [data-pinpoint-hover] — JS-driven */
.annotatable-plan [data-pinpoint-hover] {
  outline: 1px dashed var(--primary);
  outline-offset: 2px;
}
.annotatable-plan [data-pinpoint-hover]::after {
  content: attr(data-annotatable);
  /* same tooltip styles as before */
}
```

**Keep**:
- `[data-pinpoint-active]` styles (yellow dashed) — for pinpoint click state
- `mark.annotation-*` type color styles — applied via `highlighter.addClass()`
- `mark.pending-selection` → rename to `.annotation-highlight` (web-highlighter's default class)

### 9. `src/components/plan/PlanAnnotationToolbar.tsx`

**Minor change**: The `targetRange` prop becomes optional or removed — web-highlighter manages position internally. The toolbar only needs `targetText`, `mode`, and the highlight `id` (to confirm or cancel).

### 10. `src/components/plan/PlanPanel.tsx`

**No changes needed** — `handleRevise()` uses `serializeAnnotations()` which only reads `type`, `target`, `content`. Shape change is transparent.

---

## Existing Patterns to Reuse

| Pattern | Source | Reuse in |
|---------|--------|----------|
| web-highlighter Highlighter instance | plannotator `useAnnotationHighlighter` | `highlighter.ts` factory |
| `createTextRange()` for pinpoint | plannotator `usePinpoint` | AnnotatableMarkdownView click handler |
| `resolvePinpointTarget()` | plannotator `usePinpoint` mousemove | AnnotatableMarkdownView hover handler |
| `highlighter.fromStore()` for restore | plannotator `applyAnnotationsInternal` | Load from SQLite effect |
| Migration table rebuild | SQLite pattern (no ALTER DROP COLUMN) | `004_annotation_highlight_source.sql` |

---

## Edge Cases

### 1. web-highlighter in WebKitGTK
Tauri uses WebKitGTK on Linux, not Chromium. web-highlighter uses standard DOM APIs (Range, Selection, TreeWalker) — should work. Verify during task 3.1. Fallback: fork and patch if needed.

### 2. Highlighter lifecycle on tab switch
When switching view→edit→view, the container DOM is destroyed and recreated. The highlighter instance must be disposed and recreated. Use `useEffect` cleanup.

### 3. HighlightSource serialization size
`startMeta`/`endMeta` are small JSON objects (~100 bytes each). SQLite TEXT column handles this fine.

### 4. Highlight ID coordination
web-highlighter generates IDs internally. We need our own IDs for SQLite. On CREATE event, store both: our `ann-{timestamp}` ID + web-highlighter's internal ID mapping.

### 5. Concurrent highlights
web-highlighter manages its own DOM state. Don't mix manual `<mark>` creation with highlighter — let it own all marks. This is why we delete `wrapRangeInMarks.ts`.
