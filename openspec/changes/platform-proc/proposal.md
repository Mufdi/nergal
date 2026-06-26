## Why

Three subsystems read the Linux `/proc` filesystem directly — the port scanner that powers the ports status-bar chip and live-preview discovery (`browser.rs`, ungated, no quarantine), the quake-shell cwd resolver (`pty.rs`), and the MCP shim's Codex session-id recovery from ancestor processes (`shim.rs`). `/proc` does not exist on macOS, so on the first (macOS) port target these capabilities silently degrade: ports never appear, quake shells lose their cwd, and cross-session env recovery returns `None`. The reads are hand-rolled and scattered, with three independent process-enumeration loops. This change collapses them behind one cross-platform interface so the macOS port restores full behaviour without forking the call sites.

## What Changes

- **New `platform_proc` module** abstracting the three process/port primitives the app actually uses behind one interface, implemented for Linux AND macOS: (1) process-tree / PPID walk, (2) process cwd, (3) listening-TCP-port enumeration with owning pid + exe/cmdline/cwd resolution.
- **Adopt `sysinfo`** (process tree, ppid, cwd, name, cmdline) and a TCP-listener crate (`netstat2` or `listeners`, decided in design) so the three hand-rolled per-OS `/proc` reads collapse to one cross-platform impl rather than hand-rolling libproc/sysctl for macOS.
- **Migrate `browser.rs`** — the largest ungated Linux-only subsystem — off `/proc/net/tcp{,6}`, `/proc/<pid>/fd`, `/proc/<pid>/{comm,cmdline,exe,cwd}` onto `platform_proc`. The port-owner SIGTERM (`kill_port`) stays `cfg(unix)`.
- **Migrate `pty.rs`** process-tree kill (`kill_process_tree`) and `process_cwd` onto `platform_proc`, removing the `#[cfg(not(target_os = "linux"))]` cwd stub that returned `None`.
- **Migrate `shim.rs`** ancestor PPID walk + `NERGAL_SESSION_ID` recovery onto `platform_proc`, removing the non-Linux stub — restoring Codex env recovery on macOS.
- **Map `updater.rs` kernel-version diagnostic** (`/proc/sys/kernel/osrelease`) onto `sysinfo::System::kernel_version()`.
- Linux behaviour is preserved bit-for-bit (no regression); macOS is added behind the same interface. New per-OS branches are born `#[cfg]`-gated per the cross-platform invariant.

## Capabilities

### New Capabilities

- `platform-process-inspection`: A `platform_proc` module that abstracts process-tree/PPID walking, process cwd lookup, and listening-TCP-port discovery (with owning pid/exe/cmdline/cwd) behind one interface working identically on Linux and macOS, collapsing the current scattered `/proc` reads.

### Modified Capabilities

<!-- None. The consumer specs (live-preview-browser, quake-terminal, nergal-mcp-server,
     session-directory) keep their observable behaviour unchanged — this change only
     swaps the platform substrate beneath them and extends platform reach, which the
     new platform-process-inspection capability owns. -->

## Impact

- **New dependency**: `sysinfo` + one TCP-listener crate (`netstat2` or `listeners`), added under appropriate `cfg` gating. Removes most direct `/proc` string parsing.
- **Backend code**: `browser.rs` (port scanner + owner resolution + `kill_port`), `pty.rs` (`kill_process_tree`, `process_cwd`, `live_session_cwds`), `mcp/shim.rs` (`session_hint_from_ancestors`, `parent_pid`), `updater.rs` (`collect_diagnostics`). A new `platform_proc.rs` (or `platform_proc/` module) and its registration in `lib.rs`.
- **`libc`**: this change assumes the sibling `platform-compile` change has moved `libc` to `cfg(unix)` and established a `cargo check --target` CI gate, so the crate compiles on macOS. The `libc::kill` SIGTERM calls in `browser.rs`/`pty.rs` remain `cfg(unix)` (POSIX signals exist on macOS).
- **Downstream features that regain macOS support**: ports status-bar chip + live-preview port discovery, quake-shell cwd resolution, cross-session Codex env recovery, kernel-version diagnostics.
- **Tests**: existing pure-function tests in `browser.rs` (label resolution, port parsing) must keep passing; new tests cover the `platform_proc` interface against the live host.
