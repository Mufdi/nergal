# Tasks — platform-desktop

Each phase is independently verifiable. Run after every phase: `cd src-tauri && cargo clippy -- -D warnings && cargo test && cargo fmt --check && cd .. && npx tsc --noEmit`.

## 1. Dependency wiring

- [x] 1.1 Added `tauri-plugin-opener = "2"` to `[dependencies]` in `src-tauri/Cargo.toml`. Confirmed `tauri-plugin-notification = "2"` already present.
- [x] 1.2 Registered `tauri_plugin_opener::init()` in `src-tauri/src/lib.rs` after `tauri_plugin_notification::init()`.
- [x] 1.3 Added `"opener:default"`, `"opener:allow-open-url"`, `"opener:allow-reveal-item-in-dir"` to `src-tauri/capabilities/default.json`.
- [x] 1.4 Trait imports added: `OpenerExt` in `updater.rs`, `commands.rs`, `scratchpad/commands.rs`; `NotificationExt` in `notify.rs` (macOS cfg block) and `lib.rs` (macOS cfg block). `linear/mod.rs` and `clickup/poller.rs` route through `crate::notify::send` — no direct `NotificationExt` import needed there.

## 2. Downloads directory — remove xdg-user-dir subprocess

- [x] 2.1 Replaced `resolve_downloads_dir` in `updater.rs`: extracted `downloads_from(dl, home) -> PathBuf` pure helper + calls it with real `dirs` values. No `xdg-user-dir` subprocess.
- [x] 2.2 Removed `"xdg-user-dir"` from `check_system_health` `missing_binaries`. Only `"git"` remains.

## 3. Reveal in file manager — remove D-Bus + xdg-open

- [x] 3.1 Deleted `show_items_via_dbus` function (entire `#[cfg(target_os = "linux")]` block + doc comment) from `updater.rs`.
- [x] 3.2 Rewrote `reveal_in_downloads`: single unconditional `app.opener().reveal_item_in_dir(&p)` call. Added `app: tauri::AppHandle` param. Existence guard retained.
- [x] 3.3 Removed `"xdg-open"` from `check_system_health` `missing_binaries`.
- [x] 3.4 Confirmed `grep -rn "use zbus\|zbus::" src-tauri/src` = 0 results. Removed `zbus = { version = "5", ... }` line and its comment block from `Cargo.toml` under `[target.'cfg(target_os = "linux")'.dependencies]`.

## 4. Open file — remove gtk-launch + xdg-open

- [x] 4.1 Replaced `open_log_file` in `updater.rs`: added `app: tauri::AppHandle` param; uses `app.opener().open_path(log_path.to_string_lossy().as_ref(), None::<&str>)`. Existence guard retained.
- [x] 4.2 Deleted `default_app_for_mime` (confirmed 0 callers remaining after removing `open_log_file` body). Last `xdg-mime` subprocess gone.

## 5. Open URI — remove xdg-open in obsidian_open_uri

- [x] 5.1 Extracted `fn is_allowed_scheme(uri: &str) -> bool` pure helper. `obsidian_open_uri` gains `app: tauri::AppHandle`, calls `is_allowed_scheme` first (security boundary preserved per Decision 7), then `app.opener().open_url(&uri, None::<&str>)`. No `invoke_handler` edit, no frontend change.

## 6. Desktop notifications — replace notify-send at all three sites

- [~] 6.0 **DEFERRED to human GUI session** — could not run `pnpm tauri dev` (no display in builder). **Conservative/non-regressive branch taken**: Linux = `notify-send` primary (unchanged behavior), macOS = notification plugin. Branch recorded in `src/notify.rs` module doc.
- [x] 6.1 Created `src-tauri/src/notify.rs` + registered `mod notify` in `lib.rs`. Helper `crate::notify::send(app, title, body)`. Linux: `notify-send` spawn (conservative branch per 6.0 deferral). macOS: `app.notification().builder().title(title).body(body).show()` + `tracing::warn!` on Err. Other: `tracing::warn!` no-op stub.
- [x] 6.2 Replaced `send_notification` in `commands.rs`: `crate::notify::send(&app, &title, &body)`. Added `app: tauri::AppHandle` param. No frontend change.
- [x] 6.3 Replaced `notify-send` spawn in `linear/mod.rs` `notify_assignments`: `crate::notify::send(app, "Linear", &body)`.
- [x] 6.3b Replaced `TauriEffects::notify` in `clickup/poller.rs`: `crate::notify::send(&self.app, title, body)`. Updated stale comment to reference Decision 3a.
- [x] 6.4 Added macOS `#[cfg(target_os = "macos")]` block in `lib.rs` `setup()` calling `app_handle.notification().request_permission()` BEFORE `clickup::poller::restart` and `linear::restart`. Denied result non-fatal (tracing::warn). Tradeoff documented: pollers delayed until user answers the prompt (acceptable per Decision note).

## 7. Reveal directory — remove xdg-open in scratchpad

- [x] 7.1 Replaced `scratchpad_reveal_in_file_manager` in `scratchpad/commands.rs`: added `app: tauri::AppHandle` param; uses `app.opener().open_path(root.to_string_lossy().as_ref(), None::<&str>)` — NOT `reveal_item_in_dir` (preserves "open folder contents" UX per Decision 1 mapping).

## 8. Verification

- [x] 8.0 Unit tests added:
  - (a) `is_allowed_scheme`: 2 tests in `commands.rs::config_merge_tests` — permits `obsidian://` + `nergal://`, rejects `https://evil`, `file:///etc/passwd`, `javascript:`, empty.
  - (b) `downloads_from`: 3 tests in `updater.rs::tests` — `Some("/d")` wins, `(None, Some("/h"))` → `/h/Downloads`, `(None, None)` → `/tmp/Downloads`.
- [x] 8.1 Full check green: `cargo clippy -- -D warnings` (0 errors/warnings) + `cargo test` (691 passed) + `cargo fmt --check` (clean) + `npx tsc --noEmit` (0 errors). macOS cross-check: `cargo check --target aarch64-apple-darwin` → `Finished`.
- [x] 8.2 Grep gate: `grep -rn 'Command::new("xdg\|Command::new("gtk-launch\|Command::new("notify-send' src-tauri/src` (excluding migrate_legacy.rs) → exactly 1 hit: `src/notify.rs:26` (`notify-send` in the `#[cfg(target_os = "linux")]` block of `crate::notify::send`). No hits in `commands.rs`, `linear/mod.rs`, `clickup/poller.rs`.
- [~] 8.3 Linux acceptance walk: DEFERRED to human session (requires running `pnpm tauri dev`).
- [~] 8.4 macOS acceptance walk: DEFERRED to human session (requires macOS build).
- [x] 8.5 `check_system_health` `missing_binaries` now checks only `["git"]` — no `xdg-open`, no `xdg-user-dir`.
