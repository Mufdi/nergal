## Why

Every PTY-spawned process leaks as a zombie for the life of the Nergal process. In `spawn_pty` (`pty.rs:386-389`):

```
let child = pair.slave.spawn_command(cmd)...;
let child_pid = child.process_id();
drop(child);
```

`portable-pty` 0.9's `Box<dyn Child>` wraps `std::process::Child` on Unix, whose `Drop` does **not** `wait()`. No `.wait()`/`.try_wait()` on this child exists anywhere. So every agent-session shell and every aux/quake shell (same `spawn_pty` path) becomes a defunct `<defunct>` process when it exits — via `exit`, a crash, or `kill_tree`'s SIGTERM — and lingers until Nergal itself exits.

Nergal is a long-running desktop app whose core workflow is spawning many worktree/agent sessions over hours or days. The zombie count grows monotonically with sessions ever opened and can exhaust the process's pid budget. It also compounds the double-spawn orphan case (a separate Band-C finding): an orphaned PTY that is never killed is also never reaped.

## What Changes

- **Reap each PTY child after capturing its pid.** After `let child_pid = child.process_id();`, instead of `drop(child)`, hand the `child` to a lightweight reaper that `wait()`s it — either:
  - retain the `Child` handle in `PtyInstance` and `wait()` it in `PtyInstance::drop` **after** `kill_tree` runs (so the SIGTERM'd process is reaped), **or**
  - spawn a detached reaper thread that owns the `Child` and blocks on `child.wait()`, returning when the process exits.
- **Chosen direction (see design):** retain the handle and reap on instance drop, so reaping is tied to the session lifecycle and no extra thread-per-session is created. The reaper must run after `kill_tree` to avoid a wait-before-signal hang.
- **Preserve `child_pid` capture** — `kill_tree` and port/pid introspection still use the pid exactly as today; only the `Child`'s disposal changes.

## Capabilities

### Modified Capabilities

- `session-directory`: adds the requirement that a PTY-backed session reaps its child process on teardown, leaving no defunct process after the session exits or is killed. (The session lifecycle already owns spawn + kill; this closes the missing reap.)

## Impact

- **`src-tauri/src/pty.rs`**: `spawn_pty` retains the `Child` (in `PtyInstance` or a reaper) instead of `drop(child)`; `PtyInstance::drop` (or the reaper) `wait()`s it after `kill_tree`. Aux/quake shells inherit the fix (same path).
- **Cross-platform**: `Child::wait()` is portable (`portable-pty` abstracts it); on Windows the handle is closed/awaited equivalently — verify the `#[cfg]` shape at implementation time; no Unix-only reaping primitive (`waitpid`) is introduced ungated.
- **Tests**: a test that spawns a short-lived PTY command, triggers teardown, and asserts the child pid is no longer a zombie (platform-appropriate check; may be a Linux-gated `/proc/<pid>` state assertion).
- **Risk**: MED — silent cumulative leak today; the fix must order reap-after-kill to avoid blocking teardown on a still-running child. Wrap the `wait()` so a hung child cannot deadlock instance drop (bounded `try_wait` loop or a detached reaper).
- **Out of scope**: the double-spawn orphan race (`start_claude_session` check-then-act) — a separate Band-C change; this fix reaps whatever children the instance owns, and once double-spawn is fixed there are no unowned children left to leak.
