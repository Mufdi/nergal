# Tasks — windows-desktop

Depends on `windows-compile` (compile gate). Notification AUMID + `nergal://` scheme registration are owned by `windows-bundle-ci`.

## 1. Windows detached spawn

- [ ] 1.1 `obsidian/post_session.rs` — split the `#[cfg(not(target_os = "linux"))]` no-op into a `#[cfg(windows)]` real spawn (`creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP)`, re-exec `current_exe() post-session`, null stdio) + a `#[cfg(not(any(windows, target_os = "linux")))]` no-op (covers macOS AND every other non-linux/non-windows target — the call sites are ungated, so a bare `cfg(target_os="macos")` would leave FreeBSD undefined; matches the sibling `probe_spawn_health` idiom). Note the `CREATE_BREAKAWAY_FROM_JOB` fallback if a Tauri job object kills the child (verify on the Windows machine).
- [ ] 1.2 `pty.rs:181` — add a `#[cfg(windows)]` arm to the docker-stop spawn setting `creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP)` (alongside the existing `#[cfg(unix)]` `setsid` block).

## 2. Confirm cross-platform plugin paths on Windows (no code)

- [ ] 2.1 Verify (Windows gate + walk) that `opener` (open log / open `nergal://` URI / reveal downloaded file), `tauri-plugin-notification` (post-install AUMID), `dirs` (downloads/cache), and `check_system_health` (no xdg-*) all work on Windows. No code change expected; record any surprise as a follow-up.

## 3. Verification

- [ ] 3.1 **Windows gate green** — `cargo check --target x86_64-pc-windows-msvc`.
- [ ] 3.2 **Linux full check** — `cd src-tauri && cargo clippy -- -D warnings && cargo test && cargo fmt --check` (the `#[cfg(target_os = "linux")]`/`#[cfg(unix)]` detach paths untouched).
- [ ] 3.3 **macOS gate green** — `cargo check --target aarch64-apple-darwin` (the `#[cfg(not(any(windows, target_os = "linux")))]` no-op catch-all compiles).
- [ ] 3.4 **User Windows-machine walk (UNVERIFIED-pending)** — post-session runner survives GUI exit (add BREAKAWAY if a job object kills it); docker-stop completes after Nergal exit; open-log / reveal / open-`nergal://` / notification (post-install) / downloads all work.
