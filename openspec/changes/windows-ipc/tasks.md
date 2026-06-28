# Tasks — windows-ipc

API surfaces + signatures: `handoff/windows-namedpipe-research.md`. Depends on `windows-compile` (the `#[cfg(windows)]` stubs + the Unix gating this change replaces). Order per `implementation.md`.

## 1. Cargo: windows Win32 features

- [x] 1.1 Add `[target.'cfg(windows)'.dependencies.windows]` with `features = ["Win32_Foundation", "Win32_Security", "Win32_Security_Authorization", "Win32_System_Memory", "Win32_System_Pipes", "Win32_System_Threading", "Win32_System_IO", "Win32_Storage_FileSystem"]` (version per `cargo tree -p windows`). `Win32_System_IO` = `OVERLAPPED`/`CancelIoEx`; `Win32_Storage_FileSystem` = `FILE_FLAG_OVERLAPPED` (Step 7). Do NOT add `getrandom` (loopback fallback not implemented).

## 2. PeerIdentity opaque type (touches Unix hot path)

- [x] 2.1 `platform/mod.rs` — define `pub enum PeerIdentity { #[cfg(unix)] Uid(u32), #[cfg(windows)] Sid(Box<[u8]>) }` with `matches_current_process(&self) -> bool` and `display(&self) -> String`.
- [x] 2.2 Change `PlatformListener::accept` (unix + windows) to return `(PlatformStream, PeerIdentity)`.
- [x] 2.3 Update accept-loop callers (`hooks/server.rs`, `mcp/mod.rs` daemon) to gate on `matches_current_process()` and log the rejected principal via `display()`.
- [x] 2.4 Update the comparison-branch unit test (`platform/mod.rs:840`) to exercise `PeerIdentity` (mock a foreign uid on Unix / foreign SID on Windows), asserting reject-foreign / accept-self.

## 3. Windows security helpers (`#[cfg(windows)]`)

- [x] 3.1 `current_user_sid_string()` — `OpenProcessToken` → `GetTokenInformation(TokenUser)` → `ConvertSidToStringSidW` (`LocalFree` the string).
- [x] 3.2 `owner_only_security_attributes(sid)` — SDDL `D:P(A;;GA;;;{sid})` → `ConvertStringSecurityDescriptorToSecurityDescriptorW`; RAII wrapper `LocalFree`s the SD on drop.
- [x] 3.3 `client_sid_of(server_handle)` — `ImpersonateNamedPipeClient` → **RAII `RevertToSelf` guard constructed BEFORE any `?`** → `OpenThreadToken(openasself=true)` → `GetTokenInformation(TokenUser)` → copy SID bytes out. (Decision 5 CRITICAL invariant.)
- [x] 3.4 `process_owner_sid()` + `sid_eq(a,b)` (`EqualSid().is_ok()`; verify Err=not-equal on the Windows machine, else use `windows-sys` raw `BOOL`).
- [x] 3.5 `verify_pipe_owner_is_current_user(client_handle)` — `GetSecurityInfo(SE_KERNEL_OBJECT, OWNER_SECURITY_INFORMATION)` (check `== ERROR_SUCCESS`, it returns `WIN32_ERROR`) → `sid_eq` vs current user.

## 4. Windows endpoint paths

- [x] 4.1 Gate `hook_socket_path`/`mcp_socket_path`/`plan_review_fifo_path` `#[cfg(windows)]` to return `\\.\pipe\nergal-{sid}-{endpoint}`.
- [x] 4.2 Add `gui_pid_dir() -> io::Result<PathBuf>` = `ipc_dir()` on Unix, `dirs::data_local_dir()/nergal` on Windows. **Re-point BOTH call sites to it** (iprev round-3 Q3): writer `lib.rs:651` (today `write_gui_pid(&ipc_dir()?)`, `ipc_dir()` is cfg-unix → won't compile on Windows) and reader `cli.rs:246` (today `read_gui_pid(fifo_path.parent())`, wrong on Windows). `check_gui_liveness`/`process_start_time` are already cross-platform.

## 5. Named-pipe transport body (`#[cfg(windows)]`, replaces stubs at platform/mod.rs:730-807)

- [x] 5.1 `PlatformListener::bind(name)` — build `owner_only_security_attributes` once; **first** instance `ServerOptions::new().first_pipe_instance(true).create_with_security_attributes_raw(name, sa)`; map `PermissionDenied` → squat-detected log + refuse; `LocalFree` SD after create; `max_instances` = tokio default (unlimited).
- [x] 5.2 `PlatformListener::accept()` — multi-instance loop: `server.connect().await`; **create the NEXT instance BEFORE handoff** (Decision 9) with `first_pipe_instance(FALSE)` **+ the SAME owner-only SA** (Decision 4 CRITICAL — never a plain `create()` / default SD on instances 2..N); `client_sid_of` **fully synchronous, no `.await` between impersonate and revert** (Decision 5) → `PeerIdentity::Sid`; reject (close + rate-limited log) if `!matches_current_process()`; else return `(PlatformStream(server), peer)`.
- [x] 5.3 `PlatformStream` — wrap `NamedPipeServer`/`NamedPipeClient`; `AsyncRead`+`AsyncWrite` delegate; `connect(name)` = `ClientOptions::new().open(name)` with `ERROR_PIPE_BUSY` retry (bounded `WaitNamedPipe`-style backoff, Decision 9) + `verify_pipe_owner_is_current_user` before returning.

## 6. Migrate hook server + MCP daemon onto the seam

- [x] 6.1 `hooks/server.rs` — bind + accept via `PlatformListener`; peer check via `matches_current_process()`; un-gate (seam handles both platforms).
- [x] 6.2 `mcp/transport.rs` + `mcp/mod.rs` — replace `UnixSocketTransport` with `PlatformListener`/`PlatformStream`; `read_frame`/`write_frame` unchanged; daemon accept loop uses `PeerIdentity`. **Re-type `handle_connection` (`:619`) + `write_response` (`:670`) from `tokio::net::UnixStream` to `PlatformStream`** (or generic `S: AsyncRead+AsyncWrite+Unpin`) — no raw unix type at the call site (iprev #5). Remove `DaemonContext.app_uid: u32` (`:82`), `uid != ctx.app_uid` (`:594`), `test_ctx { app_uid: 0 }` (`:693`) → `matches_current_process()`.
- [x] 6.3 `lib.rs:692-714` — un-gate the MCP daemon spawn (runs via the seam on Windows); the raw `libc::getuid()` app_uid (`:700`) is gone (gate is `PeerIdentity`-based); confirm no other ungated `libc::` re-enters the block (preserve the #1 invariant).

## 7. Plan-review/ask-user gate (CLI-as-server; GUI connects-at-submit; Unix liveness rule — Decision 6)

- [x] 7.1 `platform/mod.rs` — NEW `#[cfg(windows)]` sync-server primitive (research §7): `sync_listen(name)` = `CreateNamedPipeW` (`PIPE_ACCESS_DUPLEX | FILE_FLAG_OVERLAPPED | FILE_FLAG_FIRST_PIPE_INSTANCE`, `PIPE_TYPE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS`, owner-only `SECURITY_ATTRIBUTES`). `accept_with_timeout(tick) -> Option<(SyncPlatformStream, PeerIdentity)>`: **manual-reset** event in `OVERLAPPED`, `ConnectNamedPipe` armed ONCE before the loop, handle `ERROR_PIPE_CONNECTED` as success (`SetEvent`) + `ERROR_IO_PENDING` as pending; `WaitForSingleObject(event, tick)` per tick — `WAIT_TIMEOUT` → `None` (do NOT re-arm `ConnectNamedPipe`); `CancelIoEx` + drain on give-up. `SyncPlatformStream: std::io::Read + Write`. Reuse Step 3 helpers (blocking Win32, no tokio).
- [x] 7.2 `hooks/cli.rs` — remove the broken `#[cfg(not(unix))]` `FifoGuard` placeholder (`:197-207`). On Windows the CLI hosts the gate via `sync_listen`, loops `accept_with_timeout(≈1s)`: `None` → run the **same `gui.pid` liveness check as Unix** (dead → safe deny) + `PLAN_REVIEW_WALL_CLOCK_SECS` backstop; `Some((stream, peer))` → verify `matches_current_process()` then read the decision. **`CreateNamedPipeW` BEFORE the notification send** (mirror Unix `mkfifo`@196-before-send@235). Unix FIFO path untouched.
- [x] 7.3 `commands.rs` `submit_plan_decision` (`:424`) — on Windows the GUI connects to the CLI gate pipe **at submit time** (single invocation, no held state) via the sync client (`verify_pipe_owner_is_current_user` + `ERROR_PIPE_BUSY`/`ERROR_FILE_NOT_FOUND` retry) and writes the decision JSON, replacing `std::fs::write` against a pipe path. Unix `std::fs::write` to the FIFO unchanged; GUI keeps writing `gui.pid`.
- [x] 7.4 `platform/mod.rs` — `#[cfg(windows)] sync_connect` named-pipe **client** body (replaces `UnsupportedSyncStream` from #1), `Read + Write` handle, `ERROR_PIPE_BUSY`/`ERROR_FILE_NOT_FOUND` retry (Decision 9) + `verify_pipe_owner_is_current_user`. Used by the GUI submit-write + the CLI's fire-and-forget event sends.

## 8. IPC observability

- [x] 8.1 Route every Windows rejection (squat bind-fail, foreign-SID accept, owner-mismatch client refusal) through `RejectionRateLimit` (`platform/mod.rs:509`), keyed by SID string, so the Windows boundary is logged + DoS-resistant like the Unix one.

## 9. Verification

- [x] 9.1 **Windows gate green** — `cargo check --target x86_64-pc-windows-msvc` ✅ (CI run `28332228238`, 2 rounds: round 1 caught 3 relocated 0.62 module paths, round 2 green).
- [ ] 9.2 **`cargo test` on `windows-latest`** — the `PeerIdentity` comparison-branch test (Windows arm, `peer_identity_rejects_foreign_sid`) is WRITTEN + compiles, but the CI `windows-check` job runs `cargo check` only (check-parity with macOS, no `cargo test` runner). **Walk-pending**: the user runs `cargo test` on the Windows machine. No named-pipe round-trip test was added (would need a 2-account harness anyway).
- [x] 9.3 **Linux full check** — `cargo clippy -- -D warnings` (no issues) + `cargo test` (699 passed, incl. the Unix-arm `peer_identity_rejects_foreign_uid`) + `cargo fmt --check` (clean). No Linux regression.
- [x] 9.4 **macOS gate green** — `cargo check --target aarch64-apple-darwin` ✅ (same CI run).
- [ ] 9.5 **User Windows-machine walk (UNVERIFIED-pending until run)** — hook events flow, MCP `tools/call` serves, plan-review blocks + resolves, a foreign-user connect is rejected + logged; `RevertToSelf` confirmed on error paths (no lingering impersonation).
