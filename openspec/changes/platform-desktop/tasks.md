# Tasks — platform-desktop

Each phase is independently verifiable. Run after every phase: `cd src-tauri && cargo clippy -- -D warnings && cargo test && cargo fmt --check && cd .. && npx tsc --noEmit`.

## 1. Dependency wiring

- [ ] 1.1 Add `tauri-plugin-opener = "2"` to `[dependencies]` in `src-tauri/Cargo.toml`. Confirm `tauri-plugin-notification = "2"` is already present (it is, line 27).
- [ ] 1.2 Register `tauri_plugin_opener::init()` in `src-tauri/src/lib.rs` alongside the existing `tauri_plugin_notification::init()` call (`lib.rs:305`).
- [ ] 1.3 Add opener permissions to `src-tauri/capabilities/default.json`: `"opener:default"`, `"opener:allow-open-url"`, `"opener:allow-reveal-item-in-dir"`. Keep the existing `"notification:default"` entry.

## 2. Downloads directory — remove xdg-user-dir subprocess

- [ ] 2.1 In `src-tauri/src/updater.rs`, replace `resolve_downloads_dir` (lines ~224-236): drop the `xdg-user-dir DOWNLOAD` subprocess; use `dirs::download_dir().unwrap_or_else(|| dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp")).join("Downloads"))`. The `dirs` crate is already a dependency.
- [ ] 2.2 Remove `"xdg-user-dir"` from the `missing_binaries` array in `check_system_health` (`updater.rs:66`). Keep `"git"`.

## 3. Reveal in file manager — remove D-Bus + xdg-open

- [ ] 3.1 In `src-tauri/src/updater.rs`, delete the `show_items_via_dbus` async function (lines ~329-346) and its import of `zbus`.
- [ ] 3.2 Replace the body of `reveal_in_downloads` (`updater.rs:446-458`) with a single `tauri_plugin_opener::OpenerExt::opener(&app_handle).reveal_item_in_dir(p)` call. The command must receive an `AppHandle` parameter (add it if not already present as `_app: tauri::AppHandle`).
- [ ] 3.3 Remove `xdg-open` from the `missing_binaries` array in `check_system_health` (`updater.rs:66`) — the reveal path no longer needs it.
- [ ] 3.4 Confirm `zbus` has no remaining direct usages outside the deleted function: `grep -rn "use zbus" src-tauri/src`. If clean, remove the top-level `zbus = { ... }` line from `Cargo.toml` (lines ~36).

## 4. Open file — remove gtk-launch + xdg-open

- [ ] 4.1 In `src-tauri/src/updater.rs`, replace `open_log_file` (lines ~375-402): remove `default_app_for_mime` helper and the `gtk-launch` spawn; remove the `xdg-open <dir>` fallback. Replace with `tauri_plugin_opener::OpenerExt::opener(&app_handle).open_path(&log_path, None::<&str>)`. Add `AppHandle` parameter to the command signature.
- [ ] 4.2 Delete the `default_app_for_mime` function (`updater.rs:350-360`) — it only served `open_log_file`.

## 5. Open URI — remove xdg-open in obsidian_open_uri

- [ ] 5.1 In `src-tauri/src/commands.rs`, replace `obsidian_open_uri` (line ~3202): swap `std::process::Command::new("xdg-open")` for `tauri_plugin_opener::OpenerExt::opener(&app_handle).open_url(&uri, None::<&str>)`. Add `app_handle: tauri::AppHandle` parameter to the command. Update the `invoke_handler` registration in `lib.rs` if the signature arity changes (Tauri derives the handler from the function signature automatically — check that the frontend call site passes the right args).

## 6. Desktop notifications — replace notify-send at all three sites

- [ ] 6.1 In `src-tauri/src/commands.rs`, replace `send_notification` (lines ~576-586): use `app_handle.notification().builder().title(&title).body(&body).show().unwrap_or_else(|e| tracing::warn!("notification failed: {e}"))`. Add `app_handle: tauri::AppHandle` parameter.
- [ ] 6.2 In `src-tauri/src/linear/mod.rs`, replace `notify-send` subprocess (lines ~1499-1505): use the same `app.notification()` call pattern. `app: AppHandle` is already available in the calling context (confirm and reuse it).
- [ ] 6.3 In `src-tauri/src/clickup/poller.rs`, replace `TauriEffects::notify` implementation (lines ~926-935): swap the `notify-send` spawn for `self.app.notification().builder().title(title).body(body).show().unwrap_or_else(|e| tracing::warn!("clickup notification failed: {e}"))`.

## 7. Reveal directory — remove xdg-open in scratchpad

- [ ] 7.1 In `src-tauri/src/scratchpad/commands.rs`, replace `scratchpad_reveal_in_file_manager` (lines ~184-188): swap `std::process::Command::new("xdg-open")` for `tauri_plugin_opener::OpenerExt::opener(&app_handle).reveal_item_in_dir(&root)`. Add `app_handle: tauri::AppHandle` parameter.

## 8. Verification

- [ ] 8.1 Full check green: `cd src-tauri && cargo clippy -- -D warnings && cargo test && cargo fmt --check && cd .. && npx tsc --noEmit`.
- [ ] 8.2 Confirm no remaining `xdg-open`, `xdg-user-dir`, `gtk-launch`, `notify-send` subprocess invocations in non-test Rust source: `grep -rn "Command::new(\"xdg\|Command::new(\"gtk-launch\|Command::new(\"notify-send" src-tauri/src` returns zero results (excluding `migrate_legacy.rs`).
- [ ] 8.3 Linux acceptance walk: (a) trigger "Reveal in Downloads" after an update download — file manager opens with file highlighted; (b) trigger "Open log file" from About — text editor opens the log; (c) send a test notification via dev console or ClickUp/Linear poller mock — notification appears in the OS notification center; (d) open an `obsidian://` URI — Obsidian launches; (e) open scratchpad settings "Reveal in file manager" — file manager opens the scratchpad directory.
- [ ] 8.4 macOS acceptance walk: same five checks as 8.3 — all five pass without error and without any `xdg-*`/`gtk-launch`/`notify-send` binary.
- [ ] 8.5 Health check: `check_system_health` on a machine without `xdg-open` installed reports no missing binaries (only `git` is checked post-change).
