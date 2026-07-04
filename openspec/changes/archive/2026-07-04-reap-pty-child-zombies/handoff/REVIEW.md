# REVIEW — reap-pty-child-zombies

## Reviewer: code-quality-reviewer (single-sequential, sonnet) · 2026-07-03

### Round 1: **FAIL — 1 BLOCKER + 2 MINOR**

**BLOCKER (real, confirmed by orchestrator):** the bounded reap (~2s cap per instance)
ran inside critical sections: `kill_session_aux_shells` dropped instances in-loop with
the `session_ptys`+`instances`+`trackers` guards held (`instances` is the global Mutex
every session's terminal_input/write/resize/paste/scroll takes → one SIGTERM-ignoring
child freezes ALL sessions' terminal I/O up to N×2s); `kill_aux_shell`/`kill_session_pty`
same single-instance pattern; `shutdown_all` cleared the map under lock synchronously on
the `CloseRequested` main event-loop thread (GNOME-visible hang risk).

Also confirmed non-issues via portable-pty 0.9 source: reader fd is a real dup()
(field-drop order can't starve a write-blocked child); detached fallback thread is
bounded 1-per-hung-child; `try_wait Err` ≈ ECHILD, give-up correct.

### Fix (builder, round 2)

Extract-then-drop at all sites: `kill_session_aux_shells` collects removed instances
into a `Vec` inside a scoped block (guards die at block exit) then drops it lock-free;
`kill_aux_shell`/`kill_session_pty` same shape; `shutdown_all` `mem::take`s the map
under the lock and drops it on a detached thread. **Plus a 5th defensive site the
builder found**: `spawn_pty`'s `...lock()?.insert(pty_id, instance);` discarded the
returned `Option<PtyInstance>` as an unbound temporary, which drops BEFORE the guard
temporary (reverse creation order) — i.e. under the lock; now bound + dropped after.

### Round 2 (focused re-verify): **PASS** — all sites confirmed lock-free; reviewer
traced the temporary-drop semantics of the insert fix independently. 1 new MINOR:
the deferred-drop thread also deferred the SIGTERM, a theoretical BUG-06 regression
(a shell could miss its signal if the process exits before the thread's first slice).

### Orchestrator post-PASS fix

Restored the synchronous-signal guarantee: `shutdown_all` now iterates the taken map
and `kill_tree`s every pid synchronously (sub-ms syscalls) before handing the map to
the detached reap thread; the re-signal inside each `PtyInstance::drop` is a no-op
(ESRCH) for already-dead trees. pty.rs + lib.rs comments updated to match.

## Gates

- Gate 1-3: PASS post-fix (clippy clean, 737 tests incl. `reap_child_leaves_no_zombie`,
  fmt clean, tsc unaffected).
- Gate 6 (scope): 2 files (pty.rs + lib.rs comment) vs files_estimate 1 — the lib.rs
  line is comment-only; no escalation.
