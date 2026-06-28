# Tasks — windows-proc

Depends on `windows-compile` (the `#[cfg(unix)]` gating of the kill callers this change un-gates).

## 1. Windows kill path (raw TerminateProcess — iprev #3)

- [x] 1.1 `platform_proc.rs` — `#[cfg(windows)]` `terminate(pid)` helper: `OpenProcess(PROCESS_TERMINATE, false, pid)` → `TerminateProcess(h, 1)` → `CloseHandle(h)` (windows crate `Win32::System::Threading` + `Win32::Foundation`); guard `pid > 4`. `kill_tree(root)`: reversed-BFS `descendants()` → `terminate` each + root. NOT `sysinfo` `Process::kill()` (shells to `taskkill.exe`). No `kill(-pgid)` analog. Added `#[cfg(not(any(unix, windows)))]` no-op stub (invariant: catch-all for a hypothetical third target).
- [x] 1.2 `platform_proc.rs` — `#[cfg(windows)] kill_pid(pid)`: same raw open+terminate; map open/terminate failure → `io::Error::other`, system pid → error. `windows` crate dep (Win32_System_Threading + Win32_Foundation) present from #2 (windows-ipc, in main). Added `#[cfg(not(any(unix, windows)))]` error stub.

## 2. Un-gate the kill callers

- [x] 2.1 `pty.rs` — the `#[cfg(unix)]` `Drop` teardown calling `kill_tree(pid)`: fully un-gated (no unix-only adjacent step in this block — the `setsid` concern lives elsewhere). `kill_tree` now resolves on all targets via the platform arms + catch-all stub.
- [x] 2.2 `browser.rs` `kill_port` — un-gated the `port_owner` + `kill_pid(owner.pid)` free-port block (now resolves on Windows; Docker container fallback unchanged below it).

## 3. Confirm the already-cross-platform paths on Windows

- [ ] 3.1 Confirm via the Windows gate + a `cargo test` on `windows-latest` that `listening_ports()` / `port_owner()` (listeners `GetExtendedTcpTable` backend) and `process_cwd`/`ancestor_env`/`kernel_version`/`os_name` (sysinfo) compile + run; no code change expected.

## 4. (Optional) free_disk_bytes Windows impl

- [x] 4.1 DONE (not deferred) — replaced the `#[cfg(not(unix))]` `free_disk_bytes` `0` stub (`mcp/worktree_sessions.rs`) with `sysinfo::Disks`: longest mount-point prefix of the path → `available_space()`, `unwrap_or(0)`. Unix `statvfs` path unchanged. Now reports real headroom on Windows (and any non-unix target).

## 5. Verification

- [ ] 5.1 **Windows gate green** — `cargo check --target x86_64-pc-windows-msvc`.
- [ ] 5.2 **`cargo test` on `windows-latest`** — kill round-trip (spawn throwaway child → `kill_pid` → assert exit) + `listening_ports()` returns the runner's listeners.
- [x] 5.3 **Linux full check** ✅ — `cargo clippy -- -D warnings` (no issues) + `cargo test` (699 passed, 1 ignored) + `cargo fmt --check` (clean). The `#[cfg(unix)]` kill path is untouched.
- [ ] 5.4 **macOS gate green** — `cargo check --target aarch64-apple-darwin`.
- [ ] 5.5 **User Windows-machine walk (UNVERIFIED-pending)** — ports chip shows dev servers, free-port works, quake-shell teardown kills the dev-server tree, quake cwd resolves or degrades to None.
