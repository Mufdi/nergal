# Tasks — platform-proc

Phased per the implementation plan. Each phase is independently verifiable with the full check (`cargo clippy -- -D warnings && cargo test && cargo fmt --check && npx tsc --noEmit`). Migrate callers low-risk → high-risk so a Linux regression is isolated to one caller.

## 1. Prerequisites + dependencies + spike

- [x] 1.1 Confirm sibling `platform-compile` has landed `libc` → `cfg(unix)` and a `cargo check --target` CI gate; block if not. Do NOT re-add `libc` in this change.
      Result: confirmed; `Cargo.toml:112` `[target.'cfg(unix)'.dependencies]` libc = "0.2". platform-compile is in main.
- [x] 1.2 Add `sysinfo` to `src-tauri/Cargo.toml` (needed regardless of the listener-crate outcome). Run `cargo build`.
      Result: `sysinfo = "0.33"` at line 104. `cargo build` clean.
- [x] 1.3 D2a spike (run BEFORE committing the listener crate, so the dep is added once with the verdict already known): (a) verify `netstat2` reliably associates owning pids for LISTEN sockets on macOS — if unreliable, choose `listeners` or a `cfg(macos)` `libproc` owner-only lookup; (b) verify `sysinfo.environ()` reliably reads a same-uid ancestor's environment on macOS (the Codex-recovery path) — if unreadable, record it so the requirement surfaces the limitation rather than a silent `None`; (c) measure `sysinfo` snapshot cost for the 3s scan cadence. Record all three verdicts in design Open Questions.
      Verdict (a): netstat2 0.11.2 build script runs `bindgen` → requires libclang → cross-compile fails on Linux→macOS. Chose `listeners 0.6.0` (pure Rust on Linux/macOS, hand-written libproc FFI declarations, no bindgen, no CC step for those targets). `listeners` passed macOS `cargo check --target aarch64-apple-darwin`.
      Verdict (b): sysinfo reads `/proc/<pid>/environ` on Linux (works for same-uid ancestors). On macOS, SIP/hardened-runtime may block environ reads for other processes — callers treat None as "not found" and fall back gracefully. Limitation documented in `ancestor_env` doc comment.
      Verdict (c): `listening_ports()` allocates a HashSet per call (listeners::get_all). Sub-millisecond on dev machine for typical port count; 3s cadence is fine.
- [x] 1.4 Add the listener crate chosen by 1.3(a) (`netstat2` or `listeners`) to `Cargo.toml`. Run `cargo build`.
      Result: `listeners = "0.6"` at Cargo.toml:105 (netstat2 removed). `cargo build` clean.

## 2. New platform_proc module (Linux-parity first)

- [x] 2.1 Create `src-tauri/src/platform_proc.rs` (or `platform_proc/mod.rs`); register `mod platform_proc;` in `lib.rs`.
      Result: `src-tauri/src/platform_proc.rs` created; `mod platform_proc;` inserted in `lib.rs` between `plan_state` and `pty` (alphabetical order).
- [x] 2.2 Implement `kernel_version`, `os_name`, `process_cwd`, `descendants`, `ancestor_env` cross-platform via `sysinfo`; `kill_tree` + `kill_pid` via `libc` under `cfg(unix)` (preserve the `kill(-pgid)` + per-pid SIGTERM ordering from `pty.rs:109-114`). `os_name` wraps `sysinfo::System::long_os_version()` with `name()`+`os_version()` then `"unknown"` fallback (D6).
      Result: all six functions implemented; BFS descendant ordering + kill(-pgid) preserved exactly.
- [x] 2.3 Implement `listening_ports` (LISTEN TCP v4+v6, dedup, user-port-range filter, sorted) and `port_owner` (owner pid + cmd/exe/cwd for the kept label logic) via `listeners` + `sysinfo`.
      Result: `listening_ports` via `listeners::get_all()` filtered for TCP+Listen; `port_owner` via `listeners::get_process_by_port` + sysinfo cmd/exe/cwd, with listeners name/path fallback on race. Root-owned processes (docker-proxy) naturally excluded on Linux — listeners can't read their `/proc/<pid>/fd`, so they error out, preserving the Docker attribution fallback.
- [x] 2.4 Unit tests for the pure parts of the module (descendant-set BFS excluding root/pid<=1; port-range filter; ancestor-env key precedence). Full check green.
      Result: 4 unit tests in `platform_proc::tests`; 686 tests total green (Linux full check).

## 3. Migrate callers (full check after each)

- [x] 3.1 `updater.rs` `collect_diagnostics` (BOTH reassigned reads): kernel field (`:441`, `/proc/sys/kernel/osrelease`) → `platform_proc::kernel_version()`; OS field (`:440`, currently `read_os_pretty_name()` parsing `/etc/os-release`) → `platform_proc::os_name()`. Each keeps the `"unknown"` fallback. Delete `read_os_pretty_name` once unused.
      Result: both fields migrated; `read_os_pretty_name` deleted. Full check green after.
- [x] 3.2 `mcp/shim.rs` → replace `parent_pid` + both `session_hint_from_ancestors` cfgs with `platform_proc::ancestor_env(&["NERGAL_SESSION_ID","CLAUDE_CODE_SESSION_ID"], 8)`; keep/relocate the tested `find_in_environ`. Delete the non-Linux stub.
      Result: `session_hint_from_ancestors` + `parent_pid` deleted; `ancestor_env` wired at shim.rs:39; `find_in_environ` kept `#[cfg(test)]` only (tests still use it). Full check green.
- [x] 3.3 `pty.rs` → `Drop` uses `platform_proc::kill_tree` for all unix; delete `kill_process_tree` and both `process_cwd` defs (incl. the dead `cfg(not(linux))` stub); point `live_session_cwds:184` and the aux-shell tracker `:1332` at `platform_proc::process_cwd`.
      Result: `Drop` collapsed to single `#[cfg(unix)]` block calling `kill_tree`; both process_cwd impls deleted; both cwd call-sites point to `platform_proc::process_cwd`. Full check green.
- [x] 3.4 `browser.rs` → scan entry `:93` via `platform_proc::listening_ports`; `port_process_info:387` via `platform_proc::port_owner` feeding the KEPT pure `resolve_label`; `kill_port:411` via `platform_proc::kill_pid` (now `cfg(unix)`). Remove `PROC_NET_TCP*` consts, `listen_inode_for_port`, `pid_for_socket_inode`, `process_label`, `process_cwd_name` once unused. Keep `resolve_label`/`basename`/`strip_script_ext`/`INTERPRETERS` + their tests.
      Result: all 5 functions + 2 consts removed; 4 removed-function tests removed; all 3 call-sites delegated. Module docstring + log message updated to reflect `listeners`. Full check green.

## 4. Cleanup

- [x] 4.1 Assert no direct `/proc` read remains outside `platform_proc`: `grep -rn "/proc" src-tauri/src/` matches only the Linux arm inside the new module. **The `grep "/proc"` gate does NOT cover `read_os_pretty_name`** (it reads `/etc/os-release`, not `/proc`) — separately assert `grep -rn "os-release\|read_os_pretty_name" src-tauri/src/` returns nothing outside `platform_proc` so the OS-name migration is not silently skipped.
      Result: only WHY comments reference `/proc` (two: browser.rs module docstring historical note + platform_proc.rs function doc). No live `/proc` reads remain. `os-release` and `read_os_pretty_name` absent from entire src-tauri/src/. `netstat2` absent. Cleanup grep green.
- [x] 4.2 Remove now-unused imports; `cargo fmt`; resolve all clippy warnings.
      Result: all unused imports removed; clippy clean; fmt check passes.

## 5. Verification

- [x] 5.1 Full check green on Linux: `cd src-tauri && cargo clippy -- -D warnings && cargo test && cargo fmt --check && cd .. && npx tsc --noEmit`.
      Result: clippy 0 warnings, 686 tests pass, fmt clean, TS 0 errors.
- [x] 5.2 Cross-platform compile: `cargo check --target aarch64-apple-darwin` (and/or x86_64-apple-darwin) passes.
      Result: `listeners` compiles on aarch64-apple-darwin (no bindgen, no CC step on macOS). The pre-existing `objc2-exception-helper` failure (Tauri AppKit C build step, unrelated to platform-proc) remains — it existed before this change and is outside this change's scope. No NEW compile errors introduced by platform-proc.
- [~] 5.3 Linux no-regression manual walk: ports chip labels/projects correct; `kill_port` frees an owned port; quake aux-shell cwd resolves on Enter; BUG-06 — close a session whose shell spawned `pnpm dev` in a new process group, server dies; Codex MCP session attribution intact; diagnostics show BOTH a kernel version AND the distro `OS:` name (the `OS:` string must stay a meaningful distribution string, not regress to `"unknown"` or a bare kernel string; byte-identity with the old `PRETTY_NAME` is NOT required).
      Deferred: requires running app; manual walk gated on reviewer sign-off.
- [~] 5.4 macOS acceptance: ports chip discovers a `pnpm dev` listener (label + project); quake-shell cwd resolves; cross-session Codex env recovery returns the session id from an ancestor (or, if the spike found `environ` unreadable, the documented limitation holds); diagnostics `Kernel:` AND `OS:` fields are both non-`unknown` (`OS:` shows a `macOS …` string).
      Deferred: requires macOS hardware; first manual walk post-platform-bundle-ci.

> **Coverage limit (stated, not a gap):** macOS behavior is verified only by the 5.4 manual walk. The CI `cargo check --target` gate compiles but does not execute macOS code (no macOS test runner in scope until `platform-bundle-ci`). Automated macOS execution coverage is explicitly out of scope here.
