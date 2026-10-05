## ADDED Requirements

### Requirement: Detached background spawn

The system SHALL spawn the post-session runner (`nergal post-session`) and the docker-compose-stop command **detached** so they outlive the GUI process, via a per-platform mechanism selected at compile time. On **Unix** detachment SHALL use POSIX `setsid` in a `pre_exec` closure (`#[cfg(target_os = "linux")]` for the post-session runner today; `#[cfg(unix)]` for docker-stop). On **Windows** detachment SHALL use `std::os::windows::process::CommandExt::creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP)` so the spawned process is not tied to the GUI's console/process group and survives GUI exit. The macOS post-session path remains a no-op (a pre-existing gap predating this change, explicitly out of scope here).

#### Scenario: Post-session runner survives GUI exit on Windows

- **WHEN** a session ends on Windows and `spawn_runner_detached` is called, then the GUI process exits
- **THEN** the `nergal post-session` process SHALL have been spawned with `DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP` and SHALL continue running to completion after the GUI exits, rather than being a no-op (the prior non-Linux behaviour) or being killed with the GUI

#### Scenario: Docker-compose stop completes after Nergal exits on Windows

- **WHEN** Nergal is quitting on Windows and a docker-compose stop is dispatched
- **THEN** the stop command SHALL be spawned detached (`creation_flags`) so it finishes stopping every service even though Nergal exits mid-stop

#### Scenario: Unix detachment unchanged

- **WHEN** the post-session runner or docker-stop is spawned on Linux/macOS
- **THEN** the existing `setsid` behaviour SHALL be unchanged (no regression)

---

### Requirement: Desktop integration works on Windows via the cross-platform plugins

The open/reveal/downloads/notification/health-check surfaces SHALL function on Windows through the same `tauri-plugin-opener`, `tauri-plugin-notification`, and `dirs`-crate abstractions used on macOS, with no Windows-specific subprocess calls and no reintroduced `xdg-*`/D-Bus path. Native toast notifications and the `nergal://` deep link additionally depend on install-time registration (an AppUserModelID and a scheme registry entry) provided by the Windows installer (`windows-bundle-ci`).

#### Scenario: Open, reveal, and downloads resolve on Windows

- **WHEN** the user opens the log file or a `nergal://`/`obsidian://` URI, reveals a downloaded file, or the updater resolves the downloads directory on Windows
- **THEN** the open/URL SHALL dispatch via `tauri-plugin-opener` (`open_path`/`open_url`; `ShellExecute` honours the Windows file/scheme association), the reveal SHALL open Explorer with the file selected via `opener::reveal_item_in_dir`, and the downloads dir SHALL resolve via `dirs::download_dir()` — none spawning a subprocess or a D-Bus call

#### Scenario: Notifications display on Windows when the AUMID is registered

- **WHEN** `send_notification` (or a Linear/ClickUp assignment) fires on Windows and the app's AppUserModelID has been registered by the installer
- **THEN** the OS SHALL display a WinRT toast via `tauri-plugin-notification` (no `notify-send`), and a plugin failure SHALL flow through the non-fatal `tracing::warn!` path without panicking

#### Scenario: Health check does not require xdg binaries on Windows

- **WHEN** `check_system_health` runs on Windows
- **THEN** the result SHALL NOT list `xdg-open`/`xdg-user-dir` as missing, and `git` SHALL still be listed if absent

#### Scenario: Notification + deep-link install prerequisites are recorded

- **WHEN** the Windows bundle is produced (`windows-bundle-ci`)
- **THEN** it SHALL register the AppUserModelID (for toast notifications) and the `nergal://` scheme (for the deep link); this change documents that dependency, and the runtime code SHALL degrade gracefully (warn-and-continue) if the registration is absent
