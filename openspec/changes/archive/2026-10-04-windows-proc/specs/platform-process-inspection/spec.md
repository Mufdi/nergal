## ADDED Requirements

### Requirement: Windows process and port introspection

The `platform_proc` interface SHALL function on Windows with the same signatures as Linux/macOS, selected at compile time via `#[cfg]`, reusing the cross-platform `sysinfo` and `listeners` backends already in place and adding only a Windows kill implementation. Specifically:

- **Process-tree termination** SHALL resolve descendants via the existing cross-platform `descendants()` (sysinfo BFS) and terminate each descendant (leaves first) plus the root via raw `OpenProcess(PROCESS_TERMINATE)` + `TerminateProcess` + `CloseHandle` (windows crate) — NOT `sysinfo`'s `Process::kill()`, which shells out to `taskkill.exe` (subprocess + transient console per pid). Windows has no POSIX process group, so there is no `kill(-pgid)` step; the descendant BFS is the sole reach mechanism. System pids (0 = System Idle, 4 = System) SHALL never be targeted.
- **Free-a-port** SHALL terminate the owning process via the same raw `TerminateProcess`, returning an `io::Error` on failure (parity with the Unix path).
- **Listening-port discovery** and **port-owner attribution** SHALL resolve via the `listeners` crate's Windows backend (`GetExtendedTcpTable` with `TCP_TABLE_OWNER_PID_ALL` + a `SocketState::Listen` filter), which enumerates listening sockets system-wide with the owning PID — no separate system-wide read is needed (unlike the Linux `/proc/net/tcp` path).
- **Process cwd** and **ancestor-env recovery** SHALL resolve via `sysinfo`, returning `None` when the target process is protected or owned by another user (sysinfo cannot read its PEB) — the same graceful-degradation contract as macOS under SIP; callers treat `None` as "unavailable" and fall back.
- **Kernel version** and **OS name** SHALL resolve via `sysinfo` (`kernel_version`, `long_os_version`), which already produce meaningful Windows strings.

#### Scenario: Process tree is terminated on Windows without a process group

- **WHEN** a shell PTY whose child spawned a dev server is dropped on Windows
- **THEN** every descendant of the shell's child pid SHALL be resolved via the sysinfo BFS and terminated via raw `TerminateProcess` (no `taskkill.exe` subprocess), with no reliance on a POSIX process group, and pids 0/4 SHALL never be signalled

#### Scenario: Listening ports and owner resolve on Windows

- **WHEN** a dev server is listening on a user-range port on Windows
- **THEN** the port SHALL appear in `listening_ports()` (via the `listeners` `GetExtendedTcpTable` backend, deduped, sorted, user-range filtered), and `port_owner()` SHALL return the owning pid plus a sysinfo-derived label and cwd basename

#### Scenario: cwd / ancestor-env degrade to None on a protected Windows process

- **WHEN** `process_cwd` or `ancestor_env` targets a process whose PEB sysinfo cannot read on Windows
- **THEN** the lookup SHALL return `None` (not error), and the quake-shell cwd / Codex env-recovery features SHALL fall back gracefully rather than break

#### Scenario: Free a port on Windows

- **WHEN** the user requests freeing a port owned by one of their processes on Windows
- **THEN** the owning process SHALL be terminated via raw `TerminateProcess` (windows crate) and the call SHALL return success; a failed open/terminate SHALL return an error string

---

## MODIFIED Requirements

### Requirement: Process-tree (descendant) termination

The system SHALL resolve the full descendant process set of a root pid by parent-pid relationship and SHALL terminate that set so shell-started processes (`pnpm`/`node` dev servers spawning new process groups) do not outlive session or app teardown. The termination mechanism is per-platform: on **Unix** it is descendants-first, then the root's process group (`kill(-pgid)`), then the root (POSIX `SIGTERM`); on **Windows**, which has no POSIX process group, it is descendants-first then the root via raw `OpenProcess(PROCESS_TERMINATE)` + `TerminateProcess` (windows crate, NOT sysinfo's `taskkill.exe`-shelling `Process::kill`), the descendant BFS being the sole reach mechanism. This SHALL work on Linux, macOS, and Windows.

#### Scenario: Descendant dev server is killed on teardown

- **WHEN** a shell PTY whose child spawned a dev server is dropped (session close or app exit) on any supported OS
- **THEN** every descendant of the shell's child pid SHALL be terminated (SIGTERM on Unix including a process a plain `kill(-pgid)` would miss; `TerminateProcess` on Windows)

#### Scenario: Self-pid and system pids are never targeted

- **WHEN** the tree walk runs
- **THEN** it SHALL never signal pid 0 or 1 (Unix) / pid 0 or 4 (Windows System pids), and the root SHALL not be double-counted as its own descendant
