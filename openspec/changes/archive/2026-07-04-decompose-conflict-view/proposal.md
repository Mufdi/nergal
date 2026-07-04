## Why

`ConflictView` (`src/components/git/ConflictsPanel.tsx:358`) is a ~956-line component (to ~line 1314, in a 1614-line file) holding state, effects, keyboard handling, accept/reject region logic, and the full 3-pane JSX inline. The seams are already proven: `ConnectorStrip` (~1315) and `CodePane` (~1422) exist as extracted pieces below it. The monolith makes the actively-touched conflict UX (recent bug cycles) risky to edit and impossible to test in parts.

## What Changes

- **Extract a `useConflictResolution` hook** (same file or `src/hooks/`): the keyboard-nav effect, region navigation state, and accept/reject region logic move out of the component; `ConflictView` becomes composition + layout.
- **Recompose `ConflictView`** as `CodePane` ×3 + `ConnectorStrip` ×2 + toolbar, wired by the hook's returned state/handlers.
- **Structural only — zero behavior change**: same shortcuts, same region semantics, same auto-resolve flow. The three CodeMirror `EditorView`s stay **state** (not refs) — `ConflictsPanel.tsx:401-406` documents that `ConnectorStrip` can't observe ref mutations; the hook must preserve that (the audit finding said "refs"; on disk they are `useState` — handled accordingly).
- Care points: the rAF tick coupling between panes and connectors (~1305-1314), and the parent re-key behavior via `setSelectedMap` (~374).

## Capabilities

### New Capabilities

_None._

### Modified Capabilities

- `conflict-resolution`: the resolution UI is decomposed into a hook + pane composition (structural requirement; observable behavior unchanged).

## Impact

- **`src/components/git/ConflictsPanel.tsx`**: shrinks by the extracted hook (+ possibly `CodePane`/`ConnectorStrip` moving to sibling files if the file is still unwieldy — implementer's call, documented in PR).
- **New**: `useConflictResolution` hook file.
- **Risk**: MEDIUM — actively-touched surface, editor-state coupling (EditorView instances flow through the hook boundary); mitigations: extract in small steps with manual conflict-walk between each, no logic edits mixed in.
- **Sequencing**: the pending `scope-detail-views-to-selection` change edits this same file (per-session `hadActivityRef` reset in the outer panel, lines 259-269 — outside `ConflictView`); land that first, it's surgical.
- **Out of scope**: behavior changes, styling changes, the outer `ConflictsPanel` list/chip logic.
