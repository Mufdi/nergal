import { useCallback, useEffect, useRef } from "react";

/// Pure generation-counter core, extracted so the invalidation logic is
/// testable without a React renderer. `bump()` invalidates every checker
/// captured so far; `capture()` snapshots the current generation and
/// returns a checker that reports whether it is still current.
export function makeStaleChecker() {
  let gen = 0;
  return {
    bump(): void {
      gen++;
    },
    capture(): () => boolean {
      const captured = gen;
      return () => captured === gen;
    },
  };
}

/// Guards an async result against a selection that changed while the async
/// work was in flight (e.g. a task-detail poll resolving after the user
/// opened a different task). Two-step contract, mirroring the existing
/// `cancelled`-flag idiom but usable from event handlers, not just effects:
///   1. REQUEST time — call `capture()` before starting the async work,
///      before the `await`. Keep the returned `fresh` checker.
///   2. APPLY time — call `fresh()` right before every `set*` call that
///      applies the result. If it returns false, the selection moved on;
///      discard the result instead of applying it.
/// Bumping (invalidating all outstanding captures) happens whenever `key`
/// changes, so callers key this by whatever identifies the current
/// selection (`taskId`, `${workspaceId}:${prNumber}`, `sessionId`, ...).
export function useStaleGuard(key: unknown): () => () => boolean {
  // Lazy init so a fresh checker isn't allocated-and-discarded on every render
  // (useRef evaluates its arg each time, keeping only the first result).
  const checkerRef = useRef<ReturnType<typeof makeStaleChecker> | null>(null);
  checkerRef.current ??= makeStaleChecker();
  const checker = checkerRef.current;

  useEffect(() => {
    checker.bump();
  }, [key, checker]);

  return useCallback(() => checker.capture(), [checker]);
}
