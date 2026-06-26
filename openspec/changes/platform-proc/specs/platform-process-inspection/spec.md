## ADDED Requirements

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

The system SHALL resolve the full descendant process set of a root pid by parent-pid relationship and SHALL terminate that set (descendants first, then the root's process group, then the root) so shell-started processes (`pnpm`/`node` dev servers spawning new process groups) do not outlive session or app teardown. This SHALL work on Linux and macOS.

#### Scenario: Descendant dev server is killed on teardown

- **WHEN** a shell PTY whose child spawned a dev server in a new process group is dropped (session close or app exit)
- **THEN** every descendant of the shell's child pid SHALL receive SIGTERM, including processes a plain `kill(-pgid)` would miss

#### Scenario: Self-pid and pid<=1 are never targeted

- **WHEN** the tree walk runs
- **THEN** it SHALL never signal pid 0 or 1, and the root SHALL not be double-counted as its own descendant

### Requirement: Process working-directory lookup

The system SHALL return the absolute working directory of a given pid, or `None` when it cannot be determined (process gone, permission denied, unsupported). This SHALL work on Linux and macOS (replacing the previous non-Linux stub that always returned `None`).

#### Scenario: Quake shell cwd resolves on macOS

- **WHEN** a command is submitted in a quake aux shell on macOS and the shell's child pid is known
- **THEN** the shell's real working directory SHALL be resolved so the remembered command persists with the correct cwd context

#### Scenario: Unknown pid yields None

- **WHEN** the pid no longer exists or is not readable
- **THEN** the lookup SHALL return `None` rather than erroring

### Requirement: Listening-TCP-port discovery

The system SHALL enumerate TCP sockets in the LISTEN state owned by the current user, deduplicated and filtered to the user-port range, returning ports sorted ascending. For a given listening port the system SHALL resolve the owning process's pid and, from it, a human label (interpreter scripts resolve to the script/module name, e.g. `node …/vite` → `vite`) and the project folder name (cwd basename). This SHALL work on Linux and macOS.

#### Scenario: Dev server appears in the active port set

- **WHEN** a dev server is listening on a user-range port (Linux or macOS)
- **THEN** that port SHALL appear in the enumerated listening set, deduplicated across IPv4/IPv6, sorted ascending

#### Scenario: Port owner and label resolve

- **WHEN** the owning process info for a listening port is requested
- **THEN** the system SHALL return the owning pid plus a label derived from the process cmdline (interpreter → script name) and the cwd basename as the project name

#### Scenario: Unowned (Docker-published) port has no /proc owner

- **WHEN** a port is published by a Docker container (listener is a root proxy, not user-owned)
- **THEN** the user-owned-socket lookup SHALL return no owning pid, leaving the Docker-container fallback path to label/stop it

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

### Requirement: Kernel version for diagnostics

The system SHALL report the running kernel/OS version string in the diagnostics bundle via a cross-platform source, falling back to `"unknown"` when unavailable, instead of reading `/proc/sys/kernel/osrelease` directly.

#### Scenario: Diagnostics include a kernel version on both platforms

- **WHEN** the user collects a diagnostics bundle on Linux or macOS
- **THEN** the bundle SHALL include a kernel/OS version string, or `"unknown"` if it cannot be read
