# Design — windows-proc

## Context

The macOS port deliberately replaced the hand-rolled `/proc` subsystems with `sysinfo` + `listeners` (cross-platform crates) rather than shelling to Linux binaries. That choice makes Windows nearly free: `descendants`, `process_cwd`, `ancestor_env`, `port_owner`, the non-Linux `listening_ports`, `kernel_version`, and `os_name` are all already cross-platform and compile + run on Windows. The single Windows gap is the POSIX-signal kill path (`kill_tree`/`kill_pid` are `#[cfg(unix)]`). This change fills it and un-gates the callers.

## Decision 1 — Windows kill via raw `OpenProcess` + `TerminateProcess` (windows crate), no pgid

Unix `kill_tree` does descendants-first SIGTERM, then `kill(-pgid)` (to catch a foreground group a dev server spawned), then the root. **Windows has no POSIX process group**, so the `kill(-pgid)` step has no analog — the descendant BFS (already cross-platform via `descendants()`) is the entire reach mechanism.

**Fact check (iprev #3):** `sysinfo` 0.33's `Process::kill()` on Windows does **NOT** call `TerminateProcess` directly — it shells out to `taskkill.exe /PID <pid> /F` (a subprocess per pid). An earlier draft of this design wrongly stated `kill()` "is `TerminateProcess` under the hood." For a process *tree* that means N `taskkill.exe` subprocess spawns on every teardown, a dependency on `taskkill.exe` being on PATH, and a transient console window per spawn (user-visible on a GUI app).

**Chosen**: `#[cfg(windows)] kill_tree`/`kill_pid` use **raw `OpenProcess(PROCESS_TERMINATE, false, pid)` → `TerminateProcess(h, 1)` → `CloseHandle(h)`** via the `windows` crate (`Win32::System::Threading` + `Win32::Foundation`). `kill_tree` walks the reversed-BFS `descendants()` set (cross-platform, unchanged) + the root; `kill_pid` does one pid and maps a failed open/terminate to `io::Error`. Guard against pid 0 (System Idle) and 4 (System). This makes the spec's `TerminateProcess` contract literally true, avoids N subprocess spawns + the `taskkill.exe` PATH dependency, and emits no transient console windows on session teardown.

- *Alternative: `sysinfo` `Process::kill()` (taskkill `/F` subprocess).* **Rejected** — zero-unsafe and zero-new-Win32, but spawns one `taskkill.exe` per descendant (flashing a console each time on a GUI app's teardown) and depends on `taskkill.exe`. The raw call is a small, well-understood unsafe block with better UX.
- *Cargo note:* this reuses the `windows` crate dep that `windows-ipc` (#2, sequenced before #3) introduces (`Win32_System_Threading`, `Win32_Foundation` — both already in #2's feature list). If #3 lands independently of #2, it declares the same `[target.'cfg(windows)'.dependencies.windows]` block (idempotent).
- *Note on completeness:* `TerminateProcess` is more abrupt than `SIGTERM` (no graceful-shutdown signal) — but the Unix path already escalates to a hard signal, and a dev-server teardown does not need graceful shutdown. The descendant BFS catches the `pnpm → node` tree that motivated the cross-platform kill (BUG-06), which on Windows is a parent-pid tree just like Unix.

## Decision 2 — Listening ports rely on the `listeners` Windows backend, no new code

`listeners` 0.6 ships a Windows backend (`src/platform/windows/`, `GetExtendedTcpTable` with a per-target `windows` dep). The existing `#[cfg(not(target_os = "linux"))] listening_ports()` + `port_owner()` therefore work on Windows unchanged. The crate calls `GetExtendedTcpTable` with `TCP_TABLE_OWNER_PID_ALL` and filters to `SocketState::Listen` afterward (iprev #3 — NOT `TCP_TABLE_OWNER_PID_LISTENER`; functional outcome is identical), returning listeners **system-wide with owning PID**, so the Linux-specific concern (system-wide `/proc/net/tcp` vs user-scoped `listeners`) does not arise — Windows already sees all listeners + the owner in one call. **Chosen**: no new port code; the spec is extended to assert Windows coverage and a unit/integration check confirms it.

- *Alternative: hand-roll `GetExtendedTcpTable`.* **Rejected** — duplicates what the crate already does and the macOS port already chose the crate.

## Decision 3 — `process_cwd` / `ancestor_env` degrade to `None` on protected Windows processes

Reading another process's cwd or environment on Windows requires reading its PEB (`NtQueryInformationProcess`), which `sysinfo` attempts but which fails for protected or cross-user processes (mirroring macOS under SIP). **Chosen**: accept `None` as the graceful-degradation contract — callers (`mcp/shim.rs` env recovery, the quake-shell cwd) already treat `None` as "unavailable" and fall back. No code change; documented in the spec so the behaviour is a contract, not a surprise.

## Decision 4 (optional) — fold the `windows-compile` `free_disk_bytes` stub into a real impl

`windows-compile` left `free_disk_bytes` a `#[cfg(windows)]` `0` stub and noted the functional impl belongs here (disk introspection). **Chosen (optional task)**: replace the stub with `sysinfo::Disks` (cross-platform), returning the free bytes of the disk containing the worktree path — OR leave the stub if scope is tight. Included as an optional task; the worktree disk hint is non-critical, so deferral is acceptable.

## Risks / Trade-offs

- **[Low] `TerminateProcess` abruptness** — acceptable for dev-server teardown (see Decision 1).
- **[Low] sysinfo cwd/environ unavailable on Windows** — graceful `None`, documented (Decision 3).
- **[Low] No local validation** — `windows-latest` CI compiles + can unit-test the kill/ports paths against the runner's own processes; the user's Windows machine walks the quake-shell + ports-chip features.

## Migration / rollback

Additive `#[cfg(windows)]` fns + two caller un-gates. Git-revertible; a revert restores the `#[cfg(unix)]`-only kill path and re-gates the callers (Windows returns to compile-but-no-kill).
