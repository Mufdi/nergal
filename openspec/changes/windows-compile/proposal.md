## Why

macOS shipped; Windows is the next target. Nergal does not compile for `x86_64-pc-windows-msvc` today: the `libc` crate is scoped to `cfg(unix)` (absent on Windows) and `std::os::unix::*` does not exist there, yet **20 production call sites reference these ungated**. The macOS CI gate cannot catch them — macOS *is* a unix target, so an ungated `libc::getuid()` or `std::os::unix::net::UnixStream` compiles fine on `aarch64-apple-darwin` and only fails on Windows. This change makes the crate **compile** for Windows and adds the CI gate that keeps it that way. It is the foundation every other Windows change builds on.

**Honest scope of the gate (empirical, this iteration):** a `cargo check --target x86_64-pc-windows-msvc` **cannot run on a Linux host** — the transitive C crypto dep `ring v0.17` has a build script that needs the MSVC toolchain (`lib.exe`/`cl.exe`) and aborts before our crate type-checks (verified 2026-06-28; the fake-toolchain hack that worked for the macOS objc stack does not work cleanly for ring's archive step). The Windows gate therefore runs on a **`windows-latest`** runner, mirroring the macOS port's `macos-latest` decision (platform-compat D5). With BOTH the macOS and Windows gates in CI, the previously-documented "ungated-unix seam is not caught" limitation is **closed**: a Windows-breaking ungated `std::os::unix`/`libc` seam now fails the Windows gate.

This change makes Windows **compile**, not **work**: the hook server and MCP daemon are compile-gated to `Unsupported` stubs on Windows (their real named-pipe transport is `windows-ipc`, the next change). Process/port introspection, desktop integration, and bundling are the later sibling changes.

## What Changes

- **Gate the 20 ungated Unix call sites** (enumerated exhaustively in `handoff/ungated-unix-analysis.md`, re-derived from current code) so they compile-exclude on Windows, each with a Windows stub or cross-platform alternative where the enclosing function is reachable from ungated code. Clusters:
  - **MCP transport + shim** (`mcp/transport.rs:18,70-71,82,95,127-129`, `mcp/mod.rs:587,619,671`, `mcp/shim.rs:20,25,44,115`, `lib.rs:700` daemon spawn + `libc::getuid()`): gate the `UnixSocketTransport` type and its consumers behind `#[cfg(unix)]`; gate the daemon spawn and the **shim entry `shim::run`** (`main.rs:135` is an ungated caller — flagged by iprev #1) with a `#[cfg(windows)]` stub for `shim::run` so the `nergal mcp` subcommand resolves. The real Windows named-pipe body is `windows-ipc`.
  - **Hook server** (`hooks/server.rs:7,222` ungated `UnixListener`): same compile-gate treatment; functional in `windows-ipc`.
  - **Disk free** (`mcp/worktree_sessions.rs:601,608-609` `OsStrExt` + `libc::statvfs`): gate `free_disk_bytes` `#[cfg(unix)]` + a `#[cfg(windows)]` stub (functional Windows disk-free is a noted follow-up; not a compile concern).
  - **Token fallback files** (`clickup/auth.rs:155,170`, `linear/auth.rs:233,252` `OpenOptionsExt.mode(0o600)`): gate the mode-set `#[cfg(unix)]`; on Windows the rare keyring-failure fallback file relies on the default per-user ACL.
  - **Executable-bit probe** (`commands.rs:220`): replace `if cfg!(unix) { use std::os::unix… }` — a **runtime** bool macro whose `std::os::unix` import is type-checked on every target — with a proper `#[cfg(unix)]` / `#[cfg(not(unix))]` block.
- **Fix the latent broken Windows stub** at `mcp/transport.rs:122`: the `#[cfg(not(unix))] pub fn peer_uid(_stream: &UnixStream)` references `UnixStream` from the ungated import — it never compiled because macOS (unix) never activated the `cfg(not(unix))` branch. Resolved as part of the MCP-transport gating.
- **`keyring` Windows backend** — add a `[target.'cfg(target_os = "windows")'.dependencies.keyring]` block (`version = "3"`, `features = ["windows-native"]`) for the Windows Credential Manager, mirroring the existing macOS block's style (`features = ["apple-native"]`). **Security-relevant, made in-scope, not deferred** — no plaintext fallback is introduced. Linux (`async-secret-service`) and macOS (`apple-native`) feature sets stay untouched.
- **Verify the unconditional native GUI stack compiles for Windows** — `arboard` (clipboard), `wezterm-term`/`termwiz` (terminal), `tao`/`wry` (Tauri windowing) are Tauri's supported Windows stack; the CI gate confirms. Any per-target gating they force is resolved empirically by the gate, not pre-guessed.
- **Add the CI Windows gate** — `cargo check --target x86_64-pc-windows-msvc` on `windows-latest` on every PR touching `src-tauri/**`.
- **Update the `CLAUDE.md` "Cross-platform invariant" section** — the Windows gate now exists; the macOS+Windows gate pair closes the ungated-unix-seam blind spot the macOS-only text documented as open.

## Capabilities

### New Capabilities

(none)

### Modified Capabilities

- `platform-compat`: extends the cross-platform-compile contract to `x86_64-pc-windows-msvc`. Adds Windows-compile requirements (ungated-seam gating, `keyring` Windows backend, the Windows CI gate on `windows-latest`) and **modifies** the two requirements that explicitly deferred Windows — "CI cross-target compile gate" (now a macOS **and** Windows gate) and "Windows-deferred seams documented exhaustively" (the registry's compile-gating is now executed for Windows, not just recorded).

## Impact

- **`src-tauri/Cargo.toml`**: new `[target.'cfg(target_os = "windows")'.dependencies.keyring]` (`windows-native`). `libc` (`cfg(unix)`) and `zbus` (`cfg(target_os = "linux")`) already exclude Windows — no change.
- **`src-tauri/src/mcp/transport.rs`, `mcp/mod.rs`, `mcp/shim.rs`, `lib.rs`**: `#[cfg(unix)]` gating of the `UnixSocketTransport` type + consumers + daemon spawn, `#[cfg(windows)]` `Unsupported` stubs; the broken `cfg(not(unix))` peer_uid stub fixed.
- **`src-tauri/src/hooks/server.rs`**: hook-server bind gated `#[cfg(unix)]` + Windows stub.
- **`src-tauri/src/mcp/worktree_sessions.rs`**: `free_disk_bytes` gated + Windows stub.
- **`src-tauri/src/clickup/auth.rs`, `linear/auth.rs`**: `.mode(0o600)` gated `#[cfg(unix)]`.
- **`src-tauri/src/commands.rs:220`**: `if cfg!(unix)` → `#[cfg]` block.
- **`.github/workflows/ci.yml`**: new `windows-check` job (`cargo check --target x86_64-pc-windows-msvc` on `windows-latest`).
- **`CLAUDE.md`**: "Cross-platform invariant" section updated for the Windows gate.
- **Out of scope (sibling changes)**: real named-pipe transport + FIFO migration (`windows-ipc`), functional process/port introspection (`windows-proc`), desktop-integration detach/reveal (`windows-desktop`), `.msi`/NSIS bundling + signing (`windows-bundle-ci`).
