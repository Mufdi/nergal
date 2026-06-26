# Tasks — platform-compile

Each group is independently verifiable. Full check: `cd src-tauri && cargo clippy -- -D warnings && cargo test && cargo fmt --check && cd .. && npx tsc --noEmit`. Implementation happens in a fresh session; read design.md first.

> **IMPLEMENTED 2026-06-26.** All groups done. macOS `cargo check --target aarch64-apple-darwin` is clean; Linux full check green (clippy -D warnings, 684 tests, fmt, tsc). Empirical results recorded in design.md § "Empirical pre-flight results". The one open item is the GUI smoke (8.4) which needs a human at the running app.

## 1. Pre-flight — empirical blocker discovery (do FIRST; drives D5/D7/D8)

- [x] 1.1 `rustup target add aarch64-apple-darwin`, then `cargo check --target aarch64-apple-darwin`. **Done.** Blocker set captured (libc ×10, zbus@updater.rs:333, statvfs width@worktree_sessions.rs:606 NEW, find_in_environ dead-code). Recorded in design.md empirical section.
- [x] 1.2 **Done.** A Linux runner CANNOT cross-check — build aborts in `objc2-exception-helper` C build script (`-arch arm64` + macOS SDK, from arboard/tao/rfd). → D5 fallback **branch 2 (macos-latest)** selected. Local validation done via a throwaway `cc`-stub wrapper (objc `.o` stubbed; check doesn't link) — dev-only, not CI. `arboard`/`wezterm-term`/`termwiz` raised NO Rust errors.

## 2. Cargo.toml dependency surgery

- [x] 2.1 `libc` moved to `[target.'cfg(unix)'.dependencies]`; comment updated. `webkit2gtk` stays under linux.
- [x] 2.2 (D7) `keyring` split per target: Linux `async-secret-service`+`tokio`+`crypto-rust`; macOS `apple-native`. Confirmed `apple-native` resolves (zero keyring errors on macOS check).
- [x] 2.3 (D8) `zbus` relocated verbatim (`default-features = false, features = ["tokio"]`) under the linux target block.
- [x] 2.4 Contingency resolved: arboard/wezterm/termwiz compiled fine (no per-target gating needed); the SDK failure was runner-level, so D5 → macos-latest runner (not per-dep gating).

## 3. which-crate replacement

- [x] 3.1 `commands.rs:178`: executable lookup now `which::which(&path)` (the two former `which` shell-outs collapsed into one lookup).
- [x] 3.2 Resolved-path now from the same `which::which` result (`.to_string_lossy().into_owned()`).
- [x] 3.3 `find_available_command`: inner `Command::new("which")` → `which::which(cmd).is_ok()`, return contract preserved.
- [x] 3.4 N/A — no `use std::process::Command` import existed (all sites fully-qualified `std::process::Command`); other `Command::new` usages (gh/git/xdg-open/notify-send) remain, so nothing to remove.

## 4. zbus reveal-path gating (D8)

- [x] 4.1 `show_items_via_dbus()` gated `#[cfg(target_os = "linux")]` (doc-comment notes platform-desktop will replace it).
- [x] 4.2 `reveal_in_downloads`: Linux branch keeps dbus+xdg-open fallback; non-Linux branch is a log-only `Ok(())` no-op (does NOT fall into xdg-open). macOS reveal tracked in platform-desktop.

## 5. opencode config path resolution

- [x] 5.1 **Researched (REVERSED the assumption).** opencode resolves config/data via `xdg-basedir`, keeping the XDG layout on macOS (`~/.config/opencode`, `~/.local/share/opencode`) — NOT `~/Library/Application Support`. Sources: opencode docs + `sst/opencode#8235`. Documented as a WHY-comment in the adapter.
- [x] 5.2 **No path change needed** — the current `home.join(".config/opencode")` is already correct on macOS (and was never a compile blocker). `dirs::config_dir()` would have introduced a bug. Added a WHY-comment so it isn't "fixed" to Apple dirs later. Spec requirement corrected (task 8.5).
- [x] 5.3 Data dir `~/.local/share/opencode` also XDG on macOS — unchanged, correct.
- [x] 5.4 `with_config_root()` stays hermetic (explicit root → platform-independent); untouched.

## 6. CI cross-target gate

- [x] 6.1 New `.github/workflows/ci.yml` → `macos-cross-check` job. Trigger: `pull_request` + `push:main`, path filter `src-tauri/**` (+ the workflow file). Runner **macos-latest** (D5 resolution). Step `cargo check --target aarch64-apple-darwin`; non-zero fails the PR.
- [x] 6.2 Job name + header comment document the honest reach (catches macOS-breaking; NOT ungated-unix seams) and WHY macos-latest (objc SDK).
- [x] 6.3 Path-filter note added: Cargo.lock lives under `src-tauri/` so dep bumps are covered; comment warns a future lockfile-to-root move must extend the filter.

## 7. CLAUDE.md convention update

- [x] 7.1 "Cross-platform invariant" bullet added to Critical conventions: born-gated seams + stubs, prefer cross-platform crates, the `macos-cross-check` enforcement arm AND its reach limit (green ≠ sufficient; needs deferred Windows gate for ungated-unix).

## 8. Verification

- [x] 8.1 `cargo check --target aarch64-apple-darwin` exits 0 — verified LOCALLY via the cc-stub workaround (objc stack stubbed; our Rust type-checks clean, no warnings). The REAL macOS compile runs in CI on macos-latest. *(Caveat: local check cannot compile the objc C for real; CI is authoritative.)*
- [x] 8.2 Linux full check green: clippy `-D warnings` clean, 684 tests pass, fmt clean, `tsc --noEmit` clean.
- [x] 8.3 D7 regression guard: Linux `keyring` feature set byte-identical (async-secret-service+tokio+crypto-rust); Cargo.lock keeps `zbus 4.4.0` via secret-service on Linux; 684 tests (incl. keyring-touching) pass. macOS adds security-framework/core-foundation (Keychain) as target-only deps.
- [~] 8.4 Linux binary links cleanly (`cargo build` ok); the `which`-replaced paths + gated reveal compile unchanged. **GUI smoke (`pnpm tauri dev` launch + live which-path exercise) is the one human-gated item** — left for a desktop session.
- [x] 8.5 `specs/platform-compat/spec.md` updated: opencode requirement corrected (XDG on macOS, not `dirs::config_dir()`) + new statvfs field-width scenario added.
