## 1. Shared stale guard

- [ ] 1.1 Create `src/hooks/useStaleGuard.ts`: generation counter keyed on the passed selection key; API = capture at request time, check at apply time (see design.md D1). Doc comment states the two-step contract explicitly.

## 2. ClickUpTaskView

- [ ] 2.1 `src/components/clickup/ClickUpTaskView.tsx` — adopt the guard in `confirmAfterWrite` (429-441): capture on entry, gate `setDetail(updated)` (436) and the final `clearOverlayEntry` behavior per design D1/Risks (overlay entry is id-keyed — clear with the captured id even when stale).
- [ ] 2.2 Gate the direct refreshes in `handleRemoveAssignee` (560-562) and `handlePostComment`'s confirm loop (582-587) the same way.
- [ ] 2.3 `busy` lifecycle (288): add `setBusy(null)` to the task-switch reset effect (367-378); make each handler's `finally { setBusy(null) }` stale-aware so a stale completion cannot clear a newer task's lock (design D2). Covers `handleStatusChange`, `handleChecklistToggle`, `handleDescSave`, `handleDueDateSave`, `handleRemoveAssignee`, `handlePostComment`.

## 3. PrViewer

- [ ] 3.1 `src/components/git/PrViewer.tsx` — guard `fetchDiff` (274-287): gate `setLines`/`setHunks`/`setError`/`setLoading` on `${workspaceId}:${prNumber}` still current; keep the cache write (280) unguarded (cache is per-PR keyed).
- [ ] 3.2 Guard `fetchChecks` (354-358) with the same key.

## 4. ConflictsPanel

- [ ] 4.1 `src/components/git/ConflictsPanel.tsx` — reset `hadActivityRef.current` when `sessionId` changes (small effect ahead of the auto-resolve effect at 260-269).

## 5. Verification

- [ ] 5.1 `npx tsc --noEmit`
- [ ] 5.2 Manual: in ClickUp panel, change task A's status and immediately open task B → B shows B's data, B's controls usable; back on A the write landed. In PRs chip, select a large-diff PR then quickly a small one → no diff/header mismatch. Conflicts chip: resolve conflicts in session A, switch to conflict-free session B → no auto-route on B.
