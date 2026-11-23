Linear: FRI-604

## Why

Approve or Revise in the plan panel can end the review in the UI while Claude Code keeps waiting. In the walk of 2026-11-23 the user approved a plan with the `A` key and with the mouse. The panel left the review state, but the hook never got a decision and resolved to a timeout deny after 30 minutes (XD001 transcript, `toolu_01Fo3C5j56H69mMkKQD9EXik`, 20:57:34Z → 21:27:35Z).

Two `plan:ready` emitters feed the panel. The `PreToolUse[ExitPlanMode]` one carries no decision path, and the `PlanReview` one does. Both set the review state. When the review state comes without a decision path, Approve resets the panel and sends nothing. When the decision path points at a FIFO that no longer exists, the write creates a regular file and reports success. Both failures are silent.

## What Changes

- A `plan:ready` that carries a decision path opens a review. A preview without one belongs to the same cycle when its content matches the stored plan: it keeps the decision path and review state. A preview with new content starts a new cycle with no decision path and the review state `idle`.
- `submit_plan_decision` accepts only the decision path registered for the session by its `PlanReview` event, and writes only to a live FIFO, with a bounded non-blocking write. Anything else returns an error and writes nothing. The command no longer runs on the main thread.
- The plan panel never ends a review silently. A missing decision path or a failed delivery shows an error toast that points to the terminal, and the review state goes `idle`. A failed Revise keeps the annotations.

## Capabilities

### New Capabilities

### Modified Capabilities
- `cc-adapter`: adds a requirement that a plan decision reaches the waiting plan-review hook or fails visibly.

## Impact

- `src-tauri/src/commands/plans.rs` (`submit_plan_decision`), `src-tauri/src/agents/claude_code/adapter.rs` (`submit_plan_decision`), a shared FIFO write helper, the adapter's `pending_plan_fifos` registry.
- `src/stores/hooks.ts` (`plan:ready` listener), `src/stores/plan.ts`, `src/components/plan/PlanPanel.tsx`.
- No config, schema or hook-protocol change. Windows (frozen) keeps its pipe path.
