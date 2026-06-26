## Why

Nergal compiles and runs only on Linux today. Porting to macOS (the first non-Linux target) requires fixing the ungated POSIX/libc call sites **and the unconditional D-Bus/Secret-Service dependencies** that fail to compile on macOS, plus establishing a CI gate that catches macOS-breaking regressions going forward.

**Honest scope of the gate**: a `cargo check --target aarch64-apple-darwin` catches **Linux-only (macOS-breaking)** regressions. Because macOS is itself a unix target, it does **not** catch a newly-added *ungated* `std::os::unix::*` import or `libc::` call — those compile fine on macOS. Detecting ungated-unix seams requires a **Windows** target, which is deferred to the Windows iteration. The gate is the enforcement arm for the macOS port; full unix-seam enforcement is a later milestone, and the spec/CLAUDE.md wording reflects that limit rather than overselling it.

## What Changes

- **Move `libc` dep** from `[target.'cfg(target_os = "linux")'.dependencies]` to `[target.'cfg(unix)'.dependencies]` — macOS is POSIX-compliant; every ungated `libc::*` call site (not just the three first found: `getuid` `lib.rs:674`, `statvfs` `mcp/worktree_sessions.rs:604`, `kill` `browser.rs:411`, but also `dup2` `lib.rs:82,88`, `setsid` in `obsidian/post_session.rs` + `pty.rs:253`, `localtime_r` `obsidian/channels.rs:228`, the `kill` tree in `pty.rs:64,109-114`) is valid POSIX and compiles on macOS once the dep is scoped to `unix`.
- **Replace `Command::new("which")`** in `commands.rs` (179, 185, 2026) with the already-present `which = "6"` crate.
- **Gate the unconditional D-Bus / Secret-Service deps for macOS (guaranteed blockers, made in-scope):**
  - `keyring` is declared under unconditional `[dependencies]` with the Linux-only `async-secret-service` feature. **Feature-flag it per target** — `async-secret-service` on Linux, `apple-native` (macOS Keychain) on macOS (design D7). This is a **secret-storage backend decision made in this plan**, not deferred — it is security-relevant.
  - `zbus` is unconditional and used only in the `FileManager1.ShowItems` reveal path (`updater.rs:329-345`). **Gate the dep and its call site behind `#[cfg(target_os = "linux")]`** so it compiles on macOS now (design D8). `platform-desktop` later removes that call path entirely via the Tauri opener plugin; this change only makes it compile.
- **Fix opencode config paths** in `agents/opencode/adapter.rs` — research opencode's actual macOS config location (do not assume XDG), then resolve via `dirs::config_dir()` on macOS.
- **Record Windows-deferred seams EXHAUSTIVELY** — every `std::os::unix::*` site in the tree, categorized (already-gated vs ungated vs test-only), so the Windows implementer needs no fresh grep (the previous 5-entry list failed its own completeness scenario).
- **Add CI cross-target check job** — `cargo check --target aarch64-apple-darwin` on every PR touching `src-tauri/**`, with the honest enforcement scope above.
- **Add cross-platform invariant convention to `CLAUDE.md`** — including the gate's real reach (catches macOS-breaking; Windows-seam detection deferred).

**Dropped from scope (was D3):** gating `read_os_pretty_name()` (`/etc/os-release`, `updater.rs:406`). A `std::fs::read_to_string` of a Linux path is **not a compile error** on macOS, so gating it does zero work toward this change's compile-only goal and would be a *functional* change contradicting the non-goals. The runtime Linux-path diagnostics reads (`/etc/os-release` `updater.rs:406`, `/proc/sys/kernel/osrelease` `updater.rs:436`) are **reassigned to `platform-proc`** alongside the other procfs reads, where the functional macOS behavior is owned.

## Capabilities

### New Capabilities

- `platform-compat`: Cross-platform compile correctness. Covers the `#[cfg]`-gating convention, `libc` dep-scope correction, `which`-crate policy, per-target `keyring` backend + `zbus` gating, opencode config-path resolution, the exhaustive Windows-deferred seam registry, and the CI cross-target enforcement gate (with its documented reach limit).

### Modified Capabilities

(none — no existing spec-level behavior changes; the `zbus` reveal path is removed by `platform-desktop`, not here)

## Impact

- **`src-tauri/Cargo.toml`**: `libc` dep `linux`→`unix`; `keyring` features split per target (`async-secret-service`/Linux, `apple-native`/macOS); `zbus` dep moved under `[target.'cfg(target_os = "linux")'.dependencies]`.
- **`src-tauri/src/updater.rs:329-345`**: `show_items_via_dbus` + its callsite gated `#[cfg(target_os = "linux")]` with a non-Linux stub (reveal becomes a no-op on macOS until `platform-desktop` replaces it).
- **`src-tauri/src/commands.rs:179,185,2026`**: three `which` calls → `which::which(...)`.
- **`src-tauri/src/agents/opencode/adapter.rs:60,81`**: config root via `dirs::config_dir()` on macOS.
- **ungated `libc::*` and `std::os::unix::*` POSIX sites**: compile once `libc` is `cfg(unix)`; no per-site edit needed.
- **`.github/workflows/`**: new `cross-check` job (`cargo check --target aarch64-apple-darwin`), runner choice resolved empirically (design D5).
- **`CLAUDE.md`**: new "Cross-platform invariant" section.
- **Out of scope**: functional macOS impls of procfs scanning + the `/etc/os-release` & `/proc/sys/kernel/osrelease` runtime reads (`platform-proc`), desktop integration (`platform-desktop`), Unix-socket IPC transport (`platform-ipc`), bundling/signing (`platform-bundle-ci`).
