## Context

`PtyManager.session_ptys: Mutex<HashMap<session_id, pty_id>>` maps sessions to their PTYs; `kill_session_pty` (`pty.rs:1195`) resolves through it, so an entry clobbered by a double insert leaks a process for the app's lifetime. `start_claude_session` (`pty.rs:514`) is invoked from the frontend on session open — double invocation happens in practice via rapid tab switching / deep-link + UI races. `spawn_aux_shell` (`pty.rs:808`) solved the identical race with check-and-reserve (861-867) + rollback (882-885).

## Goals / Non-Goals

**Goals:**
- At most one PTY per `session_id`, guaranteed by the map, under any interleaving.
- Preserve the existing external contract (`StartClaudeResult { pty_id }`, idempotent early-return).

**Non-Goals:**
- Reworking the ready-wait/timeout mechanics (565-568); aux-shell paths; reaping already-orphaned PTYs (pending `reap-pty-child-zombies` change).

## Decisions

### D1: check-and-reserve under one lock hold (copy `spawn_aux_shell`)

Move the `pty_id` computation (536-543) above the lock block, then:

```rust
{
    let mut session_ptys = state.session_ptys.lock().map_err(|e| e.to_string())?;
    if let Some(existing_id) = session_ptys.get(&session_id) {
        return Ok(StartClaudeResult { pty_id: existing_id.clone() });
    }
    session_ptys.insert(session_id.clone(), pty_id.clone());
}
```

`spawn_pty` runs after the reservation; on `Err`, remove the reservation and propagate.

**Alternatives considered:**
- *Hold the lock across `spawn_pty`*: eliminates the reservation concept, but `spawn_pty` does process spawn + thread setup — holding a std `Mutex` across it blocks every other PTY operation (writes, resizes, kills go through the same manager) for the spawn duration, and risks deadlock if `spawn_pty` ever touches `session_ptys`. Rejected; also not what the proven sibling does.
- *Per-session async lock (e.g. `DashMap<session_id, tokio::Mutex>`)*: lets the second caller await full readiness instead of getting a booting PTY's id. More machinery and a new dependency for a marginal benefit the current early-return contract doesn't promise anyway. Rejected.
- *Debounce at the frontend call site*: doesn't close the race (two windows, deep-links, retries), and the backend must be safe regardless of caller discipline. Rejected as primary fix.

### D2: reservation value is the final `pty_id` (no sentinel)

`spawn_aux_shell` reserves with the real `pty_id` and returns `term_id` for an existing key; here the early-return already returns the mapped id. A sentinel ("reserving") value would force every reader of `session_ptys` to understand a new state; reserving the real id keeps the map's invariant "value = the session's pty id" (readers touching a booting PTY get buffered/no-op behavior from the pty layer, same as today's post-insert window).

### D3: cross-platform neutrality (CLAUDE.md invariant)

The reserve/rollback restructure uses only `std::sync::Mutex` + `HashMap` — no new `cfg(unix)`/`cfg(windows)` seam is introduced. `spawn_pty` is portable-pty-backed (ConPTY on Windows) and untouched; the file's existing gated seams (`pty.rs:182` `#[cfg(unix)]`, `pty.rs:194` `#[cfg(windows)]`) are outside the edited region and stay as-is. The `macos-cross-check` and `windows-check` CI gates verify the invariant holds on the PR.

## Risks / Trade-offs

- [Second caller receives a `pty_id` whose PTY hasn't emitted ready yet] → identical to today's window between insert (558) and ready-wait completion for the first caller; frontend already tolerates it (it subscribes to output events keyed by session).
- [Rollback path races a concurrent third call that saw the reservation and early-returned a doomed id] → the third caller's session simply has no PTY after rollback — same visible outcome as today's plain spawn failure; the next `start_claude_session` retry re-reserves cleanly.
- [Behavioral drift from the sibling pattern] → tasks mandate copying `spawn_aux_shell`'s block shape verbatim (comment included, adapted).

## Open Questions

- None blocking.
