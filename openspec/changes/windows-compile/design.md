# Design — windows-compile

## Context

macOS is shipped and archived. The macOS port established the pattern: a `cargo check --target` CI gate, `#[cfg]`-gated seams with stubs, per-target dependency backends. Windows is fundamentally harder than macOS was, for one reason that drives this whole change:

**`#[cfg(unix)]` is TRUE on macOS but FALSE on Windows.** The macOS port could leave the entire unix surface untouched (it compiles on macOS) and only fix the handful of Linux-*only* deps (`zbus`, `keyring` feature). Windows has neither `libc` (scoped `cfg(unix)`) nor `std::os::unix`, so **every** ungated unix reference is a Windows compile error. The macOS CI gate is structurally blind to this class — an ungated `libc::getuid()` type-checks on `aarch64-apple-darwin`. This change finds and gates all of them, and adds the Windows gate that makes the blind spot impossible going forward.

This change is **compile-only**. After it, Windows builds; the hook server and MCP daemon are `Unsupported` stubs there until `windows-ipc` (the next change) supplies the named-pipe body. Functional process introspection, desktop integration, and bundling are the later siblings.

## Empirical pre-flight (RUN 2026-06-28, ground truth)

Two facts were established before designing, mirroring the macOS task-1 pre-flight:

1. **Runner — D1 RESOLVED to `windows-latest`.** `rustup target add x86_64-pc-windows-msvc` succeeds (rust-std is precompiled), but `cargo check --target x86_64-pc-windows-msvc` on the Linux dev host **aborts in `ring v0.17`'s build script**: `error occurred in cc-rs: failed to find tool "lib.exe"`. `ring` (pulled transitively via the TLS stack) compiles C + assembly and needs the MSVC archiver/compiler. A fake-toolchain shim (stub `cl.exe`/`lib.exe`/`nasm.exe` that emit empty object/lib files — the trick that let the macOS objc stack type-check on Linux) gets `ring` past tool *discovery* but then fails inside cc-rs's "copy or hard-link the generated lib file" step: ring's C build is more involved than the macOS objc `.o` stub. **Conclusion: the Windows gate must run on `windows-latest`** (which has the real MSVC toolchain), exactly as the macOS objc build forced `macos-latest`. A full `clang-cl` + `lld-link` + `xwin`(Windows SDK) cross-toolchain could check locally but is not worth standing up for CI; `windows-latest` is the clean answer and additionally gives real `cargo test` + bundling later (`windows-bundle-ci`).

2. **Ungated-seam set — re-derived exhaustively** (`handoff/ungated-unix-analysis.md`): **20 Category-A (ungated) production sites**, 28 Category-B (already gated), 8 Category-C (test-only). The re-derivation surfaced the class the macOS-era registry structurally could not: **ungated `libc::*`** (`lib.rs:700` `libc::getuid()`, `mcp/worktree_sessions.rs:608-609` `libc::statvfs`). It also found a **latent broken stub**: `mcp/transport.rs:122` `#[cfg(not(unix))] pub fn peer_uid(_stream: &UnixStream)` references the ungated `UnixStream` import — it never compiled because the macOS (unix) build never selected the `cfg(not(unix))` branch.

## Decision 1 — CI Windows gate runs on `windows-latest`

**Chosen**: add a `windows-check` job running `cargo check --target x86_64-pc-windows-msvc` on `windows-latest`, gated to PRs touching `src-tauri/**`, alongside the existing macOS `macos-cross-check` job.

- *Alternative: Linux host + `rustup target add` (no extra runner).* **Rejected empirically** — `ring`'s build script needs the MSVC toolchain; aborts before our crate compiles (pre-flight fact 1).
- *Alternative: `x86_64-pc-windows-gnu` cross-compiled from Linux with mingw-w64.* Cheaper (no Windows runner) and `ring` supports the gnu target. **Rejected**: the shipping target is `-msvc` (Tauri's Windows default); `-gnu` differs in `target_env = "gnu"` vs `"msvc"`, so a gnu-green check is not a faithful proxy for the msvc target we ship, and it would require installing a system toolchain on dev hosts. We gate the real target.
- *Alternative: `clang-cl` + `xwin` Windows SDK cross-toolchain on Linux.* Would enable local + Linux-CI checks. **Rejected for now**: heavy setup (downloads the Windows SDK), and `windows-latest` is needed anyway for `cargo test`/bundling in `windows-bundle-ci`. Recorded as the local-validation option for an implementer without a Windows machine.

## Decision 2 — keyring Windows backend = `windows-native` (Credential Manager)

**Chosen**: `[target.'cfg(target_os = "windows")'.dependencies.keyring]` with `version = "3"`, `features = ["windows-native"]`, mirroring the **style of the existing macOS block** (`features = ["apple-native"]`, no `default-features = false`). Windows Credential Manager is the OS-native secret store; `Entry::new(service, user)` is unchanged across the four call sites.

> Correction (iprev #3): an earlier draft claimed the Linux block pins `async-secret-service` with `default-features = false`. It does not — `Cargo.toml:131-133` is `features = ["async-secret-service", "tokio", "crypto-rust"]` with no `default-features` line (same for macOS). The Windows block therefore matches that no-`default-features` style; keyring's platform backends are mutually exclusive and target-gated internally, so `features = ["windows-native"]` selects the Credential Manager without pulling the Linux/macOS backends on Windows.

- *Alternative: a plaintext/file fallback on Windows.* **Rejected** — security regression; the whole point of the per-target backends (and the rename-GOTCHA lesson about silently losing tokens) is no-plaintext secret storage on every target.
- *Alternative: leave `keyring` unconditional and let it auto-select.* **Rejected** — per-target blocks are the established, working pattern (the Linux block deliberately scopes its feature set to dodge the duplicate-zbus dedupe hazard); an unconditional declaration risks reintroducing cross-target feature bleed.

**Risk** (carried from the macOS D7 lesson): a botched target-table edit can break the **Linux** secret backend. Mitigation: the Windows block is purely additive; Linux/macOS blocks are untouched; the verification group includes a Linux keyring round-trip regression check.

## Decision 3 — Compile-gate the transports now; named-pipe body is `windows-ipc`

The MCP transport (`UnixSocketTransport` + consumers in `mcp/mod.rs`, `mcp/shim.rs`, the daemon spawn in `lib.rs`) and the hook server bind (`hooks/server.rs`) are the bulk of Category A. Two ways to make them compile on Windows:

**Chosen**: gate the ungated transport items `#[cfg(unix)]`. The MCP daemon and hook server are **compile-absent, runtime-unsupported** on Windows until `windows-ipc`. This keeps `windows-compile` purely additive and mechanical (low risk), and concentrates the real, security-sensitive named-pipe transport work in one change (`windows-ipc`) where it gets dedicated iprev + security review.

**Stub surface — minimize it (iprev #5).** A `#[cfg(windows)]` stub is needed only where an **ungated caller** references a gated symbol on Windows. After gating the `lib.rs` daemon-spawn block and the hook-server bind `#[cfg(unix)]`, the only such callers are: `main.rs:135` → `mcp::shim::run` (needs a `#[cfg(windows)] run` stub), the hook-CLI blocking path → `platform::sync_connect` (needs a `#[cfg(windows)]` stub, Decision 6), and `start_hook_server`'s caller in `lib.rs` (gate the caller, or stub). The `UnixSocketTransport` type, `mcp::serve`/`handle_connection`/`write_response`, and `mcp/shim.rs::relay` have **no ungated Windows caller once the daemon block + `shim::run` are gated** — so they are gated `#[cfg(unix)]` **with no Windows stub surface at all**. This removes exactly the cluster most at risk of accidentally naming `UnixStream` in a stub.

**The shim cluster (iprev #1 — initially missed).** `main.rs:135` calls `nergal::mcp::shim::run()` ungated; `shim::run`→`run_async` (`shim.rs:44`) binds `daemon: Option<tokio::net::UnixStream>` via `transport::connect`. The original enumeration listed only `shim.rs:115` (`relay`) and missed the entry. Fix: gate `run`/`run_async` `#[cfg(unix)]` + a `#[cfg(windows)] pub fn run() -> anyhow::Result<()>` stub (prints a degraded note, returns `Ok`), so the `nergal mcp` subcommand resolves and exits cleanly on Windows.

- *Alternative: migrate the hook server + MCP transport onto the existing `platform::PlatformListener`/`PlatformStream` seam now* (which already has `#[cfg(windows)]` stubs). This is the macOS design's stated end-state. **Rejected for this change**: the migration is non-trivial (`mcp/transport.rs` has its own length-framing + `peer_uid` flow distinct from `PlatformListener`), it is functional surgery, and `windows-ipc` will do it anyway when it replaces the stub with the real body. Doing a throwaway migration here doubles the work and dilutes the compile-only risk profile. `windows-compile` gates; `windows-ipc` migrates-and-implements.

**Consequence to record**: after `windows-compile`, a Windows build runs the GUI but the hook pipeline and MCP server are inert (return `Unsupported`). That is expected and is the contract `windows-ipc` fulfills. The sequencing (ipc immediately after compile) keeps the inert window short.

## Decision 4 — `free_disk_bytes` gated with a Windows stub; functional impl deferred

`mcp/worktree_sessions.rs::free_disk_bytes` uses `OsStrExt` + `libc::statvfs`. **Chosen**: gate the unix body `#[cfg(unix)]` and add a `#[cfg(windows)]` stub returning `None`/`0` (the value feeds a worktree disk-space hint, non-critical). A functional Windows impl (`GetDiskFreeSpaceExW`, or `sysinfo`'s cross-platform `Disks`) is a noted follow-up — most naturally folded into `windows-proc` (the introspection change), not forced into this compile-only change.

- *Alternative: switch to `sysinfo::Disks` now (cross-platform, no stub).* Tempting but **functional**: it changes the Linux/macOS code path too, contradicting compile-only scope and risking a behavior delta on the shipped platforms. Deferred to `windows-proc` where disk introspection belongs.

## Decision 5 — Token-fallback `.mode(0o600)` gated; Windows relies on default ACL

`clickup/auth.rs` + `linear/auth.rs` write a `0600` fallback token file only when the keyring is unavailable. **Chosen**: gate the `OpenOptionsExt::mode(0o600)` call `#[cfg(unix)]`; on Windows the (rare) fallback file is created with the default per-user profile ACL. On Windows the keyring backend is Credential Manager (robust), so the fallback path is hit far less than on a headless Linux box, and a tightened DACL is a `windows-desktop`/follow-up concern, not a compile blocker. The fallback already warns the user that file storage is less secure than the keyring.

## Decision 6 — `sync_connect` Windows stub returns a `Read + Write` newtype, not `UnixStream`

`platform::sync_connect` (`platform/mod.rs:820`) returns `io::Result<std::os::unix::net::UnixStream>` and its callers in `hooks/cli.rs` (`:15,46,235,457,509`, with `use std::io::{Read, Write}` at `:2`) drive the returned value with **synchronous** `std::io::Read`/`Write`. A naive `#[cfg(windows)] sync_connect` "returning `Err(Unsupported)`" (iprev #2) does not type-check: the `Ok<T>` type must still exist on Windows AND implement `Read + Write` for the callers to compile.

**Chosen**: under `#[cfg(windows)]`, define a private never-constructed newtype (e.g. `pub struct UnsupportedSyncStream(());`) that implements `std::io::Read` + `std::io::Write` (every method returns `Err(Unsupported)`), and have `sync_connect` return `io::Result<UnsupportedSyncStream>` = always `Err(Unsupported)`. The callers type-check (the trait bounds are satisfied) and the runtime behavior is a clean unsupported error until `windows-ipc` provides the named-pipe sync connect.

- *Alternative: return `io::Result<std::net::TcpStream>`.* `TcpStream` is `Read + Write` and cross-platform, so callers compile. **Rejected** — it invites an implementer to actually `TcpStream::connect` later (a non-named-pipe transport that bypasses the SID auth boundary `windows-ipc` will build); the never-constructed newtype makes "unsupported until windows-ipc" explicit and un-abusable.

## Windows-deferred-to-siblings registry

This change makes Windows **compile**. What it intentionally leaves as `Unsupported` stubs / Linux-only behavior, and which sibling owns the functional Windows impl:

| Surface | Stub left by `windows-compile` | Functional Windows owner |
|---|---|---|
| Hook server transport | `#[cfg(windows)]` `Unsupported` | `windows-ipc` (named pipe) |
| MCP daemon transport | `#[cfg(windows)]` `Unsupported` | `windows-ipc` (named pipe) |
| plan-review / ask-user FIFO | `hooks/cli.rs` `#[cfg(not(unix))]` shell-`mkfifo` placeholder (pre-existing) | `windows-ipc` (PlatformStream) |
| `free_disk_bytes` | `#[cfg(windows)]` returns 0 | `windows-proc` (disk introspection) |
| Process/port enumeration | (owned by `platform-proc`'s seam) | `windows-proc` |
| `setsid`/`pre_exec` detach (obsidian, pty docker) | already `#[cfg(target_os="linux")]` no-op stub | `windows-desktop` |
| Token-fallback file ACL | default profile ACL | `windows-desktop` / follow-up |

## Risks / Trade-offs

- **[Risk] Windows build compiles but is functionally hollow until `windows-ipc`.** Accepted and bounded: `windows-ipc` is the immediate next change; no Windows release ships from `windows-compile` alone (the 3-platform release waits for the whole set).
- **[Risk] `arboard`/`wezterm-term`/`tao`/`wry` force unforeseen per-target gating.** Low — these are Tauri's supported Windows GUI stack. The CI gate surfaces any issue empirically rather than us pre-guessing; if one needs gating, it is an additive Cargo edit caught on the first gate run.
- **[Risk] keyring target-table edit breaks Linux secret storage** (the rename-GOTCHA class). Mitigation: additive-only Windows block + Linux keyring round-trip in the verification group.
- **[Risk] No local validation on the Linux dev host.** Accepted: `windows-latest` CI is the wall, plus the user has a real Windows machine for the eventual walk. The 20-site enumeration is statically vetted (each Category-A site read and confirmed) so the gating is grounded even without a local green check.

## Migration / rollback

All changes are `#[cfg]` attributes, `#[cfg(windows)]` stub fns, one Cargo.toml target block, and one CI job — fully git-revertible. A revert must restore the exact Linux/macOS `keyring` feature sets (not just drop the Windows block), same caveat as the macOS port.
