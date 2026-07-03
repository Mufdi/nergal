## Context

Detail views in Nergal follow a list → selection → async-detail pattern: ClickUpTaskView fetches `clickup_task_detail`, PrViewer fetches `get_pr_diff`/`get_pr_checks`, ConflictsPanel watches per-session conflict state. All three are long-lived components that receive a changing selection prop (`taskId`, `prNumber`, `sessionId`) instead of being remounted per selection. Async completions therefore race with selection changes. The load effect in ClickUpTaskView (`ClickUpTaskView.tsx:379-405`) already handles this with a `cancelled` cleanup flag; the write-confirm paths, PrViewer fetches, and the ConflictsPanel activity ref never got the same treatment.

## Goals / Non-Goals

**Goals:**
- One reusable, mechanical idiom for "apply this async result only if the selection hasn't changed".
- Fix the four confirmed instances (ClickUp detail writes, ClickUp `busy` lock, PR diff/checks, ConflictsPanel activity gate).
- Keep the existing SWR/optimistic-overlay flow intact — only gate the final state application.

**Non-Goals:**
- Remounting strategies or list/detail architecture changes.
- Cancelling in-flight backend work (the invoke still completes; we only ignore its result).
- Sweeping every async `set*` in the codebase — only the detail-view-for-selection shape.

## Decisions

### D1: shared `useStaleGuard(key)` hook with a generation counter

```ts
// src/hooks/useStaleGuard.ts
function useStaleGuard(key: unknown): () => boolean {
  const gen = useRef(0);
  useEffect(() => { gen.current++; }, [key]);
  // callback captures the generation at call time; returns "still current?"
  const capture = useCallback(() => {
    const captured = gen.current;
    return () => captured === gen.current;
  }, []);
  return capture;
}
```
Usage: `const capture = useStaleGuard(taskId);` then in a handler `const fresh = capture(); … if (fresh()) setDetail(updated);`. (Exact API shape may be tuned at implementation; the contract is: capture at request time, check at apply time, keyed by the selection.)

**Alternatives considered:**
- *Per-component `cancelled` flags (extend the existing pattern):* works for effects (cleanup runs on dep change) but not for event handlers — `confirmAfterWrite` is called from click handlers, which have no cleanup lifecycle. Would need ad-hoc refs per handler in each component; that is exactly the duplication that produced the bug four times.
- *`key={id}` remount at call sites:* simplest correctness story, but destroys per-selection UI state deliberately kept across switches — ClickUpTaskView's drill-in history (`detailHistory`, `ClickUpTaskView.tsx:293`), scroll positions, and PrViewer's cached-file selection restore (`PrViewer.tsx:333-339`). Also does not fix ConflictsPanel (activity must survive re-render but reset per session, not per mount).
- *AbortController threaded into `invoke`:* Tauri `invoke` has no abort semantics; simulating it still requires the same "am I stale" check at resolve time. Extra machinery, same guard.

### D2: `busy` reset on task switch, and stale writes must not clear a fresh lock

Add `setBusy(null)` to the task-switch effect (`ClickUpTaskView.tsx:367-378`). The `finally { setBusy(null) }` blocks in the write handlers must also become stale-aware (only clear if still on the same task), otherwise a stale write completing late would unlock a *newer* task's in-flight write. Both sides use the same captured-generation check.

### D3: ConflictsPanel — reset the ref, don't lift the state

Reset `hadActivityRef.current = false` when `sessionId` changes (small effect keyed on `sessionId`, placed before the auto-resolve effect at `ConflictsPanel.tsx:260-269`). **Alternative considered:** keying activity in a per-session Jotai map (like `selectedConflictFileMapAtom`) — heavier, and the spec's intent is "did *this panel view of this session* see activity", which a ref + reset captures; a durable per-session map would wrongly remember activity from a view the user long left.

### D4: PrViewer guard key is `${workspaceId}:${prNumber}`

Matches the existing diff-cache key (`PrViewer.tsx:279`). Guarding only on `prNumber` would break when two workspaces show the same PR number.

## Risks / Trade-offs

- [Stale-guard suppresses a legitimate update — e.g. guard keyed too narrowly] → key by exactly the identity the view renders (`taskId`, `${workspaceId}:${prNumber}`, `sessionId`); add a regression test per instance if a frontend runner lands (see pending `critical-path-test-coverage` change).
- [`confirmAfterWrite` stops polling early on task switch, leaving the optimistic overlay entry set] → the overlay map is keyed by task id (`setOverlayEntry(setOverlay, id, field, …)`), so a stale overlay entry is invisible until the user returns to that task; clear it in the stale branch anyway (call `clearOverlayEntry` with the captured id — id-keyed, safe when stale).
- [Hook API misuse (capture at apply time instead of request time) reintroduces the bug silently] → document the two-step contract in the hook's doc comment; review usage in the four call sites during review.

## Open Questions

- None blocking. Linear's panel was audited as not exhibiting the bug; if implementation finds the same shape there, file a follow-up change rather than widening scope.
