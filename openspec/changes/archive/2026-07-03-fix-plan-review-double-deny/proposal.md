## Why

The plan-review hang-prevention safety net writes **two concatenated JSON objects to stdout** on exactly the paths designed to be the fallback, corrupting the mechanism.

`blocking_fifo_read_liveness_aware` (`hooks/cli.rs`, Unix) and its Windows twin `blocking_pipe_read_liveness_aware` call `output_deny(...)` themselves on the wall-clock-backstop branch (`cli.rs:348`) and the dead-GUI branch (`cli.rs:378`), then return a sentinel string (`{"approved":false,"message":"wall_clock_backstop"}`). The caller `plan_review()` (`cli.rs:249-266`) unconditionally re-parses that returned string and, since `approved=false`, calls `output_deny("wall_clock_backstop")` **a second time**.

Result: on the human-never-responds / GUI-died paths — the ones built to keep the agent from hanging forever — Claude Code's hook receives two `hookSpecificOutput` JSON objects with no separator. The hook JSON parser will reject that, defeating the safe-deny and re-introducing the hang the safety net exists to prevent. The normal decision path (real FIFO write) is unaffected — it returns a decision JSON that `plan_review()` writes exactly once.

## What Changes

- **Make the liveness-aware readers return-only, never write.** Remove the internal `output_deny(...)` calls at `cli.rs:348` (wall-clock backstop) and `cli.rs:378` (dead-GUI), and the analogous calls in the Windows `blocking_pipe_read_liveness_aware`. The readers return the sentinel JSON (or the real decision); `plan_review()` performs the single stdout write it already does for every path.
- **Preserve the deny semantics.** The sentinel already carries `approved:false` + a message; `plan_review()`'s existing `else` branch maps that to one `output_deny(message)`. The user-facing deny message ("Plan review timed out — please resubmit" / GUI-gone text) moves into the sentinel's `message` field so the single write still surfaces it verbatim.
- **Keep the warn-level tracing** (`ipc_event = "dead_peer_deny"`) in the readers — that is logging, not stdout, and is correct where it is.

## Capabilities

### Modified Capabilities

- `plan-panel-multi-agent`: clarifies that the plan-review hook emits **exactly one** `hookSpecificOutput` decision to stdout on every resolution path, including the wall-clock-backstop and dead-GUI safety-net paths. (The single-writer invariant was intended but violated on the fallback paths.)

## Impact

- **`src-tauri/src/hooks/cli.rs`**: remove the two internal `output_deny` calls in `blocking_fifo_read_liveness_aware`; same in the Windows twin; carry the deny message via the sentinel; `plan_review()` unchanged (already the single writer).
- **Risk**: HIGH-value fix, LOW blast radius — the change deletes redundant writes on error paths; the happy path is untouched.
- **Tests**: unit test asserting the wall-clock and dead-GUI sentinels each produce exactly one stdout JSON object when routed through `plan_review()`'s decision-mapping (extract the sentinel→output mapping so it is unit-testable without a real FIFO/timer).
- **Out of scope**: the broader hook-server test gap (separate Band-D finding) — this change adds only the regression test for the double-write.
