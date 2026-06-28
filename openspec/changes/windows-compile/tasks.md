# Tasks — windows-compile

Authoritative site list: `handoff/ungated-unix-analysis.md`. Order per `implementation.md`.

## 1. Cargo: keyring Windows backend

- [ ] 1.1 Add `[target.'cfg(target_os = "windows")'.dependencies.keyring]` (`version = "3"`, `features = ["windows-native"]` — NO `default-features = false`, matching the macOS block style) after the macOS keyring block in `Cargo.toml`.
- [ ] 1.2 Refresh `Cargo.lock` (`cargo update -p keyring --precise <current>` not needed — just `cargo check` to re-resolve). Confirm Linux/macOS keyring feature sets are byte-identical (no diff outside the new Windows block).

## 2. Mechanical seam fixes (isolated)

- [ ] 2.1 `commands.rs:220-224` — replace `if cfg!(unix) { use std::os::unix::fs::PermissionsExt; … }` with a `#[cfg(unix)]` / `#[cfg(not(unix))]` block (Windows falls back to `is_file`).
- [ ] 2.2 `clickup/auth.rs:155,170` — move `OpenOptionsExt` + `.mode(0o600)` into a `#[cfg(unix)]` block; `#[cfg(not(unix))]` builds `OpenOptions` without `.mode()`. Preserve the "less secure than keyring" warning.
- [ ] 2.3 `linear/auth.rs:233,252` — same treatment as 2.2.

## 3. free_disk_bytes gate + stub

- [ ] 3.1 `mcp/worktree_sessions.rs` — gate the `OsStrExt`/`libc::statvfs` body of `free_disk_bytes` `#[cfg(unix)]`; add `#[cfg(not(unix))]` returning the `0`/`None` sentinel. Add a `// WHY` note pointing at `windows-proc` for the functional `GetDiskFreeSpaceExW`/`sysinfo::Disks` impl.

## 4. MCP transport + shim cluster (lands together)

Strategy (iprev #5): gate the transport types `#[cfg(unix)]` with **no Windows stub surface**; the only Windows surface is the `shim::run` stub. No `#[cfg(windows)]` fn may name `UnixStream`.

- [ ] 4.1 `mcp/transport.rs` — gate `use tokio::net::{UnixListener, UnixStream};` (`:18`), the `UnixSocketTransport` struct + `impl` (`bind`/`accept`/`connect`), and the free `connect` fn `#[cfg(unix)]`.
- [ ] 4.2 `mcp/transport.rs` — gate/delete BOTH `peer_uid` fns (`#[cfg(unix)]` real at `:115` and the broken `#[cfg(not(unix))]` stub at `:122` that names `UnixStream`); no Windows caller needs `peer_uid`, so no Windows stub. Fixes the latent break.
- [ ] 4.3 `mcp/mod.rs` — gate `serve` (`:587`), `handle_connection` (`:619`), `write_response` (`:671`) `#[cfg(unix)]`. No `#[cfg(windows)]` stub (only caller is the gated daemon block).
- [ ] 4.4 `mcp/shim.rs` — gate `relay` (`:115`), `run` (`:20`), `run_async` (`:25`, binds `daemon: Option<UnixStream>` via `transport::connect` at `:44`) `#[cfg(unix)]`. Add a `#[cfg(windows)] pub fn run() -> anyhow::Result<()>` stub (stderr note + `Ok(())`) so `main.rs:135` resolves. **(iprev #1 — was missed.)**
- [ ] 4.5 `lib.rs:692-714` — gate the MCP daemon block **including the `mcp_*` local clones at `:692-695`** (not just the spawn at `:696-714`) `#[cfg(unix)]`, so they are not `unused_variable` on Windows (iprev #4). Removes the ungated `libc::getuid()` at `:700`.

## 5. Hook server gate + stub

- [ ] 5.1 `hooks/server.rs:7,222` — gate `use tokio::net::UnixListener;` + the bind path `#[cfg(unix)]`; internal perms (`:229-232`) + `libc::getuid` (`:240-241`) blocks compile-exclude on Windows naturally.
- [ ] 5.2 Add a `#[cfg(windows)]` `start_hook_server` stub returning `Unsupported`, or gate its single caller in `lib.rs`.

## 6. platform/mod.rs sync_connect Windows stub

- [ ] 6.1 Add a `#[cfg(windows)]` `sync_connect` returning `io::Result<UnsupportedSyncStream>` where `UnsupportedSyncStream` is a never-constructed newtype implementing `std::io::Read + Write` (all methods → `Err(Unsupported)`). The concrete `Read+Write` type is REQUIRED — the hook-CLI callers (`cli.rs:15,46,235,457,509`) drive the returned value with sync `Read`/`Write`, so a bare `Err` without a concrete `T` does not type-check (iprev #2). NOT `TcpStream`. Real named-pipe sync connect is `windows-ipc`.

## 7. CI Windows gate

- [ ] 7.1 `.github/workflows/ci.yml` — add a `windows-check` job: `runs-on: windows-latest`, `rustup target add x86_64-pc-windows-msvc`, `cargo check --target x86_64-pc-windows-msvc`, path-filtered to `src-tauri/**`. No GStreamer/webkit system deps (Windows uses bundled WebView2).
- [ ] 7.2 Iterate on the gate until green: the first run may surface `arboard`/`wezterm`/`tao` per-target gating not predictable locally — fix as additive Cargo gates within this change.

## 8. CLAUDE.md convention

- [ ] 8.1 Update the "Cross-platform invariant" section: the Windows gate now exists; macOS catches Linux-only regressions, Windows catches ungated-unix seams (complementary). Remove the "deferred Windows gate" hedge.

## 9. Verification

- [ ] 9.1 **Windows gate green** — `cargo check --target x86_64-pc-windows-msvc` on `windows-latest` (CI; cannot run on the Linux dev host — `ring`/MSVC). Authoritative pass/fail for this change.
- [ ] 9.2 **macOS gate green** — `cargo check --target aarch64-apple-darwin` (CI, unchanged).
- [ ] 9.3 **Linux full check** — `cd src-tauri && cargo clippy -- -D warnings && cargo test && cargo fmt --check` + `npx tsc --noEmit`.
- [ ] 9.4 **Linux keyring round-trip** — store + read back a ClickUp/Linear token via the Secret Service backend (regression guard for the Cargo target-table edit).
- [ ] 9.5 **No ungated unix seams remain** — re-run `rg "std::os::unix" src/` + `rg "libc::" src/` and confirm every production hit is inside a `#[cfg]` gate (Category-A list in `handoff/ungated-unix-analysis.md` fully consumed).
