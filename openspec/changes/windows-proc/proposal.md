## Why

The `platform_proc` module (process-tree kill, cwd lookup, listening-port discovery, ancestor-env recovery, OS diagnostics) was made cross-platform during the macOS port via `sysinfo` + the `listeners` crate. Most of it **already works on Windows unchanged** — `sysinfo` (process tree, cwd, environ, kernel/OS strings) and `listeners` 0.6 (which ships a Windows `GetExtendedTcpTable` backend) are all cross-platform. The one gap is the **signal-based kill path**: `kill_tree` / `kill_pid` are `#[cfg(unix)]` (POSIX `kill` + `kill(-pgid)`), so on Windows they do not exist and their callers (`pty.rs`, `browser.rs`) are gated off (by `windows-compile`). This change supplies the Windows kill implementation and un-gates the callers, completing process/port introspection on Windows.

It is a small change precisely because the macOS iteration chose cross-platform crates over shelling to Linux binaries — the investment pays off here.

## What Changes

- **`#[cfg(windows)]` `kill_tree(root)`** — Windows has no POSIX process group, so the `kill(-pgid)` step has no analog. Resolve the descendant set with the existing cross-platform `descendants()` (sysinfo BFS, unchanged) and terminate each descendant (leaves first) + the root via raw **`OpenProcess(PROCESS_TERMINATE)` + `TerminateProcess` + `CloseHandle`** (windows crate). NOT `sysinfo`'s `Process::kill()`, which on 0.33 shells out to `taskkill.exe /F` (N subprocess spawns + a transient console window per pid on teardown; iprev #3). Guard against system pids (0 = System Idle, 4 = System).
- **`#[cfg(windows)]` `kill_pid(pid)`** — same raw `TerminateProcess`; map a failed open/terminate to an `io::Error` (parity with the Unix `last_os_error` path).
- **Un-gate the kill callers** that `windows-compile` gated `#[cfg(unix)]`: `pty.rs` (the `Drop` teardown kill, `:54/58`) and `browser.rs` (the free-port path, `:261`).
- **Confirm + spec the already-working Windows paths** — `listening_ports()` / `port_owner()` (the `#[cfg(not(target_os = "linux"))]` branch) resolve via `listeners` 0.6's Windows backend (`GetExtendedTcpTable` with `TCP_TABLE_OWNER_PID_ALL` + a `SocketState::Listen` filter), enumerating listening sockets system-wide with the owning PID; `process_cwd` / `ancestor_env` / `kernel_version` / `os_name` resolve via `sysinfo`. No new code — the spec is extended to assert Windows coverage.
- **Document graceful degradation** — `process_cwd` and `ancestor_env` may return `None` on Windows for a protected or cross-user process (sysinfo cannot read its PEB), the same way macOS SIP can block them; callers already treat `None` as "unavailable" and fall back. The quake-shell cwd and Codex env-recovery features degrade rather than break.

## Capabilities

### New Capabilities

(none)

### Modified Capabilities

- `platform-process-inspection`: extends the contract to Windows. **Modifies** the requirements that say "Linux and macOS" to include Windows — process-tree termination (Windows `TerminateProcess` per descendant, no pgid), cwd lookup (sysinfo, `None`-on-protected), listening-port discovery (`listeners` Windows backend, system-wide via `GetExtendedTcpTable`), free-port (Windows terminate), ancestor-env recovery (sysinfo environ, `None`-on-protected), and OS/kernel diagnostics (sysinfo, already cross-platform).

## Impact

- **`src-tauri/src/platform_proc.rs`**: new `#[cfg(windows)]` `kill_tree` + `kill_pid` (raw `OpenProcess`+`TerminateProcess`+`CloseHandle` via the windows crate); the existing ungated `descendants`/`process_cwd`/`ancestor_env`/`port_owner`/`listening_ports`(non-linux)/`kernel_version`/`os_name` are unchanged (already cross-platform).
- **`src-tauri/src/pty.rs`**: un-gate the `Drop` teardown kill (the `#[cfg(unix)]` block at `:54`).
- **`src-tauri/src/browser.rs`**: un-gate the free-port `kill_pid` caller (`:261`).
- **`Cargo.toml`**: the kill path uses the `windows` crate (`Win32_System_Threading` + `Win32_Foundation`) that `windows-ipc` (#2, sequenced first) already brings; if #3 lands independently it declares the same per-target block. `listeners = "0.6"` and `sysinfo` already pull their Windows backends.
- **Out of scope**: IPC transport (`windows-ipc`), desktop integration (`windows-desktop`), bundling (`windows-bundle-ci`). The `windows-compile` `free_disk_bytes` Windows stub (deferred to here in #1's notes) — a functional `sysinfo::Disks` impl — MAY be folded in as a small extra since it is disk introspection, or left as the stub; this proposal includes it as an optional task.
