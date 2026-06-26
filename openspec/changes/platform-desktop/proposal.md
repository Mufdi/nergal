## Why

Nergal's open-file, reveal-in-file-manager, and desktop-notification paths are implemented with Linux-only shell commands (`xdg-open`, `xdg-user-dir`, `gtk-launch`, `notify-send`) and a D-Bus `org.freedesktop.FileManager1` call. These paths fail silently or produce compile errors on macOS and Windows. Tauri ships first-party plugins (`opener`, `notification`) that provide the same three operations cross-platform; replacing the ad-hoc shell invocations unblocks macOS support without forking any logic per OS.

## What Changes

- Replace `xdg-open`/`gtk-launch` in `updater.rs` (open log file, open directory as fallback) with `tauri-plugin-opener`'s `open_path` / `open_url`.
- Replace `xdg-user-dir DOWNLOAD` in `updater.rs` with `dirs::download_dir()` (already a direct dep) — no subprocess required.
- Remove `xdg-open`/`xdg-mime`/`gtk-launch` probe from `check_system_health` in `updater.rs`; neither is required when the opener plugin handles the calls.
- Replace `zbus` `org.freedesktop.FileManager1` `ShowItems` call in `updater.rs` (`show_items_via_dbus` + `reveal_in_downloads`) with `opener::reveal_item_in_dir`.
- Replace `notify-send` subprocess in `commands.rs` (`send_notification`), `linear/mod.rs`, and `clickup/poller.rs` with `tauri_plugin_notification::NotificationExt::notification().show()`.
- Replace `xdg-open <uri>` in `commands.rs` (`obsidian_open_uri`) with `opener::open_url`.
- Replace `xdg-open <dir>` in `scratchpad/commands.rs` (`scratchpad_reveal_in_file_manager`) with `opener::reveal_item_in_dir`.
- Add `tauri-plugin-opener` to `Cargo.toml` and register it in `lib.rs`; add `opener:default` + `opener:allow-open-url` + `opener:allow-reveal-item-in-dir` to `capabilities/default.json`.
- Drop `zbus` direct dependency from `updater.rs` (note: `keyring` crate pulls its own D-Bus stack on Linux via `async-secret-service`; the `zbus` dep entry in `Cargo.toml` is owned by the reveal path — once that path moves to opener, the explicit `zbus` line can be removed; keyring's transitive pull stays).
- Remove `xdg-open` and `xdg-user-dir` from the `missing_binaries` health check; add an opener-level smoke-test if needed.

## Capabilities

### New Capabilities

- `platform-desktop-integration`: Cross-platform open-file/open-url, reveal-in-file-manager, and desktop notifications via Tauri `opener` and `notification` plugins. Covers all six call sites listed above (updater, commands, linear, clickup, scratchpad) under a single, testable contract that passes on Linux and macOS (Windows: later pass).

### Modified Capabilities

- `scratchpad`: The reveal-in-file-manager behavior (`scratchpad_reveal_in_file_manager`) changes implementation from `xdg-open` to `opener::reveal_item_in_dir`. Externally observable behavior is identical; requirement text needs a platform-neutral formulation.

## Impact

- **Rust**: `src-tauri/src/updater.rs` (6 sites), `src-tauri/src/commands.rs` (2 sites), `src-tauri/src/linear/mod.rs` (1 site), `src-tauri/src/clickup/poller.rs` (1 site), `src-tauri/src/scratchpad/commands.rs` (1 site).
- **Cargo**: add `tauri-plugin-opener = "2"` to `[dependencies]`; remove the top-level `zbus` line (after confirming it is not used elsewhere outside `updater.rs`).
- **Capabilities**: `src-tauri/capabilities/default.json` — add opener permissions.
- **lib.rs**: `.plugin(tauri_plugin_opener::init())` registration.
- **No TypeScript changes** — all six call sites are Tauri commands invoked from the backend; the frontend API surface is unchanged.
- **Sibling change `platform-compile`** handles the broader compile-time seams (unix sockets, FIFOs, /proc) — this change is scoped to desktop integration only.
