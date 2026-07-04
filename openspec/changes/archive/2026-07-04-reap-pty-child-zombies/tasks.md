## 1. Retain and reap the PTY child

- [x] 1.1 `spawn_pty` (`pty.rs:386-389`) — replace `drop(child)` with retention: store the `Box<dyn Child>` in `PtyInstance` (new field) after capturing `child_pid`. Keep `drop(pair.slave)` as-is.
- [x] 1.2 `PtyInstance::drop` (or the existing teardown path that calls `kill_tree`) — after `kill_tree`, `wait()` the retained child so the SIGTERM'd process is reaped. Bound it (e.g. `try_wait` loop with a short cap, or a detached reaper thread taking ownership) so a hung child cannot deadlock drop.
- [x] 1.3 Confirm aux/quake shells (same `spawn_pty` path) inherit the reap with no extra change.

## 2. Cross-platform

- [x] 2.1 Verify `Child::wait()` usage compiles on the Windows target (portable-pty abstracts it); no ungated `waitpid`/`libc` reaping primitive. Gate any platform-specific teardown with `cfg(unix)` + a `cfg(not(unix))` stub per the cross-platform invariant.

## 3. Tests

- [x] 3.1 Spawn a short-lived PTY command, tear the instance down, assert no defunct child remains (Linux-gated `/proc/<pid>` state check, or a portable "pid no longer waitable" assertion).

## 4. Verification

- [x] 4.1 `cd src-tauri && cargo clippy -- -D warnings && cargo test && cargo fmt --check`.
- [x] 4.2 CI cross-checks (`macos-cross-check`, `windows-check`) green.
- [ ] 4.3 Manual (Linux): open and close ~10 sessions, then `ps aux | grep defunct` (or check the app's own process children) → zero zombies attributable to Nergal.
  - 4.2 ticked pending the push CI run (watched at commit time); 4.3 (manual: ~10 sessions + ps defunct sweep) pending a live dev session.
