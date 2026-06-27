## ADDED Requirements

### Requirement: Open file or URL via Tauri opener plugin

The system SHALL open files and URLs using `tauri-plugin-opener` (`opener::open_path` for filesystem paths, `opener::open_url` for URLs and custom-scheme URIs). Direct subprocess invocations of `xdg-open` and `gtk-launch` SHALL NOT appear in any non-test source file outside `migrate_legacy.rs`.

#### Scenario: Open log file on macOS

- **WHEN** the user triggers "Open log file" from the About panel on macOS
- **THEN** the system SHALL open the log file with the OS-default text handler via the opener plugin without spawning `xdg-open` or `gtk-launch`

#### Scenario: Open log file on Linux

- **WHEN** the user triggers "Open log file" from the About panel on Linux
- **THEN** the system SHALL open the log file with the OS-default text handler via the opener plugin without spawning `xdg-open` or `gtk-launch`

#### Scenario: Open custom-scheme URI

- **WHEN** `obsidian_open_uri` is called with a URI starting with `obsidian://` or `nergal://`
- **THEN** the system SHALL dispatch it via `opener::open_url` and the OS SHALL handle the URI with the registered scheme handler
- **AND** URIs not matching the allowlist (`obsidian://` or `nergal://`) SHALL be rejected with an error before any open call is made

### Requirement: Resolve downloads directory without a subprocess

The system SHALL resolve the user's downloads directory using `dirs::download_dir()` instead of spawning `xdg-user-dir DOWNLOAD`. When `dirs::download_dir()` returns `None` the system SHALL fall back to `$HOME/Downloads`.

#### Scenario: Downloads directory resolved on macOS

- **WHEN** the updater needs to determine the download destination on macOS
- **THEN** the system SHALL return the OS-standard downloads path (`~/Downloads` or the user-customized equivalent) without spawning any subprocess

#### Scenario: Downloads directory resolved on Linux

- **WHEN** the updater needs to determine the download destination on Linux
- **THEN** the system SHALL return the path that `dirs::download_dir()` resolves, or `$HOME/Downloads` as fallback, without spawning `xdg-user-dir`

#### Scenario: Fallback when dirs returns None

- **WHEN** `dirs::download_dir()` returns `None`
- **THEN** the system SHALL use `$HOME/Downloads` as the download destination

### Requirement: Reveal file in file manager via Tauri opener plugin

The system SHALL reveal files in the native file manager using `opener::reveal_item_in_dir` (for the downloaded-file case, whose argument is a file). The `show_items_via_dbus` function (D-Bus `org.freedesktop.FileManager1` `ShowItems`, now `#[cfg(target_os = "linux")]`) and its `xdg-open <dir>` fallback in `reveal_in_downloads` SHALL be removed. The Linux-gated `zbus` dependency under `[target.'cfg(target_os = "linux")'.dependencies]` in `Cargo.toml` SHALL be removed once confirmed unused outside the replaced call sites (note: the `keyring` crate retains its own transitive D-Bus stack on Linux).

#### Scenario: Reveal downloaded file on macOS

- **WHEN** the user clicks "Reveal in Downloads" after an update download completes on macOS
- **THEN** the system SHALL open Finder with the downloaded file highlighted, without any D-Bus call or `xdg-open` subprocess

#### Scenario: Reveal downloaded file on Linux

- **WHEN** the user clicks "Reveal in Downloads" after an update download completes on Linux
- **THEN** the system SHALL open the native file manager with the downloaded file highlighted using `opener::reveal_item_in_dir`

#### Scenario: Reveal fallback when file no longer exists

- **WHEN** `reveal_in_downloads` is called for a path that no longer exists on disk
- **THEN** the system SHALL return an error (`"downloaded file no longer exists"`) without attempting any open or reveal call

### Requirement: Desktop notifications via Tauri notification plugin

The system SHALL send desktop notifications through a single shared helper. The three existing `notify-send` call sites (general `send_notification` in `commands.rs`, Linear assignment in `linear/mod.rs`, ClickUp assignment in `clickup/poller.rs`) SHALL route through this helper and SHALL NOT spawn `notify-send` directly.

On **macOS** the helper SHALL use `tauri_plugin_notification::NotificationExt::notification().show()` exclusively; no `notify-send` invocation SHALL appear.

On **Linux** the helper's implementation SHALL be chosen from an empirical verification of whether the plugin's `.show()` actually displays a notification on the supported WebKitGTK build (the historical regression was a *silent* failure — `.show()` returned `Ok(())` while displaying nothing, which is not detectable at runtime):
- If the plugin is verified to display, the helper SHALL use the plugin and no `notify-send` SHALL remain.
- If the plugin still fails silently, the helper SHALL use a `#[cfg(target_os = "linux")]`-gated `notify-send` spawn as the primary Linux path (the prior behavior), and the plugin SHALL NOT be relied upon on Linux.

A runtime `Ok`/`Err` toggle SHALL NOT be used as the silent-failure guard. Any residual `notify-send` SHALL exist only inside the gated helper, never at the three call sites.

#### Scenario: General notification on macOS

- **WHEN** `send_notification` is called with a title and body on macOS
- **THEN** the OS SHALL display a native notification via the notification plugin without spawning `notify-send`

#### Scenario: General notification on Linux

- **WHEN** `send_notification` is called with a title and body on Linux
- **THEN** the OS SHALL display a native notification via the shared helper (mechanism — plugin or gated `notify-send` — per the empirical-verification scenario)

#### Scenario: Linear assignment notification

- **WHEN** the Linear poller detects a new issue assigned to the user
- **THEN** the system SHALL emit a desktop notification titled "Linear" through the shared helper (and still emit the `linear:assigned` Tauri event for the in-app toast)

#### Scenario: ClickUp assignment notification

- **WHEN** the ClickUp poller detects a new task assigned to the user
- **THEN** the system SHALL emit a desktop notification through the shared helper (and still emit the `clickup:changed` Tauri event for the in-app toast)

#### Scenario: Notification plugin failure is non-fatal

- **WHEN** the notification plugin call fails (e.g., permission denied on macOS Sandbox)
- **THEN** the system SHALL log a warning at `tracing::warn!` level and continue; it SHALL NOT panic or return an error to the caller

#### Scenario: Linux helper implementation chosen by empirical verification

- **WHEN** the Linux notification path is implemented and the plugin's `.show()` has been verified against the supported WebKitGTK build
- **THEN** if the plugin was observed to display, the helper SHALL use the plugin and retain no `notify-send`
- **AND** if the plugin was observed to fail silently, the helper SHALL use a `#[cfg(target_os = "linux")]`-gated `notify-send` spawn as the primary Linux path
- **AND** the three notification call sites SHALL NOT spawn `notify-send` directly (any residual `notify-send` lives only inside the gated helper)

#### Scenario: macOS notification permission

- **WHEN** notifications are first used on macOS and permission has not yet been granted or denied
- **THEN** the system SHALL request notification permission via the plugin (`request_permission`)
- **AND** a denied permission SHALL flow through the non-fatal warn path without panicking or erroring to the caller

### Requirement: System health check reflects new platform model

The system SHALL remove `xdg-open` and `xdg-user-dir` from the `missing_binaries` health check in `check_system_health`. Neither binary is required when the opener plugin handles all open/reveal calls. `git` SHALL remain in the check (it is used directly by the version-control paths, not replaced by any plugin).

#### Scenario: Health check on macOS

- **WHEN** `check_system_health` is called on macOS where `xdg-open` is absent
- **THEN** the result SHALL NOT list `xdg-open` or `xdg-user-dir` as missing binaries
- **AND** the result SHALL still list `git` as missing if `git` is not installed

#### Scenario: Health check on Linux without xdg-utils

- **WHEN** `check_system_health` is called on a minimal Linux install without `xdg-open`
- **THEN** the result SHALL NOT list `xdg-open` as a missing binary
