# Implementation — windows-compile

No SQLite schema changes. This is `#[cfg]` gating + Cargo target deps + one CI job. The authoritative site list is `handoff/ungated-unix-analysis.md`; this document maps each cluster to concrete edits and an execution order.

## Verified codebase facts (do not re-assume)

All verified against current source on 2026-06-28:

- **`libc` is `cfg(unix)`** — `Cargo.toml:115-116` `[target.'cfg(unix)'.dependencies] libc = "0.2"`. Absent on Windows. **`zbus`** is `Cargo.toml:118+` under `cfg(target_os = "linux")` — already excludes Windows. **`keyring`** has Linux (`Cargo.toml:131`, `async-secret-service`) and macOS (`Cargo.toml:135`, `apple-native`) target blocks; **no Windows block exists**.
- **`arboard = "3"`** is unconditional (`Cargo.toml:37`). **`fs2 = "0.4"`** is unconditional and has Windows support (`LockFileEx`/`UnlockFile`) — `platform/mod.rs:341,348` `try_lock_exclusive`/`unlock` are safe on Windows. No `nix` crate (all "nix::" rg hits are substrings of `unix::`).
- **`platform/mod.rs` already has the `#[cfg(windows)]` seam stubs**: `PlatformListener` (`:730-754`), `PlatformStream` (`:756-807`) return/`poll` `Unsupported`. `sync_connect` (`:819-822`) is `#[cfg(unix)]` with **no** `#[cfg(not(unix))]` stub yet.
- **`lib.rs:697-700`**: the MCP daemon is spawned unconditionally inside the `.setup()` closure — `UnixSocketTransport::bind(&path)` then `let app_uid = unsafe { libc::getuid() };`. Both ungated.
- **`mcp/transport.rs:18`**: `use tokio::net::{UnixListener, UnixStream};` ungated. `UnixSocketTransport` struct (`:70-71`), `bind` (`:82`), `accept` returns `(UnixStream, u32)` (`:95`), `connect` returns `UnixStream` (`:127-129`) — all ungated. `peer_uid` has a `#[cfg(unix)]` real fn (`:115-119`) **and** a `#[cfg(not(unix))]` stub (`:122-124`) whose `_stream: &UnixStream` parameter type is unavailable on Windows = the latent break. The `#[cfg(unix)]` perms block at `:83-87` is internal to the ungated `bind`.
- **`mcp/mod.rs`**: `serve(transport: transport::UnixSocketTransport, …)` (`:587`), `handle_connection(mut stream: tokio::net::UnixStream, …)` (`:619`), `write_response(stream: &mut tokio::net::UnixStream, …)` (`:671`) — all ungated.
- **`mcp/shim.rs:115`**: `relay(conn: &mut tokio::net::UnixStream, …)` ungated.
- **`hooks/server.rs:7`**: `use tokio::net::UnixListener;` ungated; `:222` `UnixListener::bind(socket_path)?` inside `pub async fn start_hook_server(…)`. Internal `#[cfg(unix)]` blocks at `:229-232` (perms) + `:240-241` (`libc::getuid` app_uid) are skipped on non-unix once the bind is gated.
- **`mcp/worktree_sessions.rs`**: `free_disk_bytes` uses `use std::os::unix::ffi::OsStrExt;` (`:601`) + `libc::statvfs` (`:608-609`), ungated inside the fn.
- **`commands.rs:220-224`**: `let is_executable = if cfg!(unix) { use std::os::unix::fs::PermissionsExt; meta.permissions().mode() & 0o111 != 0 } else { is_file };` — `cfg!(unix)` is a **runtime bool macro**; the `use std::os::unix…` in the taken-or-not branch is type-checked on every target → Windows compile error.
- **`clickup/auth.rs:155,170`** + **`linear/auth.rs:233,252`**: `use std::os::unix::fs::OpenOptionsExt;` + `.mode(0o600)` inside each `write_fallback_file`, ungated.
- **macOS CI gate** is `.github/workflows/ci.yml` job `macos-cross-check` running `cargo check --target aarch64-apple-darwin` on `macos-latest`, path-filtered to `src-tauri/**` (per CLAUDE.md). The Windows job mirrors its shape.
- **`hooks/cli.rs`** plan-review/ask-user FIFO already has a `#[cfg(not(unix))]` shell-`mkfifo` placeholder (`:197-207`) — pre-existing, owned by `windows-ipc`, untouched here.

## Edit plan by cluster (execution order)

Order chosen so the crate moves toward green incrementally; the transport cluster is the largest and lands as one unit because its types are interdependent.

### Step 1 — Cargo.toml: keyring Windows block (lowest risk, isolated)
Add after the macOS keyring block (`Cargo.toml:135-137`), matching its no-`default-features` style:
```toml
[target.'cfg(target_os = "windows")'.dependencies.keyring]
version = "3"
features = ["windows-native"]
```
Refresh `Cargo.lock`. No source change. Verify the four `Entry::new` call sites still compile on Linux (unchanged). (iprev #3: the Linux/macOS blocks do NOT set `default-features = false`; do not add it here.)

### Step 2 — commands.rs executable-bit probe (mechanical, isolated)
Replace the `if cfg!(unix) { … }` (`:220-224`) with:
```rust
let is_executable = {
    #[cfg(unix)]
    { use std::os::unix::fs::PermissionsExt; meta.permissions().mode() & 0o111 != 0 }
    #[cfg(not(unix))]
    { is_file }
};
```
The `#[cfg]` attributes select the body at compile time so the Unix `use` is absent on Windows.

### Step 3 — clickup/auth.rs + linear/auth.rs fallback-file mode (mechanical)
In each `write_fallback_file`, move the `use std::os::unix::fs::OpenOptionsExt;` + `.mode(0o600)` into a `#[cfg(unix)]` block; on `#[cfg(not(unix))]` build the `OpenOptions` without `.mode()`. Keep the existing "less secure than keyring" warning on both paths.

### Step 4 — worktree_sessions.rs free_disk_bytes (gate + stub)
Gate the current body `#[cfg(unix)]`; add `#[cfg(not(unix))]` returning the same type with a `0`/`None` sentinel. Add a `// WHY` note pointing at `windows-proc` for the functional Windows impl (`GetDiskFreeSpaceExW`/`sysinfo::Disks`).

### Step 5 — MCP transport + shim cluster (the big one; lands together)

Key simplification (iprev #5): gate the transport types `#[cfg(unix)]` with **no Windows stub surface** — once the daemon spawn block and `shim::run` are gated, nothing ungated references these symbols on Windows.

`mcp/transport.rs`:
- Gate `use tokio::net::{UnixListener, UnixStream};` (`:18`) `#[cfg(unix)]`.
- Gate the `UnixSocketTransport` struct + `impl` (`bind`/`accept`/`connect`), the free `connect` fn, and **both** `peer_uid` fns (the `#[cfg(unix)]` real one at `:115` AND the broken `#[cfg(not(unix))]` stub at `:122`) — delete/gate the broken stub entirely; no Windows caller needs `peer_uid`. This fixes the latent break without introducing a `UnixStream`-naming Windows stub.

`mcp/mod.rs`: gate `serve`/`handle_connection`/`write_response` `#[cfg(unix)]`. **No `#[cfg(windows)]` stub** — `serve`'s only caller is the daemon spawn block in `lib.rs`, gated below.

`mcp/shim.rs`:
- Gate `relay` (`:115`) `#[cfg(unix)]`.
- Gate `run` (`:20`) + `run_async` (`:25`, which binds `daemon: Option<UnixStream>` via `transport::connect` at `:44`) `#[cfg(unix)]`.
- Add a `#[cfg(windows)] pub fn run() -> anyhow::Result<()>` stub that prints a one-line "MCP shim unsupported on Windows until windows-ipc" to stderr and returns `Ok(())` — so the ungated caller `main.rs:135` resolves and `nergal mcp` exits cleanly on Windows. (iprev #1: this entry was missed in the first enumeration.)

`lib.rs`: gate the MCP daemon block `#[cfg(unix)]` **including the locals it consumes** — extend the gate to cover `:692-714` (the `mcp_db`/`mcp_agents`/`mcp_app`/`mcp_worktree_gate` clones at `:692-695` plus the spawn at `:696-714`), not just `:696-714`, so they are not `unused_variable` on Windows (iprev #4). This also removes the ungated `libc::getuid()` at `:700`. On Windows the daemon is simply not spawned (MCP server unsupported until `windows-ipc`).

### Step 6 — hook server bind (gate + stub)
`hooks/server.rs`: gate `use tokio::net::UnixListener;` (`:7`) and the bind path (`:222`) `#[cfg(unix)]`; the internal `#[cfg(unix)]` perms (`:229-232`) + `libc::getuid` (`:240-241`) blocks then compile-exclude on Windows naturally. Add a `#[cfg(windows)]` `start_hook_server` stub returning `Unsupported` (or gate its single caller in `lib.rs`). Functional named-pipe hook server is `windows-ipc`.

### Step 7 — platform/mod.rs sync_connect Windows stub (concrete Read+Write type)
The callers in `hooks/cli.rs` (`:15,46,235,457,509`) drive the returned value with synchronous `std::io::Read`/`Write` (`use std::io::{Read, Write}` at `cli.rs:2`), so the `#[cfg(windows)]` stub's `Ok<T>` type must implement `Read + Write` (iprev #2 — a bare `Err(Unsupported)` does not type-check without a concrete `T`). Add under `#[cfg(windows)]`:
```rust
pub struct UnsupportedSyncStream(());
impl std::io::Read for UnsupportedSyncStream {
    fn read(&mut self, _: &mut [u8]) -> io::Result<usize> { Err(unsupported()) }
}
impl std::io::Write for UnsupportedSyncStream {
    fn write(&mut self, _: &[u8]) -> io::Result<usize> { Err(unsupported()) }
    fn flush(&mut self) -> io::Result<()> { Err(unsupported()) }
}
pub fn sync_connect(_path: &Path) -> io::Result<UnsupportedSyncStream> { Err(unsupported()) }
```
(`unsupported()` = `io::Error::new(io::ErrorKind::Unsupported, "…")`.) The real named-pipe sync connect is `windows-ipc`. NOT a `TcpStream` (iprev #2 alt rejected: it would invite a transport that bypasses the SID auth boundary).

### Step 8 — CI windows-check job
In `.github/workflows/ci.yml`, add a `windows-check` job mirroring `macos-cross-check`: `runs-on: windows-latest`, `rustup target add x86_64-pc-windows-msvc` (no-op if default), `cargo check --target x86_64-pc-windows-msvc`, path-filtered to `src-tauri/**`. No GStreamer/webkit system-dep install (Windows uses WebView2, bundled).

### Step 9 — CLAUDE.md cross-platform invariant
Update the "Cross-platform invariant" section: the Windows gate now exists; macOS catches Linux-only regressions, Windows catches ungated-unix seams; the two are complementary. Remove the "deferred Windows gate" hedge.

## Per-step risk + mitigation

- **Step 5 (transport + shim)** is the largest cluster. Risk lowered by the no-Windows-stub-surface strategy (iprev #5): gate the transport types `#[cfg(unix)]` with NO `#[cfg(windows)]` counterpart (no ungated caller remains once the daemon block + `shim::run` are gated), so nothing can accidentally name `UnixStream` in a stub. The only Windows surface in this cluster is the `shim::run` stub, which returns `Ok(())` and names no unix type. Verify by reading the Windows-gate compile errors (most likely cluster to need a second CI pass).
- **Steps 1, 3** touch secret-storage-adjacent code: keep Linux/macOS byte-identical; the verification group runs a Linux keyring round-trip.
- **Step 8**: first Windows gate run may surface `arboard`/`wezterm`/`tao` issues not predictable locally (no local check). That is the gate doing its job — any fix is an additive Cargo gate, folded into this change before merge.

## Verification (maps to tasks.md ## N. Verification)

- Linux full check stays green: `cargo clippy -- -D warnings && cargo test && cargo fmt --check` (+ `npx tsc --noEmit` unaffected).
- Linux keyring token round-trip (regression guard for the Cargo target-table edit).
- macOS gate stays green (`cargo check --target aarch64-apple-darwin`).
- **Windows gate green on `windows-latest`** (`cargo check --target x86_64-pc-windows-msvc`) — the authoritative pass/fail for this change; cannot be run on the Linux dev host (ring/MSVC).
