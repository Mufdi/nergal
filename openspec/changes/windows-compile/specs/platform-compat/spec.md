## ADDED Requirements

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

## MODIFIED Requirements

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
