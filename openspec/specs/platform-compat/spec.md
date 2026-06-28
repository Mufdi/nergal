# platform-compat Specification

## Purpose
TBD - created by archiving change platform-compile. Update Purpose after archive.
## Requirements
### Requirement: libc scoped to unix targets

The `libc` crate dependency in `Cargo.toml` SHALL be declared under `[target.'cfg(unix)'.dependencies]`, not `[target.'cfg(target_os = "linux")'.dependencies]`. This makes every ungated `libc::*` call site — which are all valid POSIX calls — available on all unix-family targets including macOS without any source-level changes.

#### Scenario: All ungated libc call sites compile on macOS

- **WHEN** the crate is compiled for `aarch64-apple-darwin`
- **THEN** every existing ungated `libc::*` call site SHALL compile without errors — including but not limited to `libc::getuid` (`lib.rs:674`), `libc::statvfs` (`mcp/worktree_sessions.rs:604`), `libc::kill` (`browser.rs:411`, `pty.rs:64,109-114`), `libc::dup2` (`lib.rs:82,88`), `libc::setsid` (`obsidian/post_session.rs`, `pty.rs:253`), and `libc::localtime_r` (`obsidian/channels.rs:228`)

#### Scenario: Linux behavior unchanged

- **WHEN** the crate is compiled for `x86_64-unknown-linux-gnu`
- **THEN** all `libc::*` call sites SHALL compile and behave identically to before this change

#### Scenario: statvfs field-width difference handled portably

- **WHEN** `free_disk_bytes` (`mcp/worktree_sessions.rs`) multiplies `statvfs.f_bavail` by `f_frsize` and the crate is compiled for `aarch64-apple-darwin`
- **THEN** the multiplication SHALL widen both operands to `u64` (Darwin types `f_bavail` as `u32`, `f_frsize` as `u64`; Linux types both as `u64`), so it typechecks and never truncates on either target — the one libc site that required a source edit beyond the dep-scope move

---

### Requirement: Binary-lookup uses the which crate

The crate SHALL NOT invoke `Command::new("which")` to locate executables. All binary-lookup call sites SHALL use `which::which()` from the `which = "6"` crate, which is cross-platform and does not depend on a `which` binary existing in `PATH`.

#### Scenario: Binary lookup compiles on macOS

- **WHEN** `commands.rs` is compiled for `aarch64-apple-darwin`
- **THEN** the three former `Command::new("which")` sites (lines 179, 185, 2026) SHALL compile and resolve binaries correctly via `which::which()`

#### Scenario: Binary lookup compiles on Windows (future)

- **WHEN** `commands.rs` is compiled for `x86_64-pc-windows-msvc`
- **THEN** `which::which()` SHALL resolve without requiring a `which.exe` in `PATH`

---

### Requirement: Unconditional D-Bus / Secret-Service deps compile on macOS

The `keyring` and `zbus` dependencies — declared today under unconditional `[dependencies]` and backed by Linux-only D-Bus / Secret-Service — SHALL be made target-conditional so the crate compiles for `aarch64-apple-darwin`. `keyring` SHALL use the `async-secret-service` feature on Linux and the `apple-native` (macOS Keychain) backend on macOS. `zbus` and its sole call site (`updater.rs` `FileManager1.ShowItems` reveal) SHALL be gated `#[cfg(target_os = "linux")]` with a non-Linux stub. Secret storage SHALL remain functional on every target: Secret Service on Linux, Keychain on macOS — no plaintext fallback is introduced by this change.

#### Scenario: keyring + zbus compile on macOS

- **WHEN** the crate is compiled for `aarch64-apple-darwin`
- **THEN** `keyring` SHALL compile against its macOS Keychain backend and `zbus` SHALL be excluded from the build (its call site behind a `#[cfg(target_os = "linux")]` stub), with no D-Bus compile error

#### Scenario: Linux secret storage unchanged

- **WHEN** the crate is compiled and run for `x86_64-unknown-linux-gnu`
- **THEN** `keyring` SHALL use the Secret Service (`async-secret-service`) backend exactly as before, and the ClickUp/Linear token storage and reveal-in-file-manager paths SHALL behave identically

---

### Requirement: opencode config path matches opencode's real macOS location

The opencode adapter's config/data roots SHALL match the directories opencode itself uses on each target. Research (opencode docs + `sst/opencode#8235`) established that opencode resolves these via the `xdg-basedir` package, which keeps the **Linux XDG layout on macOS** — `~/.config/opencode` and `~/.local/share/opencode`, NOT `~/Library/Application Support/opencode` (the Apple path is opencode's org-level *managed* settings, not user config). The adapter therefore SHALL use `~/.config/opencode` + `~/.local/share/opencode` on macOS as well, and SHALL NOT switch to `dirs::config_dir()` (which returns the wrong `~/Library/Application Support` directory on macOS). A WHY-comment SHALL record this so the path is not mistakenly "corrected" to Apple dirs later.

> Correction note: an earlier draft required deriving the macOS root from `dirs::config_dir()`; the task-1 research reversed that — it would have pointed at the wrong directory. This is not a compile blocker (the existing pure-`PathBuf` join compiles on macOS unchanged); it is a correctness requirement.

#### Scenario: Config root on macOS matches opencode (XDG, not Apple dirs)

- **WHEN** `agents/opencode/adapter.rs` is compiled and run for `aarch64-apple-darwin`
- **THEN** the config root SHALL resolve to `~/.config/opencode` (and data to `~/.local/share/opencode`), NOT `dirs::config_dir()`/`~/Library/Application Support/opencode`

#### Scenario: Config root on Linux unchanged

- **WHEN** `agents/opencode/adapter.rs` is compiled for `x86_64-unknown-linux-gnu`
- **THEN** the config root SHALL continue to resolve to `~/.config/opencode` (XDG convention unchanged)

---

### Requirement: CI cross-target compile gate

The CI pipeline SHALL include compile-check jobs for every non-host target on every pull request that modifies files under `src-tauri/`, and SHALL fail the PR if compilation for any target fails:

- **macOS**: `cargo check --target aarch64-apple-darwin` on `macos-latest`.
- **Windows**: `cargo check --target x86_64-pc-windows-msvc` on `windows-latest`. The Windows gate SHALL run on a `windows-latest` runner, not a Linux runner: a Linux host cannot cross-check the Windows MSVC target because the transitive `ring` C crypto dependency's build script requires the MSVC toolchain (`lib.exe`/`cl.exe`) and aborts before the crate type-checks (empirically confirmed 2026-06-28).

With both gates present, the previously-documented blind spot is **closed**: an ungated `std::os::unix::*` / `libc::*` seam — which compiles on macOS because macOS is unix — fails the Windows gate. The CLAUDE.md "Cross-platform invariant" section SHALL document that the macOS and Windows gates are complementary (macOS catches Linux-only regressions; Windows catches ungated-unix seams) so reviewers understand each gate's reach.

#### Scenario: PR that breaks macOS compilation fails CI

- **WHEN** a PR introduces a `target_os = "linux"`-only construct, an unconditional Linux-only dependency, or another macOS-incompatible change under `src-tauri/src/`
- **THEN** the macOS `cross-check` CI job SHALL fail with a compile error for `aarch64-apple-darwin`

#### Scenario: Ungated unix seam IS caught by the Windows gate

- **WHEN** a PR adds a new ungated `std::os::unix::*` import or `libc::` call under `src-tauri/src/`
- **THEN** the macOS gate SHALL still pass (macOS is unix) but the **Windows** gate SHALL fail (`libc` absent, `std::os::unix` absent on `x86_64-pc-windows-msvc`), catching the seam before merge

#### Scenario: PR with no src-tauri changes skips gate

- **WHEN** a PR only modifies files outside `src-tauri/`
- **THEN** both cross-check jobs SHALL either be skipped or pass trivially without running a Rust compile

---

### Requirement: Windows-deferred seams are documented exhaustively

Every `std::os::unix::*` and ungated `libc::*` site in the tree SHALL be recorded under a "Windows-deferred seam registry" (in the macOS-era `platform-compile` design.md) and **executed** by `windows-compile`: each Category-A (ungated) site receives a `#[cfg]` gate; each Category-B (already-gated) site receives a Windows stub branch where its function is reachable from ungated code; Category-C (test-only) sites need no production change. The registry SHALL be re-derived against current code before gating (line numbers drift), and the authoritative current enumeration is `windows-compile/handoff/ungated-unix-analysis.md`.

#### Scenario: Registry is re-derived and executed, not just recorded

- **WHEN** `windows-compile` is implemented
- **THEN** every Category-A site in `ungated-unix-analysis.md` SHALL be gated, and the Windows gate SHALL confirm no ungated unix seam remains — the registry transitions from "documented for a future iteration" to "gated and CI-enforced"

#### Scenario: Re-derivation catches drift and the ungated-libc class the macOS registry missed

- **WHEN** the enumeration is re-run against current code
- **THEN** it SHALL include ungated `libc::*` sites (e.g. `lib.rs` `libc::getuid()`, `mcp/worktree_sessions.rs` `libc::statvfs`) that the macOS-era registry omitted because it tracked only `std::os::unix::*` — these compile on macOS (libc present) and fail only on Windows

### Requirement: Ungated Unix seams compile-exclude on Windows

Every production reference to `libc::*`, `std::os::unix::*`, or a Unix-only `std` API (`tokio::net::UnixListener`/`UnixStream`, `PermissionsExt`, `OpenOptionsExt::mode`, `OsStrExt`, `CommandExt::pre_exec`, `AsRawFd`/`RawFd`, `peer_cred`) SHALL be reachable on `x86_64-pc-windows-msvc` only through a `#[cfg(unix)]` / `#[cfg(target_os = "…")]` compile gate, never an ungated reference and never a runtime `if cfg!(unix)` whose Unix-only `use` is type-checked on all targets. Where the enclosing function is reachable from ungated code, a `#[cfg(windows)]` counterpart SHALL exist — either a cross-platform alternative, a synchronous stub type implementing `std::io::Read + Write` (for the `sync_connect` blocking path), or a stub returning `io::ErrorKind::Unsupported`. The complete enumeration of affected sites is `handoff/ungated-unix-analysis.md` (23 ungated production sites after the iprev re-derivation that added the MCP shim entry cluster).

#### Scenario: Crate type-checks for the Windows target

- **WHEN** `cargo check --target x86_64-pc-windows-msvc` is run on a host with the MSVC toolchain
- **THEN** the crate SHALL type-check with no `unresolved import std::os::unix`, no "cannot find crate `libc`", and no Unix-only-type-in-signature error — every site from `ungated-unix-analysis.md` Category A is gated

#### Scenario: Runtime cfg macro replaced by compile gate

- **WHEN** `commands.rs` is compiled for `x86_64-pc-windows-msvc`
- **THEN** the executable-bit probe SHALL use a `#[cfg(unix)]` / `#[cfg(not(unix))]` block (not `if cfg!(unix) { use std::os::unix::fs::PermissionsExt; … }`), so the Unix-only import is not type-checked on Windows; on Windows the probe SHALL fall back to `is_file`

#### Scenario: Latent broken not(unix) stub fixed

- **WHEN** the crate is compiled for `x86_64-pc-windows-msvc`
- **THEN** the `mcp/transport.rs` `peer_uid` stub SHALL NOT reference a `UnixStream` parameter type that is absent on Windows — the MCP-transport gating SHALL resolve the `#[cfg(not(unix))]` branch that never compiled under the macOS (unix) gate

#### Scenario: Ungated MCP shim entry is gated with a Windows stub

- **WHEN** the crate is compiled for `x86_64-pc-windows-msvc`
- **THEN** `mcp::shim::run` (the `nergal mcp` subcommand body, called ungated from `main.rs`) SHALL resolve — `run`/`run_async` gated `#[cfg(unix)]` and a `#[cfg(windows)] pub fn run() -> anyhow::Result<()>` stub present — so the ungated caller compiles and the shim degrades cleanly on Windows until `windows-ipc`

#### Scenario: sync_connect Windows stub satisfies its blocking callers

- **WHEN** the crate is compiled for `x86_64-pc-windows-msvc`
- **THEN** the `#[cfg(windows)]` `platform::sync_connect` SHALL return an `io::Result<T>` whose `T` implements `std::io::Read + Write` (a never-constructed newtype, not `UnixStream`), so the synchronous hook-CLI callers (`hooks/cli.rs`) that read/write the returned stream type-check; the function itself SHALL return `Err(Unsupported)` at runtime until `windows-ipc`

#### Scenario: Linux and macOS behavior unchanged

- **WHEN** the crate is compiled for `x86_64-unknown-linux-gnu` or `aarch64-apple-darwin`
- **THEN** every gated site SHALL compile and behave identically to before this change — the gating is additive (`#[cfg(unix)]` selects the existing body on both unix targets)

---

### Requirement: keyring uses the Windows Credential Manager backend

The `keyring` dependency SHALL be declared with a per-target backend for Windows: `[target.'cfg(target_os = "windows")'.dependencies.keyring]` using the `windows-native` feature (Windows Credential Manager). This mirrors the per-target backends already in place (`async-secret-service` on Linux, `apple-native` on macOS). Secret storage SHALL remain functional on every target with no plaintext fallback introduced; the `keyring::Entry::new(service, user)` call shape SHALL be unchanged across all four call sites (`linear/auth.rs`, `clickup/auth.rs`, `mcp/summary/secret.rs`, `migrate_legacy.rs`).

#### Scenario: keyring compiles against Credential Manager on Windows

- **WHEN** the crate is compiled for `x86_64-pc-windows-msvc`
- **THEN** `keyring` SHALL compile against its `windows-native` backend and the four `Entry::new(service, user)` call sites SHALL compile unchanged

#### Scenario: Linux and macOS secret backends unchanged

- **WHEN** the crate is compiled for `x86_64-unknown-linux-gnu` or `aarch64-apple-darwin`
- **THEN** `keyring` SHALL use its existing backend (Secret Service / Keychain respectively) with a byte-identical feature set, and ClickUp/Linear token storage SHALL behave identically

---

