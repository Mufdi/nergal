## Why

Nergal's open-file, reveal-in-file-manager, and desktop-notification paths are implemented with Linux-only shell commands (`xdg-open`, `xdg-user-dir`, `gtk-launch`, `notify-send`) and a D-Bus `org.freedesktop.FileManager1` call. The sibling change `platform-compile` (already shipped) made the tree *compile* on macOS by gating the D-Bus/`zbus` reveal path to `#[cfg(target_os = "linux")]` and leaving the non-Linux reveal as a log-only no-op — explicitly deferring the real cross-platform reveal to **this** change. The remaining Linux-only paths (`xdg-open`/`xdg-user-dir`/`gtk-launch` in `updater.rs` + `commands.rs` + `scratchpad`, and `notify-send` at three sites) still compile on macOS but are meaningless there (the binaries do not exist), so they fail at runtime. Tauri ships first-party plugins (`opener`, `notification`) that provide the same three operations cross-platform; replacing the ad-hoc shell invocations unblocks macOS support without forking any logic per OS.

## What Changes

- Replace `xdg-open`/`gtk-launch` in `updater.rs` (open log file, open directory as fallback) with `tauri-plugin-opener`'s `open_path` / `open_url`.
- Replace `xdg-user-dir DOWNLOAD` in `updater.rs` with `dirs::download_dir()` (already a direct dep) — no subprocess required.
- Remove `xdg-open`/`xdg-mime`/`gtk-launch` probe from `check_system_health` in `updater.rs`; neither is required when the opener plugin handles the calls.
- Replace `zbus` `org.freedesktop.FileManager1` `ShowItems` call in `updater.rs` (`show_items_via_dbus` + `reveal_in_downloads`) with `opener::reveal_item_in_dir`.
- Route the three `notify-send` sites (`commands.rs` `send_notification`, `linear/mod.rs`, `clickup/poller.rs`) through a single shared notify helper. macOS uses `tauri_plugin_notification::NotificationExt::notification().show()`; the **Linux** mechanism (plugin vs. a gated `notify-send`) is decided by the task 6.0 empirical gate, because the historical regression was a *silent* plugin failure under WebKitGTK (undetectable at runtime) — see Decision 3/3a.
- Replace `xdg-open <uri>` in `commands.rs` (`obsidian_open_uri`) with `opener::open_url`.
- Replace `xdg-open <dir>` in `scratchpad/commands.rs` (`scratchpad_reveal_in_file_manager`) with `opener::open_path` (the current `xdg-open <dir>` *opens the folder's contents*; `open_path(dir)` preserves that, whereas `reveal_item_in_dir(dir)` would instead highlight the dir inside its parent — a behavior change). See Decision 1's mapping table for the file-vs-directory distinction.
- Add `tauri-plugin-opener` to `Cargo.toml` and register it in `lib.rs`. Note: all six call sites invoke the plugin **from Rust** via the `OpenerExt` trait, which bypasses the capability/scope ACL (that ACL gates only the JS/IPC command layer, which the frontend does not use here). The `opener:default` + `opener:allow-open-url` + `opener:allow-reveal-item-in-dir` entries in `capabilities/default.json` are therefore **defensive/future-proofing** (in case a future frontend call needs the JS API), not load-bearing for these Rust calls — see Decision 6.
- Drop the Linux-gated `zbus` direct dependency. **Verified against current source**: `platform-compile` already moved `zbus` out of top-level `[dependencies]` into `[target.'cfg(target_os = "linux")'.dependencies]` (`Cargo.toml` line 116, `zbus = { version = "5", ... }`) with a comment block (lines 111-115) explicitly handing the reveal path to this change. Once `show_items_via_dbus` (now itself `#[cfg(target_os = "linux")]`) is removed, that Linux-gated `zbus` entry + its comment can be deleted. `keyring`'s `async-secret-service` feature still pulls `zbus` transitively on Linux — this is NOT a full `zbus` removal, only the direct dep.
- Remove `xdg-open` and `xdg-user-dir` from the `missing_binaries` health check; add an opener-level smoke-test if needed.

## Capabilities

### New Capabilities

- `platform-desktop-integration`: Cross-platform open-file/open-url, reveal-in-file-manager, and desktop notifications via Tauri `opener` and `notification` plugins. Covers all six call sites listed above (updater, commands, linear, clickup, scratchpad) under a single, testable contract that passes on Linux and macOS (Windows: later pass).

### Modified Capabilities

- `scratchpad`: The reveal-in-file-manager behavior (`scratchpad_reveal_in_file_manager`) changes implementation from `xdg-open <dir>` to `opener::open_path(dir, None)`. Observable behavior is preserved (the scratchpad folder opens in the native file manager). `open_path` is chosen over `reveal_item_in_dir` precisely to keep that "open the folder's contents" semantics — `reveal_item_in_dir(dir)` would instead select the folder icon inside its parent. The requirement text is reformulated platform-neutrally.

## Impact

- **Rust**: `src-tauri/src/updater.rs` (6 sites), `src-tauri/src/commands.rs` (2 sites), `src-tauri/src/linear/mod.rs` (1 site), `src-tauri/src/clickup/poller.rs` (1 site), `src-tauri/src/scratchpad/commands.rs` (1 site).
- **Cargo**: add `tauri-plugin-opener = "2"` to `[dependencies]`; remove the Linux-gated `zbus` line at `Cargo.toml:116` (under `[target.'cfg(target_os = "linux")'.dependencies]`) plus its comment block (111-115), after confirming it is not used elsewhere outside `updater.rs`. This affects only the Linux build (macOS never had a direct `zbus` dep).
- **Capabilities**: `src-tauri/capabilities/default.json` — add opener permissions.
- **lib.rs**: `.plugin(tauri_plugin_opener::init())` registration.
- **No TypeScript changes** — all six call sites are Tauri commands invoked from the backend; the frontend API surface is unchanged.
- **Sibling change `platform-compile`** handles the broader compile-time seams (unix sockets, FIFOs, /proc) — this change is scoped to desktop integration only.
