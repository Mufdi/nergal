# Design — windows-ipc

## Context

The `platform-ipc-transport` spec was written during the macOS port with the full cross-platform contract, leaving the Windows body deferred. `windows-compile` (change #1) gated the Unix transport `#[cfg(unix)]` and left `#[cfg(windows)]` `Unsupported` stubs. This change supplies the real Windows transport — a `tokio::net::windows::named_pipe` implementation with an OS-enforced owner-only ACL + client-SID peer auth — and migrates the hook server, MCP daemon, and the plan-review/ask-user blocking round-trips onto the seam so they work on Windows.

All Win32/tokio API surfaces cited here are verified in `handoff/windows-namedpipe-research.md` (against docs.rs tokio 1.x + microsoft.github.io windows-rs 0.62) and marked CONFIRMED / UNCONFIRMED there. This is the security-critical change of the Windows port; the user has a Windows machine, so the boundary is runtime-walkable, not compile-only.

## Decision 1 — Named pipe, not TCP-loopback+token

**Chosen**: `tokio::net::windows::named_pipe`. This is the spec's recorded recommendation (archived `platform-ipc` Decision 3) and it maps the Unix model directly: an OS-enforced owner-only security descriptor (the ACL analog of `0600` + the `0700` dir) plus a real client-SID peer identity (the analog of peer-cred uid). It needs no new transport dependency (tokio ships the types) and reuses the existing path-string abstraction (a pipe name is a string like a socket path).

- *Alternative: TCP loopback + per-session token.* **Rejected** (spec-constrained): trades an OS-enforced ACL for an in-band secret that must be minted (≥128-bit CSPRNG), delivered over a non-inspectable channel, constant-time compared, and validated-before-dispatch (the "Constrained Windows loopback fallback" requirement). Named-pipe has no edge case forcing this. The fallback stays spec-only; this change does **not** implement it. If a future sandboxed agent CLI cannot open pipes, a follow-up adopts loopback under those constraints.

## Decision 2 — `accept` returns an opaque `PeerIdentity`, not `u32`

The seam's `PlatformListener::accept` currently returns `(PlatformStream, u32)` (a Unix uid); the hook/MCP accept loops compare `uid != app_uid`. Windows has no uid — peer identity is a variable-length SID. **Chosen**: replace `u32` with an opaque

```rust
pub enum PeerIdentity {
    #[cfg(unix)] Uid(u32),
    #[cfg(windows)] Sid(/* owned SID bytes */),
}
impl PeerIdentity { pub fn matches_current_process(&self) -> bool { /* uid==getuid() | EqualSid(owner) */ } }
```

The accept loops gate on `matches_current_process()`. Unix behaviour is byte-identical (`uid == getuid()`). This is the resolution of the archived design's open question ("peer-identity type in the trait signature — uid vs pipe client SID").

- *Alternative: keep `u32`, synthesize a fake uid on Windows from the SID hash.* **Rejected** — lossy and invites a collision/forgery; the verdict is "same principal?", so model that, not a fake integer.
- *Alternative: have `accept` do the check internally and never expose identity.* Tempting, but the existing Unix call sites log the rejected uid (IPC-observability requirement), so identity must surface for the audit line. The opaque type carries enough to log (uid / SID string) without leaking representation.

## Decision 3 — Per-user pipe naming + anti-squat (two-sided)

The `\\.\pipe\` namespace is **global per machine**, so two hazards exist that the Unix per-user `0700` dir handled structurally: cross-user name collision and name squatting. **Chosen**, both sides:

- **Name** = `\\.\pipe\nergal-<user-SID-string>-<endpoint>` (`ConvertSidToStringSidW`; the SID is unique per user and contains only `S-`/digits, safe in a pipe name). A different user's Nergal binds a different name — no collision.
- **Server bind** uses `first_pipe_instance(true)` (`FILE_FLAG_FIRST_PIPE_INSTANCE`): if a squatter pre-created the pipe, `create` fails `PermissionDenied` (CONFIRMED, cross-process) → the server refuses to start the surface and logs a squat alert, never rebinds a name an attacker owns.
- **Client open** verifies the connected pipe's owner SID (`GetSecurityInfo(SE_KERNEL_OBJECT, OWNER_SECURITY_INFORMATION)`) `EqualSid` the current user before sending any payload — anti-impersonation, closing the different-user-same-name lure on a multi-session host.

The per-user SID thus replaces the per-user `0700` directory as the isolation root on Windows; `ipc_dir()` (filesystem) has no Windows analog — the endpoint-path helpers return pipe names directly.

## Decision 4 — Owner-only security descriptor via SDDL

**Chosen**: build the pipe's `SECURITY_ATTRIBUTES` from the SDDL string `D:P(A;;GA;;;<user-SID>)` (protected DACL — no inheritance — granting GENERIC_ALL to the current user only) with `ConvertStringSecurityDescriptorToSecurityDescriptorW`, pass it to `create_with_security_attributes_raw`, then `LocalFree` the descriptor immediately after `create` returns (its lifetime need only span the call — research gotcha 2). This is the OS-enforced ACL that makes the pipe un-openable by any other user from creation (no create-then-restrict window — a strict improvement over the Unix bind-then-chmod gap, which the `0700` dir already covers).

- *Alternative: manual DACL (`InitializeSecurityDescriptor` + `InitializeAcl` + `AddAccessAllowedAce` + `SetSecurityDescriptorDacl`).* All present in the windows crate, but more verbose and error-prone; SDDL is the recommended, audited-string path. Chosen for clarity and reviewability.

**CRITICAL multi-instance rule (iprev #1).** A named-pipe server is a LOOP: each accepted connection consumes one server instance, so the listener creates a fresh instance for the next accept. The security trap: the SD and the `first_pipe_instance` flag must be handled differently for the first vs. subsequent instances.
- The **first** instance: `first_pipe_instance(true)` (anti-squat) **+** the owner-only SA.
- **Every subsequent** instance: `first_pipe_instance(false)` (the flag would make `create` fail `ERROR_ACCESS_DENIED` on instances 2..N) **but STILL the same owner-only SA**. A plain `ServerOptions::create()` (no SA) on subsequent instances would give them the **default** security descriptor — re-opening the foreign-open gap this change exists to close, on every connection after the first. The implementation SHALL build the SA once and apply it to every instance via `create_with_security_attributes_raw`, toggling only `first_pipe_instance`. `max_instances` SHALL be left at the tokio default (unlimited) so the loop is not capped at one.

## Decision 5 — Client-SID peer auth with mandatory `RevertToSelf` (defense in depth)

Even with the owner-only ACL (which already blocks foreign opens), the accept loop SHALL re-check, mirroring the Unix `0700`-dir + peer-cred pairing. **Chosen** sequence on the server pipe `HANDLE` (`NamedPipeServer::as_raw_handle()`):

```
ImpersonateNamedPipeClient(handle)  →  RAII guard ⟹ RevertToSelf() on EVERY exit
OpenThreadToken(GetCurrentThread(), TOKEN_QUERY, openasself=true, &token)
GetTokenInformation(token, TokenUser, …)  // two-call size pattern
EqualSid(client_sid, process_owner_sid)   // reject unless equal
```

**Security invariant (research §3, gotcha 6)**: `RevertToSelf()` MUST run on every path including errors, or the server thread stays impersonating the client — a privilege hole. Enforced with a `Drop`-guard struct (not a bare call), constructed immediately after `ImpersonateNamedPipeClient` succeeds and before any fallible call, so an early `?` cannot skip it. `openasself=true` opens the token in the server's own context (avoids failing when the client is lower-privilege).

**Second invariant (iprev #3): `client_sid_of` SHALL be fully synchronous — NO `.await` between impersonate and revert.** `ImpersonateNamedPipeClient` impersonates the current OS *thread*; under tokio's multi-threaded runtime an `.await` inside the impersonation window could let work-stealing migrate the task to another worker, leaving a thread impersonating the client (the same privilege hole, by a different mechanism). The whole impersonate→token→revert sequence is synchronous Win32 (research §3) and SHALL run with no await point inside it; `accept()` calls it after `connect().await` returns, never across an await.

**Open question carried to implementation (research §4 UNCONFIRMED)**: the windows-crate `EqualSid` wraps `BOOL` as `Result<()>` where `Err` = "not equal" (synthetic, not an OS error). Compare with `.is_ok()`, and the implementer SHALL verify on the Windows machine that `Err` carries no spurious code — or use the `windows-sys` raw `BOOL` binding (`!= 0`) to remove ambiguity. Same caution for `OWNER_SECURITY_INFORMATION`'s exact module path.

## Decision 6 — Plan-review/ask-user gate: CLI-as-server, GUI connects-and-holds (iprev #2)

This is the security-critical approval gate (a forged "allow" must be impossible) and the review found the original plan under-specified it. Ground truth of the Unix model: the hook **CLI owns the endpoint** — it `mkfifo`s `plan-{pid}` (`hooks/cli.rs:130`) and *reads*; the GUI *writes* the decision via `std::fs::write(decision_path)` (`commands.rs:424`). The hook socket is fire-and-forget with **no response path**, so the CLI cannot be handed a GUI-created endpoint name. Mirroring on Windows therefore requires the **CLI to host the endpoint**, and the CLI is a blocking binary with **no tokio runtime** (`platform/mod.rs:809` rationale). So:

**Chosen direction — CLI-as-synchronous-server; GUI connects-and-writes at submit; reuse the Unix liveness-aware blocking rule (NOT connect-and-hold).** The round-1 "connect-and-hold + connected-EOF" idea was reconsidered: it requires the **Rust GUI side** (the `server.rs` PlanReview handler, not the React frontend — JS cannot hold an OS pipe) to connect at `plan:ready` time and hold a `PlatformStream` in managed state across three separate events (`plan:ready` → deliberation → `submit_plan_decision`), which `submit_plan_decision` (a single submit-time Tauri command, `commands.rs:424`) cannot do; and it STILL needs a connect-never backstop. So the connected-EOF signal bought complexity, not robustness. **Chosen instead: mirror the Unix model exactly — one blocking rule, two transports.**

- The CLI hosts the gate via a NEW `#[cfg(windows)]` sync primitive — `sync_listen(name)` + a blocking `accept` (`CreateNamedPipeW` with owner-only SD + `first_pipe_instance(true)`, then **overlapped** `ConnectNamedPipe` + `WaitForSingleObject` with a bounded ≈1 s tick — the named-pipe analog of the Unix FIFO `O_NONBLOCK`+`poll(timeout)`). On Unix the CLI keeps the existing `mkfifo`+poll path (this sync server is Windows-only).
- On each timeout tick with no client yet, the CLI checks **GUI liveness exactly as on Unix** — the `gui.pid` pid+start-time token (so `gui.pid` IS written on Windows; Step 4 keeps it). GUI dead (pid gone / start-time mismatch) → safe deny; live GUI not-yet-submitted → keep waiting. The human-scale wall-clock backstop (`PLAN_REVIEW_WALL_CLOCK_SECS`) applies as on Unix, covering the connect-never window (GUI crashed before submitting, or no GUI received the notification).
- The **GUI connects and writes the decision at submit time** (mirrors the Unix `std::fs::write` timing — a single `submit_plan_decision` invocation, no state held across events). On Windows `submit_plan_decision` connects to the CLI gate pipe via `PlatformStream`/sync connect (with `verify_pipe_owner_is_current_user` + `ERROR_PIPE_BUSY`/`ERROR_FILE_NOT_FOUND` retry) and writes the decision JSON, replacing `std::fs::write` against a pipe path. Unix `std::fs::write` to the FIFO is unchanged.

The owner-only boundary is identical to Unix: the CLI server's SD admits only the same user, and the accept-side client-SID check re-verifies — a forged "allow" is impossible (only a same-principal GUI can connect at all). Squat/pre-seed rejection is `first_pipe_instance(true)`. The broken `#[cfg(not(unix))]` `FifoGuard` placeholder (`cli.rs:197-207`) is removed, not patched.

**Ordering invariant (iprev round-2 NEW-3):** the CLI's `CreateNamedPipeW` SHALL complete BEFORE the PlanReview socket notification is sent (mirroring the Unix `mkfifo` at `cli.rs:196` preceding the socket send at `cli.rs:235`), so the GUI never races a not-yet-created pipe; the GUI connect additionally retries on `ERROR_FILE_NOT_FOUND` as belt-and-suspenders.

- *Alternative: connect-and-hold + connected-EOF death signal.* **Rejected (round-2)** — needs unspecified Rust managed-state holding the stream across `plan:ready`→submit, cannot live in `submit_plan_decision`, and still needs a connect-never backstop. The Unix liveness rule is proven, already implemented, and ports with one transport swap.

## Decision 9 — `ERROR_PIPE_BUSY` retry on the hot hook path (iprev #4)

Hook events are high-frequency, one short connection each (`hooks/cli.rs:46`). In a single-instance-at-a-time accept loop, a client `open()` between instance N's `connect()` returning and instance N+1 being created gets `ERROR_PIPE_BUSY` (research §1) → a silently dropped event. **Chosen** two-part mitigation: (a) the listener creates the **next** server instance *before* handing the accepted one to the handler (shrink the no-instance window), and (b) the client side (`PlatformStream::connect` + the sync connect) retries on `ERROR_PIPE_BUSY` with a short bounded `WaitNamedPipe`-style backoff. Without this, events drop under load — a correctness regression vs. the always-available Unix socket.

## Decision 10 — Cross-session messaging rides the MCP socket (no separate transport)

The spec's abstraction requirement names cross-session messaging as a binder. Confirmed: it has **no separate listener** — it is dispatched over the MCP daemon socket (DB-as-queue, `mcp/mod.rs`). Migrating the MCP daemon onto `PlatformListener` (Decision 7) therefore covers cross-session messaging automatically; no extra transport work. Recorded so a reviewer does not suspect a missed binder.

## Decision 7 — MCP `UnixSocketTransport` → `PlatformListener`

`windows-compile` gated `UnixSocketTransport` `#[cfg(unix)]`. **Chosen**: replace it with `PlatformListener` + `PlatformStream` carrying the **unchanged** `read_frame`/`write_frame` length-framing helpers (they are generic over `AsyncRead`/`AsyncWrite`, which both the Unix stream and `NamedPipeServer`/`Client` implement). The MCP daemon accept loop uses `PeerIdentity`. On Unix this is a no-op refactor (the seam's Unix body is `UnixListener`), satisfying the spec's "MCP daemon binds through the abstraction" + "No Linux regression". The hook server (`hooks/server.rs`, raw `UnixListener` gated by #1) is migrated the same way.

**Signature re-typing (iprev #5):** the dispatch layer still names raw Unix types — `handle_connection(mut stream: tokio::net::UnixStream, …)` (`mcp/mod.rs:619`) and `write_response(&mut tokio::net::UnixStream)` (`:670`). The "Platform transport abstraction" requirement forbids a raw `unix::net`/`named_pipe` type at the call site, so these two signatures SHALL be re-typed to `PlatformStream` (or a generic `S: AsyncRead + AsyncWrite + Unpin`). Mechanical, but a named edit — a literal "framing unchanged" reading would leave a `UnixStream` on the Windows path. Relatedly, `DaemonContext.app_uid: u32` (`mcp/mod.rs:82`), its use `uid != ctx.app_uid` (`:594`), and the `test_ctx { app_uid: 0 }` (`:693`) are replaced by the `PeerIdentity::matches_current_process()` comparison (the raw uid is no longer threaded through the daemon context).

## Decision 8 — `windows` crate dependency, Win32 features only

**Chosen**: `[target.'cfg(windows)'.dependencies.windows]` with exactly the Win32 features the transport needs: `Win32_Foundation`, `Win32_Security`, `Win32_Security_Authorization`, `Win32_System_Memory`, `Win32_System_Pipes`, `Win32_System_Threading`, **`Win32_System_IO`** (for `OVERLAPPED`, `CancelIoEx`, `GetOverlappedResult` — the sync-server overlapped accept; iprev round-3) and **`Win32_Storage_FileSystem`** (for the `FILE_FLAG_OVERLAPPED` constant). Tauri already pulls the `windows` crate transitively on Windows, so this adds features, not a new vendor. `getrandom` is added only if `cargo tree` shows it is not already transitive (it almost certainly is, via tokio/ring) — and only matters for the un-adopted loopback fallback, so it can be deferred entirely.

## Risks / Trade-offs

- **[Risk] Multi-user SID-rejection path is not exercisable in single-user CI.** `cargo test` on `windows-latest` can unit-test the `PeerIdentity::matches_current_process()` comparison (mock a foreign SID, like the Unix mocked comparison-branch test) but cannot exercise a real foreign-user connect without a second account — exactly the Unix `sudo -u nobody` UNVERIFIED-pending situation. Mitigation: commit the mocked comparison-branch test (CI), and document the real two-user harness as UNVERIFIED-pending; the user can run it on the Windows machine.
- **[Risk — CRITICAL] `RevertToSelf` skipped on an error path = privilege hole.** Mitigation: RAII `Drop`-guard, never a bare call; iprev must confirm no `?` between `ImpersonateNamedPipeClient` and the guard's construction.
- **[Risk] `EqualSid` / `GetSecurityInfo` return-type gotchas** (Result-vs-WIN32_ERROR, Err=not-equal). Mitigation: research §4/§5 records the exact success checks; implementer verifies on the Windows machine; fallback to `windows-sys` raw bindings if ambiguous.
- **[Risk] SD / SID pointer lifetimes** (owner SID points into a `LocalFree`-able buffer). Mitigation: research gotchas 2,3,5 — free after use, never hold a SID pointer past its buffer.
- **[Risk] Refactor touches the Unix hot path** (PeerIdentity, MCP transport replacement). Mitigation: Unix bodies are mechanically equivalent; the full Linux machine-check suite + the peer-uid comparison-branch CI test are the regression guard (spec "No Linux regression").

## Migration / rollback

Additive `#[cfg(windows)]` bodies + one Unix-visible type change (`u32` → `PeerIdentity`, mechanically applied at the two accept loops) + one Cargo target block. Git-revertible. A revert restores the `#[cfg(windows)]` `Unsupported` stubs from `windows-compile` and the `u32` accept signature — Windows returns to compile-but-inert, Unix unaffected.
