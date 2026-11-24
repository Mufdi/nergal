Changes after round 2:
- #1 (medium) Registry claim with DashMap::remove_if before writing; only the claimer writes; never undone. Panel ignores e.repeat and has an in-flight ref guard. Scenarios: two concurrent submits write one decision; a held key sends one decision. Tasks 1.1 (claim tests incl. two threads), 1.3, 2.3.
- #2 (medium) applyPlanReady compares with `original`; same-cycle keeps stored content and mode. Spec text + scenario with an edit; task 2.1.
- #3 (medium) Deadline scenario + test (reader that never reads, 200 ms). Deadline/EPIPE after partial write documented in Risks (hook waits until terminal answer or 30-min timeout; toast points there).
- #4 (low) Adapter calls the helper inside spawn_blocking (design D2, implementation step 3, task 1.4).
- #5 (low) Mutex guard drop before .await noted in Per-phase risk.
- #6 (low) warn! when PlanReview resolves no plan; Risks line.
- #7 (low) PlanReview registers a FIFO only when its parent is ipc_dir(); residual risk documented.
