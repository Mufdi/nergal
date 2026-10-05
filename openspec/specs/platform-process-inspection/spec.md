# platform-process-inspection Specification

## Purpose
TBD - created by archiving change platform-proc. Update Purpose after archive.
## Requirements
### Requirement: Unified process/port inspection interface

The system SHALL provide a single `platform_proc` module exposing process-tree walking, process cwd lookup, and listening-TCP-port discovery behind one interface, with one implementation per supported OS selected at compile time via `#[cfg]`. All current callers (`browser.rs`, `pty.rs`, `mcp/shim.rs`, `updater.rs`) SHALL consume this interface instead of reading `/proc` directly. New per-OS code SHALL be born `#[cfg]`-gated.

#### Scenario: One interface, per-OS impl

- **WHEN** the crate is compiled for Linux or for macOS
- **THEN** the same `platform_proc` function signatures SHALL be available to callers, and exactly one OS-specific implementation SHALL be selected at compile time
- **AND** no caller outside `platform_proc` SHALL read `/proc` paths directly

#### Scenario: Linux behaviour is unchanged

- **WHEN** the app runs on Linux after this change
- **THEN** the ports chip, quake-shell cwd resolution, Codex env recovery, and kernel-version diagnostics SHALL behave identically to before the change (no regression)

### Requirement: Process-tree (descendant) termination

The system SHALL resolve the full descendant process set of a root pid by parent-pid relationship and SHALL terminate that set so shell-started processes (`pnpm`/`node` dev servers spawning new process groups) do not outlive session or app teardown. The termination mechanism is per-platform: on **Unix** it is descendants-first, then the root's process group (`kill(-pgid)`), then the root (POSIX `SIGTERM`); on **Windows**, which has no POSIX process group, it is descendants-first then the root via raw `OpenProcess(PROCESS_TERMINATE)` + `TerminateProcess` (windows crate, NOT sysinfo's `taskkill.exe`-shelling `Process::kill`), the descendant BFS being the sole reach mechanism. This SHALL work on Linux, macOS, and Windows.

#### Scenario: Descendant dev server is killed on teardown

- **WHEN** a shell PTY whose child spawned a dev server is dropped (session close or app exit) on any supported OS
- **THEN** every descendant of the shell's child pid SHALL be terminated (SIGTERM on Unix including a process a plain `kill(-pgid)` would miss; `TerminateProcess` on Windows)

#### Scenario: Self-pid and system pids are never targeted

- **WHEN** the tree walk runs
- **THEN** it SHALL never signal pid 0 or 1 (Unix) / pid 0 or 4 (Windows System pids), and the root SHALL not be double-counted as its own descendant

### Requirement: Process working-directory lookup

The system SHALL return the absolute working directory of a given pid, or `None` when it cannot be determined (process gone, permission denied, unsupported). This SHALL work on Linux and macOS (replacing the previous non-Linux stub that always returned `None`).

#### Scenario: Quake shell cwd resolves on macOS

- **WHEN** a command is submitted in a quake aux shell on macOS and the shell's child pid is known
- **THEN** the shell's real working directory SHALL be resolved so the remembered command persists with the correct cwd context

#### Scenario: Unknown pid yields None

- **WHEN** the pid no longer exists or is not readable
- **THEN** the lookup SHALL return `None` rather than erroring

### Requirement: Listening-TCP-port discovery

The system SHALL enumerate TCP sockets in the LISTEN state, deduplicated and filtered to the user-port range, returning ports sorted ascending. The enumeration is system-wide on Linux (reading `/proc/net/tcp` + `/proc/net/tcp6`) so that container-published ports whose listener is a root proxy (e.g. Docker) are included; on macOS it enumerates via the `listeners` crate. Owner attribution is a separate, current-user-scoped step: for a given listening port the system SHALL resolve the owning process's pid and, from it, a human label (interpreter scripts resolve to the script/module name, e.g. `node …/vite` → `vite`) and the project folder name (cwd basename), returning no owner when the socket is not user-owned. This SHALL work on Linux and macOS.

#### Scenario: Dev server appears in the active port set

- **WHEN** a dev server is listening on a user-range port (Linux or macOS)
- **THEN** that port SHALL appear in the enumerated listening set, deduplicated across IPv4/IPv6, sorted ascending

#### Scenario: Port owner and label resolve

- **WHEN** the owning process info for a listening port is requested
- **THEN** the system SHALL return the owning pid plus a label derived from the process cmdline (interpreter → script name) and the cwd basename as the project name

#### Scenario: Unowned (Docker-published) port has no /proc owner

- **WHEN** a port is published by a Docker container (listener is a root proxy, not user-owned)
- **THEN** the port SHALL still appear in the enumerated listening set (the Linux list is system-wide via `/proc/net/tcp`), AND the current-user owner lookup SHALL return no owning pid, leaving the Docker-container fallback path to label/stop it

### Requirement: Free a port by terminating its owner

The system SHALL send SIGTERM to the user process owning a given listening port to free it, returning an error when the signal fails. POSIX signal delivery SHALL remain available on all supported unixes.

#### Scenario: Owned port is freed

- **WHEN** the user requests freeing a port owned by one of their processes
- **THEN** that process SHALL receive SIGTERM and the call SHALL return success; a failed signal SHALL return an error string

### Requirement: Ancestor environment recovery

The system SHALL walk a bounded number of ancestor processes by parent-pid and read each ancestor's environment to recover a session-id variable (`NERGAL_SESSION_ID`, falling back to `CLAUDE_CODE_SESSION_ID`) when the current process's own environment was stripped. This SHALL work on Linux and macOS (replacing the previous non-Linux stub that returned `None`).

#### Scenario: Codex env recovery works on macOS

- **WHEN** the MCP shim runs under Codex (which sanitizes the MCP server env) on macOS
- **THEN** the shim SHALL recover `NERGAL_SESSION_ID` from a bounded walk of ancestor process environments, restoring session attribution

#### Scenario: Walk is bounded and stops at init

- **WHEN** no ancestor within the bound carries the variable, or the walk reaches pid <= 1
- **THEN** the recovery SHALL stop and return `None` without unbounded traversal

### Requirement: OS name and kernel version for diagnostics

The system SHALL report BOTH the human-readable OS/distribution name AND the running kernel version string in the diagnostics bundle via cross-platform sources, each falling back to `"unknown"` when unavailable, instead of reading `/proc/sys/kernel/osrelease` (kernel) and `/etc/os-release` (OS name) directly. The OS-name source SHALL produce a meaningful value on macOS, where `/etc/os-release` does not exist.

#### Scenario: Diagnostics include a kernel version on both platforms

- **WHEN** the user collects a diagnostics bundle on Linux or macOS
- **THEN** the bundle SHALL include a kernel version string, or `"unknown"` if it cannot be read

#### Scenario: Diagnostics include an OS name on both platforms

- **WHEN** the user collects a diagnostics bundle on macOS (where `/etc/os-release` is absent)
- **THEN** the bundle's `OS:` field SHALL show a meaningful OS/version string (e.g. `macOS 14.x`) sourced cross-platform, NOT `"unknown"`
- **AND** on Linux the `OS:` field SHALL remain a meaningful distribution string (no regression to `"unknown"` or a bare kernel string); byte-identity with the old `/etc/os-release` `PRETTY_NAME` is NOT required, since `sysinfo::System::long_os_version()` constructs an equivalent distro string

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

