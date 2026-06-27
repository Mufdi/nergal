# Design — platform-desktop

Design session 2026-06-26. All decisions are final for the macOS-first port pass.

## Context

Nergal reaches out to the desktop (open files/URLs, reveal in file manager, fire notifications) through six call sites that hardcode Linux userland binaries (`xdg-open`, `xdg-user-dir`, `gtk-launch`, `notify-send`) and one D-Bus `org.freedesktop.FileManager1` call via `zbus`. None of these exist on macOS. Tauri already has two first-party plugins — `opener` and `notification` — that wrap the same operations for Linux, macOS, and Windows through a single Rust API. The mismatch is therefore a substitution problem, not an architectural one: swap the subprocess calls for plugin calls at the six sites, adjust `Cargo.toml` and `capabilities/default.json`, and the portability gap closes.

**Current-source reconciliation (post-`platform-compile`)**: this design was first drafted against the pre-`platform-compile` tree; it has been reconciled against the shipped state. Concretely: (a) `zbus` is no longer a top-level `[dependencies]` entry — `platform-compile` moved it to `[target.'cfg(target_os = "linux")'.dependencies]` at `Cargo.toml:116`; (b) `show_items_via_dbus` is now `#[cfg(target_os = "linux")]`; (c) `reveal_in_downloads` now has two `#[cfg]` branches — a Linux D-Bus+`xdg-open` path and a non-Linux log-only `Ok(())` no-op that `platform-compile` left for this change to replace; (d) `open_log_file`/`default_app_for_mime`/`resolve_downloads_dir`/`check_system_health` are unchanged by `platform-compile` and still Linux-shaped. All task line references below reflect the current tree.

The `tauri-plugin-notification` crate is already listed in `Cargo.toml` and already initialized in `lib.rs` (`.plugin(tauri_plugin_notification::init())`). A comment in `clickup/poller.rs` notes that `.show()` was "unreliable under WebKitGTK and was failing silently" — that bug drove the regression to `notify-send`. This design resolves the regression with a targeted fix rather than a bypass: see Decision 3.

## Goals / Non-Goals

**Goals:**
- Replace all six Linux-only desktop integration call sites with `opener` + `notification` plugin calls that compile and work on macOS and Linux.
- Remove the Linux-gated `zbus` `Cargo.toml` entry under `[target.'cfg(target_os = "linux")'.dependencies]` (line 116, owned by `show_items_via_dbus`; `keyring`'s own transitive D-Bus stack is untouched).
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

**Mapping** (all via the `OpenerExt` trait: `app.opener().<method>(...)` — see Decision 6):
| Old call | New call | Note |
|---|---|---|
| `xdg-open <file>` / `gtk-launch <app> <file>` (open log) | `app.opener().open_path(path, None::<&str>)` | opens a **file** |
| `xdg-open <url_or_uri>` (obsidian) | `app.opener().open_url(url, None::<&str>)` | URL/custom-scheme |
| D-Bus `ShowItems` + `xdg-open <dir>` fallback (downloads) | `app.opener().reveal_item_in_dir(path)` | targets a **file** → highlight it in its folder. Correct use of reveal. |
| `xdg-open <dir>` (scratchpad) | `app.opener().open_path(dir, None::<&str>)` | targets a **directory**. `open_path` preserves the current "open the folder's contents" behavior; `reveal_item_in_dir(dir)` would instead select the folder icon one level up — an observable UX change we deliberately avoid. |

**File-vs-directory distinction (was a latent bug in the first draft)**: `reveal_item_in_dir` highlights its argument *inside the parent directory*. That is the right semantics for `reveal_in_downloads` (argument is a downloaded file) but the WRONG semantics for the scratchpad reveal (argument is the scratchpad directory itself — the user wants the folder opened, which is what `xdg-open <dir>` did). The two sites therefore map to different opener methods.

The `open_log_file` function in `updater.rs` used `xdg-mime` + `gtk-launch` to bypass `.log` extension mapping (a v0.3.0 bug where `xdg-open nergal.log` exited 0 without opening anything). `opener::open_path` delegates to the OS default for the file's MIME type directly, so the MIME probe and `gtk-launch` workaround are no longer needed — opener resolves the handler correctly on all platforms.

### Decision 2: Use `dirs::download_dir()` instead of `xdg-user-dir DOWNLOAD`

**Chosen**: `dirs::download_dir()` with `$HOME/Downloads` fallback.

`dirs` is already a direct dependency (visible in `Cargo.toml`). The existing code already falls through to `dirs::home_dir().join("Downloads")` when `xdg-user-dir` is unavailable. The new implementation simply calls `dirs::download_dir()` first, which returns the OS-correct path on macOS (`~/Downloads`) and Linux (respects `xdg-user-dirs` via the same lookup `xdg-user-dir` performs). The subprocess is eliminated without any behavior regression.

**Alternative considered**: Keep `xdg-user-dir` with `#[cfg(unix)]` guard. Rejected because `dirs::download_dir()` already covers both Linux and macOS correctly; the guard would be dead code on macOS anyway.

### Decision 3: Fix `tauri-plugin-notification` rather than keep `notify-send`

**Context**: `clickup/poller.rs:923-925` documents that `tauri_plugin_notification`'s `.show()` was "unreliable under WebKitGTK and failing silently," which drove the `notify-send` regression across all three notification sites. The comment was written against an older version of the plugin.

**Chosen**: Use `tauri_plugin_notification::NotificationExt` on **macOS** unconditionally. On **Linux**, choose ONE implementation — plugin-only *or* `notify-send`-primary — at build time based on the task 6.0 empirical observation (Decision 3a); this is an either/or branch, NOT a runtime "plugin with a fallback." `"notification:default"` is already present in `capabilities/default.json`. On a *loud* failure (an actual `Err`), log at `tracing::warn!` and continue — matching the existing non-fatal error-handling pattern.

**Why a contingency is required, not optional**: the `notify-send` regression was a *deliberate root-cause workaround* for the plugin "failing silently" under WebKitGTK. Reverting it on the unverified claim that the regression "was version-specific" / "works on modern builds" is exactly the patch-over-root-cause inversion the project forbids. Critically, **the failure is SILENT — `.show()` returned `Ok(())` while displaying nothing.** That has a hard consequence for the design: *silent failure cannot be detected at runtime*, so a runtime `Err`-triggered fallback would never fire in the exact scenario that motivates it. The Linux strategy must therefore be decided **at build/implementation time from an empirical verification (task 6.0)**, not papered over with a runtime check that protects nothing.

**Decision 3a — Linux notification strategy is gated on the 6.0 empirical result (no runtime silent-failure detection)**:
- **macOS**: always the plugin alone (no `notify-send` exists).
- **Linux**: task 6.0 wires one site and visually confirms whether `.show()` actually displays a notification on the project's current WebKitGTK build.
  - **If 6.0 confirms the plugin displays reliably** → use the plugin as the sole Linux path; remove all `notify-send`. Clean cross-platform result.
  - **If 6.0 shows the plugin still fails silently** (`Ok` but nothing shown) → keep `notify-send` as the **PRIMARY** Linux path behind `#[cfg(target_os = "linux")]` (the plugin is NOT used on Linux because its success is indistinguishable from silent failure), and use the plugin on macOS. This is the documented decision branch the implementer commits to based on observed behavior — NOT a runtime `Ok`/`Err` toggle.
- An `Err`-triggered fallback is explicitly REJECTED as the silent-failure safety net: it cannot catch an `Ok`-returning no-display. (A best-effort `Err`→`notify-send` retry for *loud* failures may be added on Linux as a minor extra, but it is NOT claimed as protection against the silent regression and the Rollback section must not rely on it.)

This keeps the macOS path plugin-native and the Linux path empirically grounded, consistent with the CLAUDE.md cross-platform invariant (Linux-specific path behind a `#[cfg]` gate, chosen by evidence rather than assumption).

**Alternative considered**: `#[cfg(target_os = "linux")] notify-send` + `#[cfg(target_os = "macos")] osascript`. Rejected — `osascript` is a new subprocess dep on macOS and the plugin already covers macOS natively. (Note the rejected "plugin-only, no fallback" stance from the first draft is itself rejected by Decision 3a: plugin-only is fine on macOS but premature on Linux until verified.)

### Decision 4: Remove the Linux-gated direct `zbus` dependency

**Verified current location**: `zbus` is NOT a top-level `[dependencies]` entry. `platform-compile` already moved it to `[target.'cfg(target_os = "linux")'.dependencies]` at `Cargo.toml:116` (`zbus = { version = "5", default-features = false, features = ["tokio"] }`), with a comment block (lines 111-115) that explicitly states "platform-desktop later replaces this reveal path with the cross-platform opener plugin." It exists solely to support `show_items_via_dbus` (itself now `#[cfg(target_os = "linux")]`) in `updater.rs`. Once that function is removed, the Linux-gated `zbus` entry **and its comment block** are deleted. macOS was never affected (no direct `zbus` there).

**Caveat**: `keyring`'s `async-secret-service` feature pulls its own `zbus` transitively on Linux. Removing the direct entry does not remove `zbus` from the Linux build; it only removes the explicit version pin. This is intentional — the transitive pull is the keyring's concern. Guard the removal with `grep -rn "use zbus\|zbus::" src-tauri/src` (task 3.4) to confirm no other direct usage before deleting.

### Decision 5: No changes to the frontend-observable command surface

Several commands gain an injected `app_handle: tauri::AppHandle` parameter (needed for `app.opener()` / `app.notification()`). **This is not a frontend-observable signature change**: Tauri auto-injects `AppHandle`, `State<'_, T>`, `Window`, etc. from the runtime — they are NOT part of the JS argument surface and are never passed from `invoke(...)`. The `invoke_handler!` registration is derived from the function and needs no manual edit for an added managed/injected param. Therefore the frontend `invoke('send_notification', { title, body })` (and the other four call sites) is byte-for-byte unchanged; no JS/TS edits, no arity check, no frontend call-site audit required. The commands' *user-supplied* parameters and their return/error-string shapes are unchanged.

### Decision 6: Rust-side `OpenerExt` calls bypass the capability ACL; capabilities are defensive only

The opener plugin's permission/scope ACL (`opener:default`, `opener:allow-open-url`, `opener:allow-reveal-item-in-dir`, plus path scope) gates the **JS/IPC command layer** — i.e. the auto-generated `open_path`/`open_url`/`reveal_item_in_dir` Tauri commands that the `@tauri-apps/plugin-opener` JS API invokes. The opener command implementations run a scope check (`is_path_allowed`) and *then* call the same `app.opener().<method>()` trait method this change uses directly. **Calling `app.opener().<method>()` from Rust skips the command wrapper entirely, so no capability entry and no path scope is consulted.** All six call sites here are Rust-side; the frontend never touches the opener JS API. Consequence: the `opener:*` capability entries are **not required** for these calls to function — the earlier "forgetting the capabilities entry causes runtime errors" claim is incorrect for the Rust path. They are added **defensively** (zero cost; ready if a future feature calls opener from JS) and to keep the capability file an honest record of which plugins are wired. What IS mandatory and load-bearing is the plugin registration `tauri_plugin_opener::init()` in `lib.rs` (without it, `app.opener()` panics/errs at runtime). See also Decision 7 — because the ACL is bypassed, the only guard on `obsidian_open_uri` is the in-code scheme allowlist, which MUST be preserved.

### Decision 7: Preserve the `obsidian_open_uri` scheme allowlist as the sole security boundary

`obsidian_open_uri` currently rejects any URI not prefixed `obsidian://` or `nergal://` *before* spawning `xdg-open`. Because the Rust `app.opener().open_url()` call bypasses the opener path/URL scope ACL (Decision 6), this in-code `starts_with` allowlist becomes the **only** thing preventing an arbitrary-URI open. The swap to `open_url` MUST keep the allowlist check ahead of the open call (task 5.1), and a unit test asserts the rejection path (task 8.x / spec scenario "URIs not matching the allowlist SHALL be rejected ... before any open call").

## Risks / Trade-offs

- **`tauri-plugin-opener` not yet in `Cargo.toml`** → add `tauri-plugin-opener = "2"` to `[dependencies]` and register `.plugin(tauri_plugin_opener::init())` in `lib.rs` (mandatory — see Decision 6). The `opener:*` `capabilities/default.json` entries are defensive, not load-bearing for the Rust call path (Decision 6); their absence does NOT cause runtime errors for `app.opener().*` calls.

- **`NotificationExt`/`OpenerExt` traits must be in scope** → `app.opener()` and `app.notification()` are trait methods; each touched module (`updater.rs`, `commands.rs`, `linear/mod.rs`, `clickup/poller.rs`, `scratchpad/commands.rs`) needs `use tauri_plugin_opener::OpenerExt;` and/or `use tauri_plugin_notification::NotificationExt;`. Omission is a compile error, caught by the full check.

- **Background-thread notification calls** → the ClickUp/Linear poller notification sites fire from background tasks, not the main thread. `app.notification().builder().show()` is not a window operation and is expected to be thread-agnostic (it dispatches to the OS notification service). The acceptance walk (task 8.3c) exercises the poller path specifically; if `.show()` ever requires the main thread on a target, wrap it in `app.run_on_main_thread(...)`.

- **Notification plugin reliability on Linux** → A *loud* failure (`.show()` returns `Err`) is surfaced by `tracing::warn!` and is non-fatal (same as the current `notify-send` spawn failure path). A **silent** no-display (`.show()` returns `Ok(())` while showing nothing — the actual historical WebKitGTK regression) is NOT runtime-detectable and a warn log will never fire for it; log monitoring is therefore explicitly NOT the mitigation. That risk is addressed at build time by the task 6.0 empirical gate, which picks the Linux mechanism by human observation (Decision 3/3a) and is reaffirmed in the Rollback section.

- **`open_log_file` regression risk** → The v0.3.0 bug (xdg-open exiting 0 without opening `.log` files) was caused by the extension-to-MIME mapping, not by `xdg-open` itself. `opener::open_path` uses the OS handler lookup; on macOS this is the LaunchServices resolver; on Linux it delegates to `gio open` / `xdg-open` under the hood but with a working MIME query. Manual acceptance test required: open the log file on both Linux and macOS after the change.

- **`dirs::download_dir()` returns `None` on edge-case Linux installs** → The `$HOME/Downloads` fallback is identical to the existing behavior when `xdg-user-dir` was unavailable. No regression.

## Migration Plan

1. Add `tauri-plugin-opener = "2"` to `[dependencies]` in `src-tauri/Cargo.toml`.
2. Register opener plugin in `src-tauri/src/lib.rs`.
3. Add opener permissions to `src-tauri/capabilities/default.json`.
4. Replace `resolve_downloads_dir` in `updater.rs` (remove `xdg-user-dir` subprocess).
5. Replace `show_items_via_dbus` + `reveal_in_downloads` in `updater.rs` (remove D-Bus call).
6. Replace `open_log_file` in `updater.rs` (remove `gtk-launch` + `xdg-open` fallback).
7. **Empirical gate** (Decision 3/3a / task 6.0): observe whether `tauri-plugin-notification` `.show()` actually displays on the project's WebKitGTK. This decides the Linux helper body (plugin if it displays, `notify-send` if it fails silently). macOS is always plugin. Then add the shared `crate::notify::send(&AppHandle, &str, &str)` helper accordingly.
8. Replace `send_notification` in `commands.rs` (via the helper; preserve user-supplied title/body).
9. Replace `notify-send` in `linear/mod.rs` (via the helper; `app: &AppHandle` already in scope).
10. Replace `notify-send` in `clickup/poller.rs` (via the helper; `self.app: AppHandle`).
11. Wire macOS notification permission in `setup()` BEFORE pollers spawn (`permission_state`/`request_permission`, task 6.4).
12. Replace `xdg-open` in `commands.rs` (`obsidian_open_uri`) — **keep the scheme allowlist check ahead of the open call** (Decision 7).
13. Replace `xdg-open <dir>` in `scratchpad/commands.rs` with `open_path(dir)` (not `reveal_item_in_dir` — Decision 1 mapping).
14. Remove `check_system_health` entries for `xdg-open` and `xdg-user-dir`.
15. Remove the Linux-gated `zbus` entry at `Cargo.toml:116` + its comment (after `grep -rn "use zbus\|zbus::" src-tauri/src` is clean).
16. Run full check: `cd src-tauri && cargo clippy -- -D warnings && cargo test && cargo fmt --check && cd .. && npx tsc --noEmit`.
17. Manual acceptance walk: open file, reveal in downloads, desktop notification — on Linux. Repeat on macOS.

No database migrations. **Rollback**: all changes are local to the Rust backend; reverting is a plain `git revert`. The riskiest single piece is the `notify-send`→plugin notification change; it is de-risked NOT by a runtime fallback (which cannot detect silent failure — Decision 3a) but by the task 6.0 empirical gate: the Linux path only switches to the plugin if 6.0 observes it actually displaying, otherwise `notify-send` remains the primary Linux path. So the change never ships an unverified silent-notification regression in the first place.

**Honest residual risk** (no false safety claim): the gate moves the silent-failure assumption from runtime to one developer's build-time WebKitGTK observation — it does not eliminate it for users on a *different* WebKitGTK. In the `6.0 = displays` branch `notify-send` is removed entirely, so there is NO in-tree fallback there; if a later WebKitGTK reintroduces silent failure, the mitigation is a **code change** (re-add the `notify-send` Linux path), not a runtime degrade. In the `6.0 = silent` branch the Linux path is already `notify-send`, so it is unaffected. Reverting the macOS-only plugin wiring is independent of the Linux path either way.

## Open Questions

- **Notification permission on macOS** (now an explicit, sequenced task — not a deferral): macOS gates notifications behind a per-app permission. For a notification-heavy app (two background pollers + the general command), a denied/un-granted first prompt means every later notification fails. **Ordering matters**: the permission must be resolved during `setup()` BEFORE the ClickUp/Linear pollers are spawned, so a poller's first send never races the "not determined" window from a background thread. Task 6.4: in `setup()` (main thread), if `permission_state()` is `NotDetermined`, call `request_permission()` (on the main thread — macOS may require it; use `run_on_main_thread` if the call originates off-main); spawn the pollers only after this resolves; treat a denied result as the non-fatal warn path. This must not block the macOS build but must be wired before the macOS acceptance walk so 8.4(c) is meaningful.
- **`opener::open_path` on macOS for log files that are in `~/Library/Caches/`**: Confirm Finder / default text editor opens the file (LaunchServices should handle it). Covered by the acceptance test in tasks.md.
