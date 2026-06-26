# Design — platform-desktop

Design session 2026-06-26. All decisions are final for the macOS-first port pass.

## Context

Nergal reaches out to the desktop (open files/URLs, reveal in file manager, fire notifications) through six call sites that hardcode Linux userland binaries (`xdg-open`, `xdg-user-dir`, `gtk-launch`, `notify-send`) and one D-Bus `org.freedesktop.FileManager1` call via `zbus`. None of these exist on macOS. Tauri already has two first-party plugins — `opener` and `notification` — that wrap the same operations for Linux, macOS, and Windows through a single Rust API. The mismatch is therefore a substitution problem, not an architectural one: swap the subprocess calls for plugin calls at the six sites, adjust `Cargo.toml` and `capabilities/default.json`, and the portability gap closes.

The `tauri-plugin-notification` crate is already listed in `Cargo.toml` and already initialized in `lib.rs` (`.plugin(tauri_plugin_notification::init())`). A comment in `clickup/poller.rs` notes that `.show()` was "unreliable under WebKitGTK and was failing silently" — that bug drove the regression to `notify-send`. This design resolves the regression with a targeted fix rather than a bypass: see Decision 3.

## Goals / Non-Goals

**Goals:**
- Replace all six Linux-only desktop integration call sites with `opener` + `notification` plugin calls that compile and work on macOS and Linux.
- Remove the top-level `zbus` `Cargo.toml` entry (owned by `show_items_via_dbus`; `keyring`'s own transitive D-Bus stack is untouched).
- Remove `xdg-open` and `xdg-user-dir` from `check_system_health`'s required-binary list.
- Keep the frontend API surface and all Tauri command signatures unchanged (zero JS/TS changes required).

**Non-Goals:**
- Windows support (this pass targets macOS + Linux only; Windows is a later change).
- Porting other Linux-only seams (`/proc`, Unix sockets, FIFOs) — those belong to `platform-compile`.
- Deep-link registration or the `nergal://` scheme handler — already handled by `tauri-plugin-deep-link`, unchanged.
- Changing notification copy, timing, or UX behavior.

## Decisions

### Decision 1: Use `tauri-plugin-opener` instead of per-OS `Command` shells

**Chosen**: `tauri-plugin-opener` (`opener::open_path`, `opener::open_url`, `opener::reveal_item_in_dir`).

**Alternatives considered:**
- **`#[cfg(target_os)]` + per-OS `Command`**: Would replace Linux subprocess calls with macOS `open`/`open -R` equivalents. Achieves the same result but introduces OS-specific branches in business logic — the exact anti-pattern the plugin was designed to eliminate. Every future OS target would need another branch.
- **`open` crate**: A thin cross-platform wrapper around `xdg-open`/`open`/`start`. Simpler than the Tauri plugin but unaware of Tauri's sandbox and capability model; also adds a second dep solving the same problem as `opener`.

`tauri-plugin-opener` is the endorsed Tauri v2 path, is already used by other Tauri apps in the ecosystem, and capability-gates the calls through `capabilities/default.json` consistently with every other privileged operation in the app.

**Mapping**:
| Old call | New call |
|---|---|
| `xdg-open <file>` / `gtk-launch <app> <file>` | `opener::open_path(path, None)` |
| `xdg-open <url_or_uri>` | `opener::open_url(url, None)` |
| D-Bus `ShowItems` + `xdg-open <dir>` fallback | `opener::reveal_item_in_dir(path)` |
| `xdg-open <dir>` (scratchpad) | `opener::reveal_item_in_dir(path)` |

The `open_log_file` function in `updater.rs` used `xdg-mime` + `gtk-launch` to bypass `.log` extension mapping (a v0.3.0 bug where `xdg-open nergal.log` exited 0 without opening anything). `opener::open_path` delegates to the OS default for the file's MIME type directly, so the MIME probe and `gtk-launch` workaround are no longer needed — opener resolves the handler correctly on all platforms.

### Decision 2: Use `dirs::download_dir()` instead of `xdg-user-dir DOWNLOAD`

**Chosen**: `dirs::download_dir()` with `$HOME/Downloads` fallback.

`dirs` is already a direct dependency (visible in `Cargo.toml`). The existing code already falls through to `dirs::home_dir().join("Downloads")` when `xdg-user-dir` is unavailable. The new implementation simply calls `dirs::download_dir()` first, which returns the OS-correct path on macOS (`~/Downloads`) and Linux (respects `xdg-user-dirs` via the same lookup `xdg-user-dir` performs). The subprocess is eliminated without any behavior regression.

**Alternative considered**: Keep `xdg-user-dir` with `#[cfg(unix)]` guard. Rejected because `dirs::download_dir()` already covers both Linux and macOS correctly; the guard would be dead code on macOS anyway.

### Decision 3: Fix `tauri-plugin-notification` rather than keep `notify-send`

**Context**: `clickup/poller.rs:923-925` documents that `tauri_plugin_notification`'s `.show()` was "unreliable under WebKitGTK and failing silently," which drove the `notify-send` regression across all three notification sites. The comment was written against an older version of the plugin.

**Chosen**: Use `tauri_plugin_notification::NotificationExt` at all three sites. Add `"notification:default"` permission (already present in `capabilities/default.json`) and call `.show()` via the `AppHandle`. If the call fails, log at `tracing::warn!` and continue — matching the existing `notify-send` error-handling pattern (spawn errors were already non-fatal).

**Why this is the right fix**: `notify-send` does not exist on macOS, so keeping it would require a `#[cfg(target_os = "linux")]` guard that skips notifications entirely on macOS — worse than the original problem. The plugin works correctly on modern WebKitGTK builds (the regression was version-specific). If silent failure recurs on a specific desktop, the `tracing::warn!` log surfaces it without crashing the app.

**Alternative considered**: `#[cfg(target_os = "linux")] notify-send` + `#[cfg(target_os = "macos")] osascript`. Rejected — more fragile than the plugin, adds a new subprocess dependency on macOS, and is exactly what the plugin exists to avoid.

### Decision 4: Remove the top-level `zbus` dependency

`zbus` appears in `Cargo.toml` at line 36 solely to support `show_items_via_dbus` in `updater.rs`. Once that function is replaced by `opener::reveal_item_in_dir`, the explicit `zbus` dependency is unused. It can be removed.

**Caveat**: `keyring = { version = "3", features = ["async-secret-service", ...] }` pulls its own `zbus` transitively on Linux (via `secret-service`). Removing the top-level entry does not remove `zbus` from the build on Linux; it only removes the explicit version pin. This is intentional — the remaining transitive pull is the keyring's concern, not ours. The `Cargo.toml` comment block (lines 104-109) already documents this separation.

### Decision 5: No changes to Tauri command signatures or frontend code

All six call sites are in Rust; none return new data or change their error-string format in a way observable to the frontend. The commands (`send_notification`, `obsidian_open_uri`, `reveal_in_downloads`, `open_log_file`, `scratchpad_reveal_in_file_manager`, `check_system_health`) retain their exact signatures. The frontend invokes them identically before and after this change.

## Risks / Trade-offs

- **`tauri-plugin-opener` not yet in `Cargo.toml`** → add `tauri-plugin-opener = "2"` to `[dependencies]` and register `.plugin(tauri_plugin_opener::init())` in `lib.rs`. Also add `opener:default`, `opener:allow-open-url`, `opener:allow-reveal-item-in-dir` to `capabilities/default.json`. Forgetting the capabilities entry causes runtime errors, not compile errors.

- **Notification plugin reliability on Linux** → If `.show()` silently fails on a particular GTK/WebKit version, the `tracing::warn!` log will surface it. The user-visible regression is the same as the current `notify-send` spawn failure path (already non-fatal). Monitoring the warn logs during the Linux acceptance walk is sufficient mitigation.

- **`open_log_file` regression risk** → The v0.3.0 bug (xdg-open exiting 0 without opening `.log` files) was caused by the extension-to-MIME mapping, not by `xdg-open` itself. `opener::open_path` uses the OS handler lookup; on macOS this is the LaunchServices resolver; on Linux it delegates to `gio open` / `xdg-open` under the hood but with a working MIME query. Manual acceptance test required: open the log file on both Linux and macOS after the change.

- **`dirs::download_dir()` returns `None` on edge-case Linux installs** → The `$HOME/Downloads` fallback is identical to the existing behavior when `xdg-user-dir` was unavailable. No regression.

## Migration Plan

1. Add `tauri-plugin-opener = "2"` to `[dependencies]` in `src-tauri/Cargo.toml`.
2. Register opener plugin in `src-tauri/src/lib.rs`.
3. Add opener permissions to `src-tauri/capabilities/default.json`.
4. Replace `resolve_downloads_dir` in `updater.rs` (remove `xdg-user-dir` subprocess).
5. Replace `show_items_via_dbus` + `reveal_in_downloads` in `updater.rs` (remove D-Bus call).
6. Replace `open_log_file` in `updater.rs` (remove `gtk-launch` + `xdg-open` fallback).
7. Replace `send_notification` in `commands.rs`.
8. Replace `notify-send` in `linear/mod.rs`.
9. Replace `notify-send` in `clickup/poller.rs`.
10. Replace `xdg-open` in `commands.rs` (`obsidian_open_uri`).
11. Replace `xdg-open` in `scratchpad/commands.rs` (`scratchpad_reveal_in_file_manager`).
12. Remove `check_system_health` entries for `xdg-open` and `xdg-user-dir`.
13. Remove top-level `zbus` from `Cargo.toml` (confirm no remaining direct usages with `grep -rn "use zbus" src-tauri/src`).
14. Run full check: `cd src-tauri && cargo clippy -- -D warnings && cargo test && cargo fmt --check && cd .. && npx tsc --noEmit`.
15. Manual acceptance walk: open file, reveal in downloads, desktop notification — on Linux. Repeat on macOS.

No database migrations. No rollback strategy required (all changes are local to Rust backend; reverting is a plain git revert).

## Open Questions

- **Notification permission on macOS**: macOS requires explicit user permission for notifications. `tauri-plugin-notification` handles the permission request automatically on first `.show()` call — confirm this is the right UX vs an explicit prompt at app launch. Defer to acceptance testing; not a blocker for the macOS build.
- **`opener::open_path` on macOS for log files that are in `~/Library/Caches/`**: Confirm Finder / default text editor opens the file (LaunchServices should handle it). Covered by the acceptance test in tasks.md.
