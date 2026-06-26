# Implementation Plan: platform-proc

> Grounded in current codebase, symbols verified 2026-06-26. Behaviour (not just symbol existence) verified for the load-bearing claims below.

## Verified codebase facts (do not re-assume)

### Cargo / gating
- `src-tauri/Cargo.toml:114-115` — `libc = "0.2"` is currently under `[target.'cfg(target_os = "linux")'.dependencies]`. This is *narrower* than its use: `pty.rs:64` calls `libc::kill` under `cfg(all(unix, not(target_os = "linux")))`, so today the crate would not compile that branch on a non-Linux unix. **Sibling `platform-compile` moves `libc` to `cfg(unix)`** — this change assumes that has landed. Do NOT re-add `libc` here.
- No `sysinfo`, `netstat2`, or `listeners` dependency exists yet (`grep` over `Cargo.toml` returns nothing) — they are net-new.

### pty.rs (process tree + cwd)
- `pty.rs:48-67` — `impl Drop for PtyInstance`: on `cfg(target_os = "linux")` calls `kill_process_tree(pid)`; on `cfg(all(unix, not(target_os = "linux")))` calls `libc::kill(-(pid), SIGTERM)` (process-group fallback, no `/proc`). Guarded `pid > 1`.
- `pty.rs:75-116` — `fn kill_process_tree(root: u32)` is `#[cfg(target_os = "linux")]`. Inner `ppid_of` reads `/proc/{pid}/stat` (line 78), splits after the last `)` to skip a `comm` containing spaces. Enumerates via `read_dir("/proc")` (line 84). BFS builds the descendant set (94-104). Signals descendants first reversed, then `kill(-(root), SIGTERM)` + `kill(root, SIGTERM)` (107-115). **No non-Linux counterpart exists** — on macOS the `Drop` path uses the process-group fallback only, losing the new-process-group descendants (BUG-06 regressions).
- `pty.rs:118-123` — `fn process_cwd(pid) -> Option<String>` `#[cfg(target_os = "linux")]` readlinks `/proc/{pid}/cwd`.
- `pty.rs:125-128` — `fn process_cwd(_pid) -> Option<String>` `#[cfg(not(target_os = "linux"))]` returns `None` (the dead-on-macOS stub).
- `pty.rs:179-192` — `live_session_cwds()` iterates instances, calls `process_cwd(child_pid)` to collect unique dirs (used by `stop_compose_projects` at shutdown).
- `pty.rs:1330-1335` — aux-shell input tracker: on Enter, `shell_cwd = instance.child_pid.and_then(process_cwd)` (the quake-shell cwd resolution path).
- `PtyInstance.child_pid: Option<u32>` (`pty.rs:43`) is the pid source for all of the above.

### browser.rs (ports — LARGEST ungated Linux-only subsystem)
- `browser.rs:55-58` — `SCAN_INTERVAL = 3s`; `REMOVE_AFTER_INACTIVE_SCANS = 2` (port-flap absorption). Behaviour to preserve.
- `browser.rs:60-61` — `const PROC_NET_TCP = "/proc/net/tcp"`, `PROC_NET_TCP6 = "/proc/net/tcp6"`.
- `browser.rs:184-199` — `read_listening_user_ports() -> Vec<u16>`: reads both files, `parse_listening_ports`, sort + dedup + retain `(MIN_PORT..=MAX_PORT)`. Tested at `browser.rs:489`.
- `browser.rs:201-230` — `listen_inode_for_port(port)`: parses `/proc/net/tcp{,6}` columns (1=local, 3=state, 9=inode), filters `state == TCP_LISTEN`, returns the socket inode.
- `browser.rs:232-257` — `pid_for_socket_inode(inode)`: scans `read_dir("/proc")` then each `/proc/<pid>/fd` for a `socket:[<inode>]` symlink. "Only the user's own processes are readable."
- `browser.rs:259-306` — `basename`, `strip_script_ext`, `INTERPRETERS`, `resolve_label(args, exe_base, comm)` — **pure, unit-tested**; the interpreter→script heuristic + Chromium `arg0=="exe"` special-case (reads exe/comm). Keep as-is; only change the data source.
- `browser.rs:308-332` — `process_label(pid)`: reads `/proc/{pid}/comm`, `/proc/{pid}/cmdline` (NUL-split), `/proc/{pid}/exe` (readlink basename); feeds `resolve_label`.
- `browser.rs:334-338` — `process_cwd_name(pid)`: readlink `/proc/{pid}/cwd`, returns the basename (project folder).
- `browser.rs:387-399` — `port_process_info(port) -> Option<PortProcess>`: `listen_inode_for_port` → `pid_for_socket_inode` → `{ label: process_label, project: process_cwd_name, pid }`; falls back to `docker_container_for_port`.
- `browser.rs:401-419` — `kill_port(port)` `#[tauri::command]`: resolves owner via inode→pid, `libc::kill(pid, SIGTERM)` (line 411, UNGATED today), error on non-zero rc; Docker fallback after. Signal call must become `cfg(unix)`.
- `browser.rs:93` — public scan entry calls `read_listening_user_ports()`.
- Tests at `browser.rs:489`, `:560` exercise `read_listening_user_ports`/the filter — must keep passing (Linux no-regression guard).

### mcp/shim.rs (ancestor env recovery)
- `shim.rs:144-152` — `find_in_environ(data, key) -> Option<String>`: pure, NUL-split, first non-empty match. Tested at `shim.rs:196`. Keep; only change the data source.
- `shim.rs:154-161` — `parent_pid(pid)` `#[cfg(target_os = "linux")]` reads `/proc/{pid}/status` `PPid:` line.
- `shim.rs:163-185` — `session_hint_from_ancestors()` `#[cfg(target_os = "linux")]`: walks up to 8 parents from `std::process::id()`, reads `/proc/{pid}/environ`, returns first `NERGAL_SESSION_ID` else `CLAUDE_CODE_SESSION_ID`; stops at `pid <= 1`.
- `shim.rs:187-190` — `session_hint_from_ancestors()` `#[cfg(not(target_os = "linux"))]` returns `None` (dead-on-macOS stub; Codex env recovery disabled there).

### updater.rs (kernel diagnostics — minor)
- `updater.rs:431-443` — `collect_diagnostics()` `#[tauri::command]`: `kernel = std::fs::read_to_string("/proc/sys/kernel/osrelease")` (line 436), fallback `"unknown"`. Map to `sysinfo::System::kernel_version()` (or a `platform_proc::kernel_version()` wrapper) with the same `"unknown"` fallback.

## Execution order

1. **Prereq gate**: confirm `platform-compile` has landed `libc` → `cfg(unix)` and the `cargo check --target <macos>` CI gate. If not, block.
2. **Deps + spike**: add `sysinfo` + `netstat2` (and `listeners` as spike fallback) to `Cargo.toml`. Run the D2a spike: does `netstat2` associate owning pids for LISTEN sockets on macOS? Record the verdict in design Open Questions.
3. **New module** `src-tauri/src/platform_proc.rs` (or `platform_proc/mod.rs`): free functions — `kernel_version`, `process_cwd`, `descendants`, `kill_tree`, `kill_pid`, `listening_ports`, `port_owner`, `ancestor_env`. Register `mod platform_proc;` in `lib.rs`. Linux impl mirrors current `/proc` behaviour exactly first (parity), then add the `sysinfo`/`netstat2`-backed cross-platform impl.
4. **Migrate callers low-risk → high-risk**, full check after each:
   a. `updater.rs` kernel version.
   b. `shim.rs` ancestor env (`parent_pid` + `session_hint_from_ancestors` → `platform_proc::ancestor_env`); delete both stubs.
   c. `pty.rs` `process_cwd` (delete the `cfg(not(linux))` stub) + `kill_process_tree` → `platform_proc::descendants` + `kill_tree`; `live_session_cwds` + the line-1332 tracker now call `platform_proc::process_cwd`.
   d. `browser.rs` ports: `read_listening_user_ports` → `platform_proc::listening_ports`; `port_process_info`/`listen_inode_for_port`/`pid_for_socket_inode`/`process_label`/`process_cwd_name` → `platform_proc::port_owner` feeding the kept pure `resolve_label`; `kill_port` SIGTERM → `platform_proc::kill_pid` (`cfg(unix)`).
5. **Cleanup**: remove dead `/proc` consts and stubs; ensure no direct `/proc` path string remains outside `platform_proc` (`grep -rn "/proc" src/` should only hit the Linux impl in `platform_proc`).
6. **Verify**: full check on Linux (no regression) + macOS acceptance walk.

## Plan

### platform_proc module (new)
- `kernel_version() -> Option<String>` — `sysinfo::System::kernel_version()`.
- `process_cwd(pid: u32) -> Option<String>` — `sysinfo` process `.cwd()` → absolute path string; `None` on missing.
- `descendants(root: u32) -> Vec<u32>` — build the pid→ppid map from a `sysinfo` refresh, BFS the descendant set of `root` excluding `root` and pid<=1. Mirrors `pty.rs:94-104` semantics.
- `kill_tree(root: u32)` `cfg(unix)` — call `descendants`, SIGTERM descendants reversed, then `libc::kill(-(root), SIGTERM)` + `libc::kill(root, SIGTERM)` (preserve `pty.rs:107-115` ordering). Guard `pid > 1`.
- `kill_pid(pid: u32) -> std::io::Result<()>` `cfg(unix)` — `libc::kill(pid, SIGTERM)`, map non-zero rc to `last_os_error()` (preserve `browser.rs:411-417`).
- `listening_ports() -> Vec<u16>` — `netstat2` LISTEN TCP sockets (v4+v6), map to local port, sort + dedup + retain user-port range. Same output shape as `read_listening_user_ports`.
- `port_owner(port: u16) -> Option<PortOwner>` — `netstat2` owner pid for the LISTEN socket on `port`; pull `cmd()`/`exe()`/`cwd()` from `sysinfo` for that pid; return `{ pid, args, exe_base, comm, cwd_basename }` for the kept `resolve_label`/project logic. `None` for unowned (Docker-published) ports → caller keeps the Docker fallback.
- `ancestor_env(keys: &[&str], max_depth: usize) -> Option<String>` — walk parents from `std::process::id()` via `sysinfo` `.parent()`, read each ancestor's environment (`sysinfo` `.environ()` or equivalent) for the first matching key; stop at depth or pid<=1. Mirrors `shim.rs:167-184`.

### Caller edits
- `updater.rs:436` → `platform_proc::kernel_version().unwrap_or_else(|| "unknown".into())`.
- `shim.rs`: replace `parent_pid` + `session_hint_from_ancestors` (both cfgs) with one call to `platform_proc::ancestor_env(&["NERGAL_SESSION_ID","CLAUDE_CODE_SESSION_ID"], 8)`; keep pure `find_in_environ` only if still needed by tests, else fold into the module.
- `pty.rs`: `Drop` (48-67) calls `platform_proc::kill_tree` on all unix (drop the linux/non-linux split — `kill_tree` now handles descendants cross-platform); delete `kill_process_tree` (75-116) and both `process_cwd` defs (118-128); point `live_session_cwds` (184) and the tracker (1332) at `platform_proc::process_cwd`.
- `browser.rs`: route scan (93) through `platform_proc::listening_ports`; `port_process_info` (387) through `platform_proc::port_owner` + kept `resolve_label`; `kill_port` (411) through `platform_proc::kill_pid`; delete `PROC_NET_TCP*` consts, `listen_inode_for_port`, `pid_for_socket_inode`, `process_label`, `process_cwd_name` once unused. KEEP `resolve_label`, `basename`, `strip_script_ext`, `INTERPRETERS`, the port-range filter, and their tests.

## Per-phase risk

- **[Phase 2 — netstat2 macOS owner unreliable]** → Spike before migrating `browser.rs`; fallback `listeners` crate or `cfg(macos)` `libproc` owner-only lookup. Port *list* is independent and lower-risk than owner resolution.
- **[Phase 4c — kill_tree semantics drift]** → Preserve exact signalling order + `kill(-pgid)`; only enumeration source changes. Re-run the BUG-06 Linux manual check (dev server in a new process group must die on session close).
- **[Phase 4d — label regression]** → `resolve_label` + tests stay; only its inputs change source. Diff a live Linux ports-chip render before/after; the existing unit tests gate the pure logic.
- **[Phase 3 — sysinfo snapshot cost in the 3s scan]** → One process refresh per scan tick, reuse the snapshot for all owner lookups; do not new-up `System` per port. Measure in the spike.
- **[Any phase — accidental new `/proc` read]** → After cleanup, assert `grep -rn "/proc" src/` only matches the Linux arm inside `platform_proc`.

## Verification

Project full check (CLAUDE.md): `cd src-tauri && cargo clippy -- -D warnings && cargo test && cargo fmt --check && cd .. && npx tsc --noEmit`.

Cross-platform compile (owned by `platform-compile`, exercised here): `cargo check --target aarch64-apple-darwin` (and/or x86_64) must pass.

Change-specific manual checks:
- **Linux (no-regression):** ports chip lists dev servers with correct labels/projects; `kill_port` frees an owned port; quake aux-shell cwd resolves on Enter; close a session whose shell spawned a `pnpm dev` in a new process group → server dies (BUG-06); Codex MCP session attribution intact; diagnostics shows a kernel version.
- **macOS (acceptance):** ports chip discovers a `pnpm dev` listener with label/project; quake-shell cwd resolves; cross-session Codex env recovery returns the session id from an ancestor; diagnostics kernel string is non-`unknown`.
