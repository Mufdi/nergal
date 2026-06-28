# Implementation — windows-proc

No SQLite schema changes. Small `#[cfg(windows)]` addition + two un-gates.

## Verified codebase facts (do not re-assume)

Verified against current source 2026-06-28:

- **Already cross-platform (compile + run on Windows, no change):** `kernel_version`/`os_name` (sysinfo, `platform_proc.rs:27,34`), `process_cwd` (sysinfo `cwd()`, `:46`), `descendants` (sysinfo BFS, `:62`, ungated), `port_owner` (listeners + sysinfo, `:215`, ungated), `ancestor_env` (sysinfo environ, `:275`, ungated), and the `#[cfg(not(target_os = "linux"))] listening_ports` (listeners, `:154`).
- **`listeners` 0.6 has a Windows backend** — `~/.cargo/registry/.../listeners-0.6.0/src/platform/windows/` + `[target.'cfg(target_os = "windows")'.dependencies.windows]` in its Cargo.toml. So the non-Linux `listening_ports`/`port_owner` branch resolves on Windows via `GetExtendedTcpTable`.
- **The Windows gap:** `kill_tree` (`platform_proc.rs:96-111`) and `kill_pid` (`:115-122`) are `#[cfg(unix)]` (libc `kill` + `kill(-pgid)`). Their callers: `pty.rs` `Drop` teardown calls `kill_tree(pid)` inside a `#[cfg(unix)]` block (`:54`/`:58`); `browser.rs:261` calls `kill_pid(owner.pid)` in the free-port path (gated by `windows-compile`).
- **`sysinfo` `Process::kill()` on Windows shells out to `taskkill.exe /F`** (sysinfo 0.33 `src/windows/process.rs`), NOT `TerminateProcess` — so we use the raw Win32 call instead (Decision 1). `descendants()` returns the BFS set already (reused as-is, cross-platform).
- **`windows` crate** is brought by `windows-ipc` (#2, sequenced first) with `Win32_System_Threading` + `Win32_Foundation` already in its feature list — `OpenProcess`/`TerminateProcess`/`CloseHandle` live there. If #3 lands without #2, declare the same `[target.'cfg(windows)'.dependencies.windows]` block.

## Edit plan

### Step 1 — `#[cfg(windows)]` `kill_tree` (raw TerminateProcess)
```rust
#[cfg(windows)]
fn terminate(pid: u32) {
    use windows::Win32::System::Threading::{OpenProcess, TerminateProcess, PROCESS_TERMINATE};
    use windows::Win32::Foundation::CloseHandle;
    if pid <= 4 { return; } // 0 = System Idle, 4 = System
    unsafe {
        if let Ok(h) = OpenProcess(PROCESS_TERMINATE, false, pid) {
            let _ = TerminateProcess(h, 1);
            let _ = CloseHandle(h);
        }
    }
}

#[cfg(windows)]
pub fn kill_tree(root: u32) {
    if root <= 4 { return; }
    for pid in descendants(root).into_iter().rev() { terminate(pid); } // leaves first
    terminate(root);
}
```
No `kill(-pgid)` analog (no POSIX process group on Windows); the BFS is the reach.

### Step 2 — `#[cfg(windows)]` `kill_pid`
```rust
#[cfg(windows)]
pub fn kill_pid(pid: u32) -> std::io::Result<()> {
    use windows::Win32::System::Threading::{OpenProcess, TerminateProcess, PROCESS_TERMINATE};
    use windows::Win32::Foundation::CloseHandle;
    if pid <= 4 { return Err(std::io::Error::other("refusing to terminate a system pid")); }
    unsafe {
        let h = OpenProcess(PROCESS_TERMINATE, false, pid)
            .map_err(|e| std::io::Error::other(format!("OpenProcess: {e}")))?;
        let r = TerminateProcess(h, 1);
        let _ = CloseHandle(h);
        r.map_err(|e| std::io::Error::other(format!("TerminateProcess: {e}")))
    }
}
```
(The `windows` crate's `TerminateProcess`/`OpenProcess` return `Result<()>` / `Result<HANDLE>`; adjust the exact `map_err` to the crate version in tree.)

### Step 3 — Un-gate the kill callers
- `pty.rs:54` — the `#[cfg(unix)]` `Drop` teardown block that calls `kill_tree(pid)`: make it call `kill_tree` on Windows too (un-gate, or add a `#[cfg(windows)]` arm calling the same fn). Preserve any unix-only steps (e.g. a `setsid`-related call stays `#[cfg(unix)]`; only the `kill_tree` call un-gates).
- `browser.rs:261` — the `kill_pid(owner.pid)` free-port path: un-gate (it now resolves on Windows).

### Step 4 (optional) — `free_disk_bytes` Windows impl
Replace the `windows-compile` `#[cfg(windows)]` `0` stub in `mcp/worktree_sessions.rs` with `sysinfo::Disks::new_with_refreshed_list()` → find the disk whose mount point is a prefix of the worktree path → `available_space()`. Cross-platform; or leave the stub (non-critical hint). If done, keep the Unix `statvfs` path unchanged.

## Verification (maps to tasks.md ## N. Verification)

- Windows gate green (`cargo check --target x86_64-pc-windows-msvc`).
- `cargo test` on `windows-latest`: a kill round-trip test (spawn a throwaway child, `kill_pid` it, assert it dies) + `listening_ports()` returns the runner's own listeners.
- Linux full check stays green (no change to the `#[cfg(unix)]` path).
- macOS gate green.
- User Windows-machine walk (UNVERIFIED-pending): ports chip shows dev servers, free-port works, quake-shell teardown kills the dev-server tree, quake cwd resolves (or degrades to None gracefully).
