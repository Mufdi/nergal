# REVIEW — cross-session-delivery-confirmation

## Reviewer: concurrency + code-quality + spec lens (single, sonnet) · 2026-07-03

**Verdict: PASS** — zero blocking findings.

- **SettleGuard fires on_settled EXACTLY once**: `fire(mut self)` + `Option::take` →
  subsequent Drop sees None (no-op); Drop-only path (panic/bail) fires `false` once.
  Guard created only AFTER paste success + inside the spawned task, so a paste-Err
  never creates it and the bare boxed `FnOnce` is dropped without invoking (drop of a
  boxed FnOnce does not call it). Narrow teardown gap (task dropped before first poll →
  callback dropped without the false fallback) is moot (in-memory marker dies with the
  process).
- **In-flight marker**: all 6 drain_idle exits traced — success/failure callbacks clear
  first line, sync paste-Err/empty-pending/DB-lock-fail clear explicitly, in-flight-skip
  correctly does NOT clear (never inserted). `clear_in_flight` is the callback's first
  statement (before DB work). No leak, no double-clear.
- **No Mutex across await**: IN_FLIGHT locked only in sync helpers; SharedDb locked only
  in the callback (post-sleep-await).
- **emit_handle** `'static` (boxed trait object, owned cloned AppHandle) — no dangling
  borrow.
- **NoopDelivery Ok→Err rewrite honest**: verified old test asserted `pending.is_empty()`
  (the buggy consume); new asserts `len()==1` (correct: headless can't confirm a submit).
  Doc nit: design D4's HEADING says "calls on_settled(false)" but its body + tasks.md say
  keep the Err return — implementation matches body/tasks (the operative text).
- **worktree_sessions** 2 no-op callbacks preserve fire-and-forget (out of scope, no
  behavior change).

## Finding (Low, test-quality) — FIXED by orchestrator

`paste_failure_never_invokes_callback_and_leaves_unconsumed` asserted the retry returns
0 to "prove" the marker was cleared — but both "cleared→retry→fail→0" and
"leaked→skip→0" produce 0, so it didn't discriminate. Orchestrator fix: added a
`wake_calls` counter (incremented on `wake_idle` ENTRY, before the fail check) to
MockDelivery and asserted `wake_calls() == 2` on the retry, proving it reached the paste
logic rather than the in-flight skip path. Test re-run green.

## Gates

- Gate 1-3: PASS (clippy -D warnings clean, 772 tests incl. 4 new + 1 reworked, fmt
  clean).
- Gate 6 (scope): 3 files (delivery + messaging test + worktree_sessions call-site
  adapt) vs files_estimate 3.
