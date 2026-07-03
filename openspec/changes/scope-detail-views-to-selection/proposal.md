## Why

Four frontend bugs share one root cause: a detail view for a list-selected entity applies async results without checking that the current selection still matches the request. Editing ClickUp task A then opening task B shows A's data under B's header; an old PR's diff lands under a new PR's header; a mid-write `busy` lock leaks onto the next task; and the ConflictsPanel auto-route fires for the wrong session. All four are selection-scoping failures of the same shape, fixable with one shared guard.

## What Changes

- **Introduce a shared `useStaleGuard(key)` hook** (generation counter): each async callback captures the current generation for its `key`; results are applied only if the generation is still current when the promise resolves. Lives in `src/hooks/`.
- **`src/components/clickup/ClickUpTaskView.tsx`** — guard every write-path detail refresh with the hook:
  - `confirmAfterWrite` (`ClickUpTaskView.tsx:429-441`) polls `clickup_task_detail` up to 5× over ~3.2s and calls `setDetail(updated)` unconditionally (line 436). Called from `handleStatusChange` (482), `handleChecklistToggle` (504), `handleDescSave` (525), and `handleDueDateSave` (546 — not in the original audit finding, same defect).
  - `handleRemoveAssignee` (560-562) and `handlePostComment`'s confirm loop (582-584) call `setDetail` directly with the same unguarded shape.
  - The load effect (367-406) already guards with a `cancelled` flag — the write paths never got the same treatment.
  - `busy` (`ClickUpTaskView.tsx:288`) is component state not keyed by `taskId`; the task-switch effect (367-378) resets detail/error/statuses/draft state but not `busy`, and every write handler gates on `if (!taskId || busy) return`. Navigating away mid-write leaves the next task's controls locked until the stale write resolves. Reset `busy` on task switch (stale completions must not clear a newer task's lock either).
- **`src/components/git/PrViewer.tsx`** — `fetchDiff` (274-287) writes `setLines`/`setHunks` unconditionally on resolve; `fetchChecks` (354-358) same for `setChecks`; the mount/param effect (364-380) re-runs on `[workspaceId, prNumber]` with no cancellation, and no call site mounts the viewer with a `key` (verified: `src/components/zen/ZenMode.tsx:176`, `src/components/git/chips/PrsChip.tsx:226`). Guard both fetches by `${workspaceId}:${prNumber}`.
- **`src/components/git/ConflictsPanel.tsx`** — `hadActivityRef` (`ConflictsPanel.tsx:259`) is `useRef(false)` with no `sessionId` keying or reset; the panel is mounted once with a changing `sessionId` prop (`src/components/git/chips/ConflictsChip.tsx:33`, `src/components/zen/ZenMode.tsx:158` — no `key={sessionId}`). The spec (`openspec/specs/conflict-resolution/spec.md:107-115`) requires `onResolved` "gated on prior activity", which is per-session in intent; today activity from session A can auto-route the user while session B is displayed. Reset the activity flag when `sessionId` changes.

## Capabilities

### New Capabilities

_None._ (`useStaleGuard` is an implementation detail of the affected panels, not a user-facing capability.)

### Modified Capabilities

- `clickup-task-panel`: detail-view async results (write confirms, comment/assignee refreshes) and the single-flight `busy` lock are scoped to the currently displayed task.
- `git-panel-v2`: PR viewer diff/checks results are applied only when they match the currently displayed `${workspaceId}:${prNumber}`.
- `conflict-resolution`: the `onResolved` "prior activity" gate is per-session — activity observed under one `sessionId` never routes a different session.

## Impact

- **`src/hooks/useStaleGuard.ts`** (new): generation-counter hook, ~30 LOC, no dependencies.
- **`src/components/clickup/ClickUpTaskView.tsx`**: `confirmAfterWrite` + 2 direct-refresh sites adopt the guard; `busy` reset added to the task-switch effect.
- **`src/components/git/PrViewer.tsx`**: `fetchDiff`/`fetchChecks` adopt the guard.
- **`src/components/git/ConflictsPanel.tsx`**: per-session reset of `hadActivityRef`.
- **Behavior-preserving otherwise**: no UI changes, no backend changes, no new dependencies.
- **Risk**: medium — the guarded paths are the ClickUp write flow (optimistic overlays + read-after-write polling) and PR diff rendering; a wrong guard key would suppress legitimate updates. Mitigated by keeping the existing state flow intact and only gating the final `set*` calls.
- **Out of scope**: remounting the views with `key={id}` (rejected in design — resets scroll/drill-in history), Linear panel equivalents (no confirmed instance of the bug), backend read-after-write lag itself.
