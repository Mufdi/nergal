# Design — platform-proc

Ceremony: DEEP (cross-module, real per-OS design alternatives). This records the *why* behind collapsing three hand-rolled `/proc` subsystems into one cross-platform module as part of the macOS-first port.

## Context

Nergal is a Linux-only desktop wrapper being ported to macOS (first) then Windows. Three subsystems read `/proc` directly:

1. **Port scanner** (`browser.rs`) — the largest self-contained Linux-only subsystem, fully ungated. Every 3s it parses `/proc/net/tcp{,6}` for LISTEN sockets, then resolves owner via `/proc/<pid>/fd` (socket-inode match) → `/proc/<pid>/{comm,cmdline,exe,cwd}` for label + project name. Powers the ports status-bar chip and live-preview port discovery. `kill_port` SIGTERMs the owner.
2. **Process-tree + cwd** (`pty.rs`) — `kill_process_tree` BFS-walks `/proc/<pid>/stat` PPIDs to SIGTERM a shell's descendants (BUG-06: dev servers leak past session close); `process_cwd` readlinks `/proc/<pid>/cwd` for quake-shell cwd resolution and `live_session_cwds` (docker-compose-stop-on-exit). Already `#[cfg(target_os = "linux")]` with a non-Linux cwd stub returning `None`.
3. **Ancestor env recovery** (`mcp/shim.rs`) — walks `/proc/<pid>/status` PPIDs and reads `/proc/<pid>/environ` to recover `NERGAL_SESSION_ID` when Codex strips the MCP server env. Already `#[cfg(target_os = "linux")]` + stub returning `None`.

Plus a minor `/proc/sys/kernel/osrelease` read in `updater.rs` diagnostics.

On macOS, `/proc` does not exist. (1) is the worst case: ungated, so it would compile-fail or silently scan nothing. (2)/(3) already degrade to `None` stubs, meaning quake cwd and Codex env recovery are dead on macOS.

The sibling change `platform-compile` moves `libc` to `cfg(unix)` and adds a `cargo check --target` CI gate; this change assumes the crate compiles on macOS. The sibling `platform-ipc` owns sockets/FIFOs — out of scope here.

## Goals / Non-Goals

**Goals:**
- One `platform_proc` interface for: descendant-process termination, process cwd, listening-TCP-port discovery (+ owner pid/label/project), ancestor env recovery, kernel version.
- Working identically on Linux and macOS behind that interface; Linux behaviour preserved bit-for-bit.
- Collapse three independent process-enumeration loops into one substrate.
- New per-OS branches `#[cfg]`-gated from birth.

**Non-Goals:**
- Windows implementation (later target; interface SHALL leave room but no Windows code lands here).
- Changing any observable behaviour of the consumer features on Linux.
- Sockets/FIFOs/named-pipes (owned by `platform-ipc`).
- Docker-container port fallback logic (unchanged; it already shells out to `docker`).

## Decisions

### D1: Adopt `sysinfo` for process tree / cwd / cmdline (over hand-rolling libproc/sysctl)

**Chosen:** Use the `sysinfo` crate for process enumeration (pid, ppid, cwd, name, cmd vector). It maintains a `System` snapshot and exposes `process(pid)`, `.parent()`, `.cwd()`, `.cmd()`, `.exe()`, and `System::kernel_version()` cross-platform.

**Alternatives considered:**
- **Per-OS hand-roll (`libproc`/`sysctl` on macOS, keep `/proc` on Linux):** macOS process cwd requires `proc_pidinfo(PROC_PIDVNODEPATHINFO)`; process listing requires `sysctl(KERN_PROC)` or `proc_listpids`; cmdline requires `sysctl(KERN_PROCARGS2)` and manual parsing. This is ~200+ lines of `unsafe` FFI per primitive, duplicating what `sysinfo` already maintains and tests across OS versions. High soundness risk (raw pointers, `MaybeUninit`, struct layout). Rejected: cost/risk far exceeds the dependency.
- **Keep `/proc` behind `cfg(linux)` + a separate new macOS module:** preserves Linux exactly but leaves two parallel hand-rolled impls to maintain and still requires the macOS FFI hand-roll above for the macOS half. Rejected: doesn't reduce the macOS hand-roll, doubles maintenance.

**Trade-off:** `sysinfo` snapshots all processes when refreshed; the tree walk and ancestor walk want a fresh snapshot. We refresh only the process list (`refresh_processes` / targeted refresh) at the moment of use — not a long-lived background `System`. Acceptable: these calls are teardown/submit/recovery paths, not hot loops. The port scanner's 3s cadence is the only periodic caller and already pays a full `/proc` sweep today.

### D2: TCP-listener enumeration — `netstat2` vs `listeners` vs keep parsing

**Chosen (provisional, finalised in D2a):** a dedicated TCP-listener crate so the `/proc/net/tcp{,6}` + `/proc/<pid>/fd` inode-matching dance disappears. Two candidates:

- **`netstat2`** — cross-platform (Linux netlink/`/proc`, macOS via `sysctl`/`libproc`), returns sockets with state, local addr, and associated pids. Maps directly onto "LISTEN sockets + owning pid". Actively the successor to the abandoned `netstat-rs`.
- **`listeners`** — purpose-built "who listens on what": returns `{socket_addr, process{pid, name}}` for listening sockets, Linux + macOS. Smaller surface, exactly our use case, but less control over IPv4/IPv6 filtering and gives only name (not full cmdline) — we still need `sysinfo` for the interpreter→script label heuristic and the cwd-basename project name.

**Alternatives considered:**
- **Keep `/proc/net/tcp` parser on Linux + hand-roll macOS:** macOS has no `/proc/net/tcp`; the equivalent is `sysctl(NET_RT_...)`/`proc_pidfdinfo` FFI — again heavy `unsafe`. Rejected for the same reason as D1.

**D2a — Resolution:** Prefer **`netstat2`** for listener+pid (state filtering control, IPv4+IPv6, owning pid), then feed the pid into `sysinfo` for the label (cmdline interpreter heuristic) and project name (cwd basename). Fall back to `listeners` only if `netstat2`'s macOS pid-association proves unreliable in the spike. The label/project resolution stays in our `resolve_label`/`process_label` logic (already pure + tested) but sourced from `sysinfo.cmd()`/`.exe()`/`.cwd()` instead of `/proc` reads. **This is the one open call to validate in the macOS spike (see Open Questions).**

### D3: Keep POSIX signals direct (`libc::kill`) under `cfg(unix)`

SIGTERM delivery (`kill_port`, the process-tree termination, the process-group SIGTERM in `PtyInstance::drop`) stays `libc::kill` gated `cfg(unix)`. POSIX signals exist identically on macOS; no crate needed. `sysinfo`'s `Process::kill_with(Signal::Term)` is an option but it kills a single pid, not a process group (`kill(-pgid)`), which BUG-06 specifically needs. Keeping `libc::kill` preserves the `-pgid` semantics exactly. The descendant-set *discovery* moves to `sysinfo`; the *signalling* stays `libc`.

### D4: Module shape — `Region`-style free functions, not a trait object

`platform_proc` exposes plain free functions (`listening_ports() -> Vec<u16>`, `port_owner(port) -> Option<PortOwner>`, `process_cwd(pid) -> Option<String>`, `descendants(root) -> Vec<u32>`, `ancestor_env(keys, max_depth) -> Option<String>`, `kernel_version() -> Option<String>`). No trait/dyn — there is exactly one impl per build, selected by the crate's cross-platform backends (`sysinfo`/`netstat2` already `#[cfg]` internally). This keeps call sites identical to today (they call a function, get a value) and avoids an abstraction the single-impl-per-OS reality doesn't need. The signalling helpers (`kill_tree`, `kill_pid`) live here too but wrap `libc` under `cfg(unix)`.

### D5: Preserve pure, tested label logic

`browser.rs` already factors `resolve_label(args, exe_base, comm)` and the port-range filter as pure, unit-tested functions. These MOVE unchanged (or stay in `browser.rs` consuming `platform_proc` data). The migration only swaps the *source* of `args`/`exe_base`/`comm`/`cwd` from `/proc` reads to `sysinfo`. Existing tests (`read_listening_user_ports` filter, label resolution) MUST keep passing — they are the Linux no-regression guard.

## Risks / Trade-offs

- **[`netstat2` macOS pid association unreliable]** → Validate in a focused spike before wide migration (D2a). Fallback: `listeners` crate, or last-resort `cfg(macos)` `libproc` for the owner lookup only. The port *list* is lower-risk than the *owner* resolution.
- **[`sysinfo` snapshot cost in the 3s scanner]** → Refresh only processes once per scan and reuse the snapshot for all owner lookups in that tick; do not construct a new `System` per port. Net work is comparable to today's per-port `/proc/<pid>/fd` directory walks (often cheaper — one snapshot vs N directory scans).
- **[Behaviour drift in `kill_process_tree` semantics]** → Keep `libc::kill(-pgid)` + per-pid SIGTERM ordering (descendants reversed, then root group, then root) identical; only the descendant *enumeration* changes source. Cover with the existing BUG-06 manual check on Linux + the macOS acceptance walk.
- **[`sysinfo` permission/visibility differs on macOS]** → macOS may restrict `cwd`/`cmd` for processes the user doesn't own; our callers only inspect user-owned processes (own shells, own dev servers), matching the existing "only the user's own processes are readable" assumption in `browser.rs`. `None`-on-failure contract absorbs the rest.
- **[New dependencies enlarge the build]** → `sysinfo` + `netstat2` are widely-used, `cfg`-internally-gated crates; acceptable for collapsing three hand-rolled subsystems and unblocking two dead-on-macOS features. Removes more `/proc` string-parsing than it adds.
- **[`libc` not yet `cfg(unix)`]** → Hard dependency on sibling `platform-compile`. Sequence `platform-proc` after `platform-compile` lands the `cfg(unix)` move + CI target gate; until then macOS won't compile regardless.

## Migration Plan

1. Land `platform-compile` first (libc → `cfg(unix)`, `cargo check --target` gate) — prerequisite.
2. Add `sysinfo` + chosen listener crate; spike `netstat2` macOS owner resolution (D2a).
3. Introduce `platform_proc` with the Linux impl mirroring today's `/proc` reads exactly; migrate one caller at a time (`updater.rs` kernel version → `shim.rs` ancestor env → `pty.rs` cwd/tree → `browser.rs` ports). After each, run the full check; Linux output must be unchanged.
4. Delete the now-dead `#[cfg(not(target_os = "linux"))]` stubs and the direct `/proc` reads.
5. Validate on macOS: ports chip, quake cwd, Codex env recovery, diagnostics kernel string.

**Rollback:** the change is per-caller; if a caller regresses on Linux, revert that caller to its `/proc` read (kept in git history) without touching the others.

## Open Questions

- **D2a spike outcome**: does `netstat2` reliably associate owning pids for LISTEN sockets on macOS, or do we fall back to `listeners` / a `cfg(macos)` `libproc` owner lookup? Resolve before migrating `browser.rs`.
- Whether to expose a single reusable `sysinfo::System` snapshot to the 3s scanner (cached, refreshed each tick) vs per-call refresh — decide from the spike's measured cost.
- Whether `process_label`'s Chromium/Electron `arg0 == "exe"` special-case (today reads `/proc/<pid>/exe`) maps cleanly to `sysinfo.exe()` on macOS or needs a `cfg`-specific tweak (macOS app bundles differ). Low stakes — falls back to `comm`/name.
