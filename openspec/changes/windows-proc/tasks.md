# Tasks — windows-proc

Depends on `windows-compile` (the `#[cfg(unix)]` gating of the kill callers this change un-gates).

## 1. Windows kill path (raw TerminateProcess — iprev #3)

- [ ] 1.1 `platform_proc.rs` — `#[cfg(windows)]` `terminate(pid)` helper: `OpenProcess(PROCESS_TERMINATE, false, pid)` → `TerminateProcess(h, 1)` → `CloseHandle(h)` (windows crate `Win32::System::Threading` + `Win32::Foundation`); guard `pid > 4`. `kill_tree(root)`: reversed-BFS `descendants()` → `terminate` each + root. NOT `sysinfo` `Process::kill()` (shells to `taskkill.exe`). No `kill(-pgid)` analog.
- [ ] 1.2 `platform_proc.rs` — `#[cfg(windows)] kill_pid(pid)`: same raw open+terminate; map open/terminate failure → `io::Error::other`, system pid → error. Confirm the `windows` crate dep (Win32_System_Threading + Win32_Foundation) is present (shared with #2; declare the per-target block if #3 lands alone).

## 2. Un-gate the kill callers

- [ ] 2.1 `pty.rs:54` — the `#[cfg(unix)]` `Drop` teardown calling `kill_tree(pid)`: make the `kill_tree` call reachable on Windows (un-gate or add a `#[cfg(windows)]` arm); keep any genuinely unix-only adjacent step (`setsid` etc.) `#[cfg(unix)]`.
- [ ] 2.2 `browser.rs:261` — un-gate the `kill_pid(owner.pid)` free-port caller (now resolves on Windows).

## 3. Confirm the already-cross-platform paths on Windows

- [ ] 3.1 Confirm via the Windows gate + a `cargo test` on `windows-latest` that `listening_ports()` / `port_owner()` (listeners `GetExtendedTcpTable` backend) and `process_cwd`/`ancestor_env`/`kernel_version`/`os_name` (sysinfo) compile + run; no code change expected.

## 4. (Optional) free_disk_bytes Windows impl

- [ ] 4.1 OPTIONAL — replace the `windows-compile` `#[cfg(windows)]` `free_disk_bytes` `0` stub (`mcp/worktree_sessions.rs`) with `sysinfo::Disks` (free space of the disk containing the worktree path). Keep the Unix `statvfs` path unchanged. Defer if scope is tight (non-critical hint).

## 5. Verification

- [ ] 5.1 **Windows gate green** — `cargo check --target x86_64-pc-windows-msvc`.
- [ ] 5.2 **`cargo test` on `windows-latest`** — kill round-trip (spawn throwaway child → `kill_pid` → assert exit) + `listening_ports()` returns the runner's listeners.
- [ ] 5.3 **Linux full check** — `cd src-tauri && cargo clippy -- -D warnings && cargo test && cargo fmt --check` (the `#[cfg(unix)]` kill path is untouched).
- [ ] 5.4 **macOS gate green** — `cargo check --target aarch64-apple-darwin`.
- [ ] 5.5 **User Windows-machine walk (UNVERIFIED-pending)** — ports chip shows dev servers, free-port works, quake-shell teardown kills the dev-server tree, quake cwd resolves or degrades to None.
