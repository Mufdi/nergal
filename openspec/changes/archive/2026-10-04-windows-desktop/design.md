# Design — windows-desktop

## Context

The macOS port replaced every Linux desktop subprocess with a cross-platform plugin/crate (`tauri-plugin-opener`, `tauri-plugin-notification`, `dirs`). Those are first-class on Windows, so open/reveal/URL/downloads/notification/health all work on Windows unchanged. The only genuinely-Windows code is the **detached spawn** (POSIX `setsid` has no Windows analog); the rest is confirmation + recording the install-time prerequisites the bundle owns.

## Decision 1 — Windows detach via `creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP)`

`spawn_runner_detached` (`obsidian/post_session.rs:112`) is `#[cfg(target_os = "linux")]` (real, `setsid`) + `#[cfg(not(target_os = "linux"))]` (a no-op `Ok(())` — so the post-session runner spawns on neither macOS nor Windows today). The docker-stop detach (`pty.rs:181`) is `#[cfg(unix)]` `setsid` with no non-unix arm.

**Chosen**: split the non-Linux stub into a `#[cfg(windows)]` real spawn and a `#[cfg(not(any(windows, target_os = "linux")))]` no-op catch-all. The catch-all (NOT a bare `cfg(target_os = "macos")`) keeps a definition on every non-linux/non-windows target — the call sites (`lib.rs:1032`, `hooks/server.rs:551`) are ungated, so a bare macOS arm would leave FreeBSD/other-unix with an `E0425`; the catch-all also matches the sibling `probe_spawn_health` idiom (`:187`) and the CLAUDE.md "non-Linux stub, never ungated" invariant. The Windows spawn uses `std::os::windows::process::CommandExt::creation_flags` with `DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP` (no new crate — these constants are stable `u32`s, or pulled from the `windows` crate that #2 already brings). `DETACHED_PROCESS` runs the child without a console; `CREATE_NEW_PROCESS_GROUP` detaches it from the parent's Ctrl-C/console group, the closest analog to `setsid`'s session detachment. Same arm added to the docker-stop spawn.

- *Alternative: keep the no-op on Windows (match macOS).* **Rejected** — the post-session runner (obsidian session logging) is a real feature; a no-op silently disables it on Windows. The Windows detach is a one-line `creation_flags` with no new dependency, so there is no reason to punt as the macOS path did.
- *Note (macOS gap):* the macOS `spawn_runner_detached` stays a no-op — that is a **pre-existing** gap (macOS has `setsid`, so it *could* be gated `#[cfg(unix)]`, but the macOS port chose not to). Fixing macOS is out of scope for a Windows change; flagged for a follow-up.

**Risk (verify on the Windows machine):** if the Tauri GUI runs its children inside a Windows **job object** with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`, a `DETACHED_PROCESS` child is still killed when the GUI exits. If the walk shows the post-session runner dying with the GUI, add `CREATE_BREAKAWAY_FROM_JOB` to the creation flags (requires the job to permit breakaway). Recorded so the implementer checks the actual behaviour rather than assuming.

## Decision 2 — Everything else relies on the cross-platform plugins (no code)

`opener::{open_path, open_url, reveal_item_in_dir}`, `tauri_plugin_notification`, `dirs::{download_dir, cache_dir}`, and the `missing_binaries` health check are already cross-platform after the macOS port. On Windows: `open_path`/`open_url` → `ShellExecute` (honours file/scheme associations); `reveal_item_in_dir` → Explorer `/select`; notifications → WinRT toast; dirs → known-folders. **Chosen**: no new code; the spec is extended to assert Windows coverage and the walk confirms it.

## Decision 3 — Install-time prerequisites are owned by `windows-bundle-ci`, runtime degrades gracefully

Windows toast notifications require a registered **AppUserModelID**, and the `nergal://` deep link requires a **scheme registry entry**. On Windows these are install-time concerns (the installer registers them), so they belong to `windows-bundle-ci`, not here. **Chosen**: record the dependency in the spec; the runtime notification path already degrades to a non-fatal `tracing::warn!` if the toast fails (the spec's "Notification plugin failure is non-fatal" requirement), so an unregistered AUMID does not crash — it just doesn't display until the app is installed. (`tauri-plugin-deep-link` can also register the scheme in HKCU at first run, so the deep link may work pre-install; the installer association is the robust path.)

## Risks / Trade-offs

- **[Medium] Job-object kill-on-close** may defeat `DETACHED_PROCESS` (Decision 1 risk) — verify on the Windows machine, add `CREATE_BREAKAWAY_FROM_JOB` if needed.
- **[Low] Toast needs the AUMID from the installer** — degrades to warn-and-continue until installed; not a crash.
- **[Low] No local validation** — `windows-latest` CI compiles; the user's Windows machine walks open/reveal/notify/post-session.

## Migration / rollback

Additive `#[cfg(windows)]` arms on two spawn sites; no Cargo change. Git-revertible (revert restores the non-Linux no-op / unix-only detach).
