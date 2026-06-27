# Tasks — platform-proc

Phased per the implementation plan. Each phase is independently verifiable with the full check (`cargo clippy -- -D warnings && cargo test && cargo fmt --check && npx tsc --noEmit`). Migrate callers low-risk → high-risk so a Linux regression is isolated to one caller.

## 1. Prerequisites + dependencies + spike

- [ ] 1.1 Confirm sibling `platform-compile` has landed `libc` → `cfg(unix)` and a `cargo check --target` CI gate; block if not. Do NOT re-add `libc` in this change.
- [ ] 1.2 Add `sysinfo` to `src-tauri/Cargo.toml` (needed regardless of the listener-crate outcome). Run `cargo build`.
- [ ] 1.3 D2a spike (run BEFORE committing the listener crate, so the dep is added once with the verdict already known): (a) verify `netstat2` reliably associates owning pids for LISTEN sockets on macOS — if unreliable, choose `listeners` or a `cfg(macos)` `libproc` owner-only lookup; (b) verify `sysinfo.environ()` reliably reads a same-uid ancestor's environment on macOS (the Codex-recovery path) — if unreadable, record it so the requirement surfaces the limitation rather than a silent `None`; (c) measure `sysinfo` snapshot cost for the 3s scan cadence. Record all three verdicts in design Open Questions.
- [ ] 1.4 Add the listener crate chosen by 1.3(a) (`netstat2` or `listeners`) to `Cargo.toml`. Run `cargo build`.

## 2. New platform_proc module (Linux-parity first)

- [ ] 2.1 Create `src-tauri/src/platform_proc.rs` (or `platform_proc/mod.rs`); register `mod platform_proc;` in `lib.rs`.
- [ ] 2.2 Implement `kernel_version`, `os_name`, `process_cwd`, `descendants`, `ancestor_env` cross-platform via `sysinfo`; `kill_tree` + `kill_pid` via `libc` under `cfg(unix)` (preserve the `kill(-pgid)` + per-pid SIGTERM ordering from `pty.rs:109-114`). `os_name` wraps `sysinfo::System::long_os_version()` with `name()`+`os_version()` then `"unknown"` fallback (D6).
- [ ] 2.3 Implement `listening_ports` (LISTEN TCP v4+v6, dedup, user-port-range filter, sorted) and `port_owner` (owner pid + cmd/exe/cwd for the kept label logic) via the chosen listener crate + `sysinfo`.
- [ ] 2.4 Unit tests for the pure parts of the module (descendant-set BFS excluding root/pid<=1; port-range filter; ancestor-env key precedence). Full check green.

## 3. Migrate callers (full check after each)

- [ ] 3.1 `updater.rs` `collect_diagnostics` (BOTH reassigned reads): kernel field (`:441`, `/proc/sys/kernel/osrelease`) → `platform_proc::kernel_version()`; OS field (`:440`, currently `read_os_pretty_name()` parsing `/etc/os-release`) → `platform_proc::os_name()`. Each keeps the `"unknown"` fallback. Delete `read_os_pretty_name` once unused.
- [ ] 3.2 `mcp/shim.rs` → replace `parent_pid` + both `session_hint_from_ancestors` cfgs with `platform_proc::ancestor_env(&["NERGAL_SESSION_ID","CLAUDE_CODE_SESSION_ID"], 8)`; keep/relocate the tested `find_in_environ`. Delete the non-Linux stub.
- [ ] 3.3 `pty.rs` → `Drop` uses `platform_proc::kill_tree` for all unix; delete `kill_process_tree` and both `process_cwd` defs (incl. the dead `cfg(not(linux))` stub); point `live_session_cwds:184` and the aux-shell tracker `:1332` at `platform_proc::process_cwd`.
- [ ] 3.4 `browser.rs` → scan entry `:93` via `platform_proc::listening_ports`; `port_process_info:387` via `platform_proc::port_owner` feeding the KEPT pure `resolve_label`; `kill_port:411` via `platform_proc::kill_pid` (now `cfg(unix)`). Remove `PROC_NET_TCP*` consts, `listen_inode_for_port`, `pid_for_socket_inode`, `process_label`, `process_cwd_name` once unused. Keep `resolve_label`/`basename`/`strip_script_ext`/`INTERPRETERS` + their tests.

## 4. Cleanup

- [ ] 4.1 Assert no direct `/proc` read remains outside `platform_proc`: `grep -rn "/proc" src-tauri/src/` matches only the Linux arm inside the new module. **The `grep "/proc"` gate does NOT cover `read_os_pretty_name`** (it reads `/etc/os-release`, not `/proc`) — separately assert `grep -rn "os-release\|read_os_pretty_name" src-tauri/src/` returns nothing outside `platform_proc` so the OS-name migration is not silently skipped.
- [ ] 4.2 Remove now-unused imports; `cargo fmt`; resolve all clippy warnings.

## 5. Verification

- [ ] 5.1 Full check green on Linux: `cd src-tauri && cargo clippy -- -D warnings && cargo test && cargo fmt --check && cd .. && npx tsc --noEmit`.
- [ ] 5.2 Cross-platform compile: `cargo check --target aarch64-apple-darwin` (and/or x86_64-apple-darwin) passes.
- [ ] 5.3 Linux no-regression manual walk: ports chip labels/projects correct; `kill_port` frees an owned port; quake aux-shell cwd resolves on Enter; BUG-06 — close a session whose shell spawned `pnpm dev` in a new process group, server dies; Codex MCP session attribution intact; diagnostics show BOTH a kernel version AND the distro `OS:` name (the `OS:` string must stay a meaningful distribution string, not regress to `"unknown"` or a bare kernel string; byte-identity with the old `PRETTY_NAME` is NOT required).
- [ ] 5.4 macOS acceptance: ports chip discovers a `pnpm dev` listener (label + project); quake-shell cwd resolves; cross-session Codex env recovery returns the session id from an ancestor (or, if the spike found `environ` unreadable, the documented limitation holds); diagnostics `Kernel:` AND `OS:` fields are both non-`unknown` (`OS:` shows a `macOS …` string).

> **Coverage limit (stated, not a gap):** macOS behavior is verified only by the 5.4 manual walk. The CI `cargo check --target` gate compiles but does not execute macOS code (no macOS test runner in scope until `platform-bundle-ci`). Automated macOS execution coverage is explicitly out of scope here.
