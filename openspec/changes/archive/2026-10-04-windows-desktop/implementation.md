# Implementation — windows-desktop

No SQLite schema changes. Two `#[cfg(windows)]` spawn arms + spec confirmation.

## Verified codebase facts (do not re-assume)

Verified against current source 2026-06-28:

- **`spawn_runner_detached`** (`obsidian/post_session.rs:112`): `#[cfg(target_os = "linux")]` real impl (`pre_exec` + `libc::setsid`), `#[cfg(not(target_os = "linux"))]` no-op `Ok(())` (`:139`). So the post-session runner spawns on **neither** macOS nor Windows today.
- **docker-stop detach** (`pty.rs:181`): a `#[cfg(unix)]` block doing `pre_exec` + `libc::setsid` before `cmd.spawn()`; **no** non-unix arm (on Windows the spawn happens without detach).
- **Opener / notification / dirs / health** are cross-platform after the macOS port: `app.opener().open_url()` (`commands.rs:3188`), the `#[cfg(not(target_os = "linux"))] open_log_path` opener branch (`updater.rs`, from `windows-compile`), `opener::reveal_item_in_dir`, `dirs::download_dir`/`cache_dir`, and the `missing_binaries` check (xdg-* already removed). No cfg-gating change needed for these on Windows.
- **`creation_flags`** is `std::os::windows::process::CommandExt::creation_flags(u32)` — `DETACHED_PROCESS = 0x0000_0008`, `CREATE_NEW_PROCESS_GROUP = 0x0000_0200`, `CREATE_BREAKAWAY_FROM_JOB = 0x0100_0000` (use the `windows` crate constants #2 brings, or the literals).

## Edit plan

### Step 1 — `spawn_runner_detached` Windows arm
Replace the single `#[cfg(not(target_os = "linux"))]` no-op with two arms:
```rust
#[cfg(windows)]
pub fn spawn_runner_detached() -> Result<()> {
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    let exe = std::env::current_exe().unwrap_or_else(|_| std::path::PathBuf::from("nergal.exe"));
    Command::new(exe)
        .arg("post-session")
        .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null())
        .creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP)
        .spawn()
        .context("spawning detached nergal post-session")?;
    Ok(())
}

#[cfg(not(any(windows, target_os = "linux")))]
pub fn spawn_runner_detached() -> Result<()> { Ok(()) } // macOS + other non-linux/non-windows: pre-existing no-op (out of scope)
```
The no-op uses the `not(any(windows, target_os = "linux"))` catch-all (NOT a bare `cfg(target_os = "macos")`) so every non-Linux/non-Windows target keeps a definition — the call sites (`lib.rs:1032`, `hooks/server.rs:551`) are ungated, so a missing arm is an `E0425` on e.g. FreeBSD. This matches the sibling `probe_spawn_health` idiom (`:187`) and the CLAUDE.md "non-Linux stub, never ungated" invariant.
(If the Windows-machine walk shows the runner dying with the GUI due to a Tauri job object, add `CREATE_BREAKAWAY_FROM_JOB = 0x0100_0000` to the flags — Decision 1 risk.)

### Step 2 — docker-stop Windows arm
In `pty.rs:181`, alongside the `#[cfg(unix)]` `setsid` block, add:
```rust
#[cfg(windows)]
{
    use std::os::windows::process::CommandExt;
    cmd.creation_flags(0x0000_0008 | 0x0000_0200); // DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP
}
```

### Step 3 — confirm the plugin paths on Windows (no code)
Build for Windows (CI) + the user walk: open log file, open a `nergal://` URI, reveal a downloaded file, resolve downloads dir, fire a notification (post-install for the AUMID), run `check_system_health`. No code change expected — record any Windows-specific surprise as a follow-up.

## Verification (maps to tasks.md ## N. Verification)

- Windows gate green (`cargo check --target x86_64-pc-windows-msvc`).
- Linux full check stays green (the `#[cfg(target_os = "linux")]`/`#[cfg(unix)]` detach paths untouched).
- macOS gate green (the `#[cfg(not(any(windows, target_os = "linux")))]` no-op catch-all compiles; behaviour unchanged).
- User Windows-machine walk (UNVERIFIED-pending): post-session runner survives GUI exit (or add BREAKAWAY); docker-stop completes after exit; open-log / reveal / open-URI / notification (post-install) / downloads all work.
