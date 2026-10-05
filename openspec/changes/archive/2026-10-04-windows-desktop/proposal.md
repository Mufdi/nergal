## Why

The macOS port replaced Linux subprocess desktop calls (`xdg-open`, `gtk-launch`, `notify-send`, `xdg-user-dir`, D-Bus `FileManager1`) with cross-platform abstractions — the `tauri-plugin-opener`, `tauri-plugin-notification`, and the `dirs` crate. Those abstractions **already cover Windows** (Tauri's plugins are first-class on Windows; `dirs` resolves Windows known-folders), so most of desktop integration works on Windows with no new code. Two seams remain Windows-specific: the **detached background spawn** (the `spawn_runner_detached` + docker-stop paths use POSIX `setsid`, which has no Windows analog and is currently a non-Linux no-op) and the **install-time prerequisites** for native notifications and the `nergal://` deep link (an AppUserModelID and a scheme registry entry, which on Windows are set by the installer — `windows-bundle-ci`).

This change supplies the Windows detached spawn, confirms + specs the already-working plugin paths on Windows, and records the install-time prerequisites as a dependency on `windows-bundle-ci`.

## What Changes

- **`#[cfg(windows)]` detached background spawn** — `spawn_runner_detached` (`obsidian/post_session.rs`) currently has a `#[cfg(not(target_os = "linux"))]` no-op stub (so the post-session runner does not spawn on macOS **or** Windows today). Split it into a `#[cfg(windows)]` real detached spawn (`std::os::windows::process::CommandExt::creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP)`, so `nergal post-session` outlives the GUI on Windows) + a `#[cfg(not(any(windows, target_os = "linux")))]` no-op catch-all (covers macOS and every other non-linux/non-windows target — the call sites are ungated, so the catch-all is required, not a bare `cfg(macos)`). (The macOS no-op is a **pre-existing, out-of-scope** gap — this change does not fix macOS, only fills Windows.)
- **`#[cfg(windows)]` docker-stop detach** — the `pty.rs` docker-compose-stop spawn has a `#[cfg(unix)]` `setsid` detach with no non-unix arm; add a `#[cfg(windows)]` arm using `creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP)` so the stop survives Nergal's exit on Windows too.
- **Confirm + spec the already-working Windows plugin paths** (no new code):
  - **Open file/URL/log** via `tauri-plugin-opener` — the `#[cfg(not(target_os = "linux"))]` `open_log_path` branch (from `windows-compile`) uses the opener plugin; on Windows `ShellExecute` respects the `.log` file association. URL + custom-scheme (`nergal://`/`obsidian://`) dispatch via `opener::open_url`.
  - **Reveal in file manager** via `opener::reveal_item_in_dir` — opens Explorer with the file selected on Windows (the D-Bus path is Linux-only).
  - **Downloads / cache dirs** via `dirs::download_dir()` / `dirs::cache_dir()` — Windows known-folders.
  - **Desktop notifications** via `tauri-plugin-notification` — WinRT toast on Windows (the `notify-send` fallback is Linux-gated). **Caveat**: Windows toast notifications require a registered AppUserModelID, set by the installer (dependency on `windows-bundle-ci`).
  - **System health check** — `xdg-open`/`xdg-user-dir` already removed from `missing_binaries`; on Windows neither is checked, `git` stays.
- **Record install-time prerequisites** (provided by `windows-bundle-ci`, not this change): the AppUserModelID for toast notifications, and the `nergal://` scheme registry registration (the `tauri-plugin-deep-link` plugin registers the scheme in HKCU at runtime on Windows, but the installer association is the robust path). Documented so the notification + deep-link features are known to depend on the bundle.

## Capabilities

### New Capabilities

(none)

### Modified Capabilities

- `platform-desktop-integration`: extends the contract to Windows. **Modifies** the open/reveal/downloads/notification/health requirements to assert Windows coverage via the same plugins, and **adds** a "Detached background spawn" requirement covering the per-platform detach mechanism (Unix `setsid`, Windows `DETACHED_PROCESS` creation flags, with the macOS no-op noted as a pre-existing gap).

## Impact

- **`src-tauri/src/obsidian/post_session.rs`**: the `#[cfg(not(target_os = "linux"))]` `spawn_runner_detached` no-op → a `#[cfg(windows)]` real detached spawn (`creation_flags`) + a `#[cfg(not(any(windows, target_os = "linux")))]` no-op catch-all retaining today's macOS (and other non-linux/non-windows) behaviour.
- **`src-tauri/src/pty.rs`**: add a `#[cfg(windows)]` arm to the docker-stop detach (`creation_flags`).
- **No change** to the opener/notification/dirs/health-check code (already cross-platform); the spec is extended to assert Windows.
- **`Cargo.toml`**: no new dep (the detach uses `std::os::windows`, no crate).
- **Depends on `windows-bundle-ci`**: AppUserModelID (toast notifications) + `nergal://` scheme registration.
- **Out of scope**: compile gate (`windows-compile`), IPC (`windows-ipc`), process/port (`windows-proc`), bundling (`windows-bundle-ci`), and the **macOS `spawn_runner_detached` no-op** (pre-existing gap, separate follow-up).
