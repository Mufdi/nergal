# platform-ipc-transport Specification

## Purpose
TBD - created by archiving change platform-ipc. Update Purpose after archive.
## Requirements
### Requirement: Platform transport abstraction

The system SHALL define a `PlatformListener` / `PlatformStream` transport abstraction through which the hook server, the MCP daemon, cross-session messaging, and the blocking request/response round-trips bind, accept, and connect. The hook server and MCP daemon SHALL obtain their listener and streams from this abstraction rather than referencing `std::os::unix::net` / `tokio::net::Unix*` types (Unix) or `tokio::net::windows::named_pipe` types (Windows) directly at the call site. The Unix implementation SHALL wrap the existing `UnixListener` / `UnixStream` code with no behavioural change; the Windows implementation SHALL wrap `tokio::net::windows::named_pipe`. The existing length-framing helpers (`read_frame` / `write_frame`, generic over async reader/writer) SHALL remain unchanged and continue to operate over the abstracted stream on both families. `accept` SHALL return `(PlatformStream, PeerIdentity)`.

#### Scenario: Hook server binds through the abstraction

- **WHEN** the hook server starts and binds its socket on Unix or Windows
- **THEN** it SHALL obtain the listener via the `PlatformListener` abstraction and accept connections as `(PlatformStream, PeerIdentity)`, with no raw `unix::net` or `named_pipe` type at the call site

#### Scenario: MCP daemon binds through the abstraction

- **WHEN** the MCP daemon starts and binds its socket on Unix or Windows
- **THEN** it SHALL obtain the listener via the `PlatformListener` abstraction (replacing the former `UnixSocketTransport`), and the length-framed read/write helpers SHALL operate unchanged over the resulting `PlatformStream`

#### Scenario: Connect side uses the abstraction

- **WHEN** the hook CLI or the MCP shim connects to a daemon socket on Unix or Windows
- **THEN** it SHALL connect via the `PlatformStream` abstraction rather than calling `UnixStream::connect` / `ClientOptions::open` directly at the call site

### Requirement: Per-user IPC runtime directory rooted at an un-squattable base

All IPC endpoints — the hook event socket, the MCP daemon socket, and the blocking plan-review / ask-user FIFOs — SHALL be created inside a single per-user IPC runtime directory whose ROOT is a base into which no other uid can write, so the directory cannot be pre-created (squatted) by a hostile different-uid process. A `0700` leaf under a shared sticky `/tmp` does NOT satisfy this: the sticky bit prevents deletion but not a hostile `mkdir` of the predictable `nergal-<uid>` name, which would then force a permanent refuse-to-bind. Specifically:

- On Linux the root SHALL be `/run/user/<uid>/nergal/`, where `<uid>` is computed from `getuid()` — NOT read from the `$XDG_RUNTIME_DIR` environment variable, and with NO `$XDG_RUNTIME_DIR` override, because the connecting processes do not share the binding process's environment (the Codex MCP shim runs with a sanitized env, so any env-keyed root — even as an override with weaker-than-string-equal validation — would diverge between bind and connect for non-standard `$XDG_RUNTIME_DIR` and silently break MCP). If `/run/user/<uid>` does not exist or fails validation, the system SHALL fall back to a `0700` directory under the user's home derived from `getpwuid(getuid())` (NOT the `$HOME` env var, which `sudo`/`su` leaves pointing at the original user), validating the fallback parent is owned by `getuid()`; it SHALL NOT fall back to a guessable-named directory under shared `/tmp`.
- On macOS the root SHALL be `std::env::temp_dir()` (already a per-user `0700` `/var/folders/.../T` whose parent is user-owned) → `<temp_dir>/nergal/`.

The system SHALL create the directory `0700` and SHALL validate, before binding any endpoint inside it, that it is owned by the current uid and no broader than `0700`; on a foreign-owned or over-permissive directory it SHALL refuse to start the affected IPC surface and log the condition rather than binding into an attacker-controlled location. The directory resolution and the endpoint-path derivation SHALL be performed by a single shared resolver keyed off `getuid()` (not a process-local env var) used identically by the binding process (GUI), the connecting shim, and the separate hook-CLI binary, so the connect side can never target a different path than the bind side. If the resolved directory disappears mid-run (e.g. systemd tears down `/run/user/<uid>` on last-session logout), the server SHALL re-arm by **re-running the full resolver** (NOT `mkdir /run/user/<uid>`, which a non-root process cannot do after teardown — `/run/user` becomes root-owned; re-running the resolver falls correctly to the `getpwuid` home fallback) rather than dying silently, accepting that the instance may sit on the fallback root until restart.

This requirement closes the Linux `/tmp`-sticky-bit asymmetry (where a hostile different-uid process can pre-create or squat a predictable socket / `nergal-plan-{pid}.fifo` path, or pre-create the per-user directory itself) and harmonises the threat model with macOS, whose `temp_dir()` is already a per-user owner-only location. It is the structural prerequisite for the owner-only boundary the peer-auth and blocking-primitive requirements depend on.

#### Scenario: IPC directory root is un-squattable and uid-validated

- **WHEN** Nergal starts and prepares any IPC endpoint (hook socket, MCP socket, or a blocking FIFO)
- **THEN** the endpoint SHALL reside inside a per-user directory whose root is un-writable by other uids (`$XDG_RUNTIME_DIR` or a home/state fallback on Linux; `temp_dir()` on macOS), validated to be `0700` and owned by the current uid, so no different-uid process can open, connect to, pre-seed, OR pre-create the directory or endpoint

#### Scenario: Hostile pre-created directory cannot cause a permanent DoS

- **WHEN** a different-uid process attempts to deny the IPC surface by pre-creating the per-user directory it expects Nergal to use
- **THEN** because the directory root is un-writable by other uids, the hostile `mkdir` SHALL fail, so the attacker cannot force Nergal into a permanent refuse-to-bind state; a foreign-owned directory encountered on a misconfigured host SHALL be refused and logged (tripwire), not silently used

#### Scenario: Concurrent same-uid instances do not corrupt each other

- **WHEN** two same-uid Nergal instances start and one finds a pre-existing endpoint at the expected path
- **THEN** removal of the existing endpoint SHALL be conditional on a liveness probe — `connect()` success means an instance is live (defer to it), `ECONNREFUSED` on an existing socket file means a stale endpoint with no listener (provably dead → safe to unlink), NOT a reply (the hook socket is fire-and-forget and never replies) — guarded by an `flock`'d lockfile, so a second launch SHALL NOT unlink the socket a live instance is actively serving

#### Scenario: Resolver derives an identical path under a sanitized env

- **WHEN** the Codex MCP shim (which runs with a sanitized environment that strips `$XDG_RUNTIME_DIR`) resolves the IPC directory to connect to the daemon
- **THEN** the shim SHALL derive the same `/run/user/<uid>/nergal/` path the GUI bound, because the resolver keys off `getuid()` rather than the stripped env var, so MCP connectivity is not silently broken by the relocation

### Requirement: macOS runs the POSIX path unchanged

Because `#[cfg(unix)]` is true on macOS, the system SHALL run the existing Unix-socket and FIFO transport on macOS through the abstraction without a rewrite. The real hook event flow, the MCP directory/messaging flow, and the cross-session messaging flow SHALL function on macOS via the Unix implementation of the transport.

#### Scenario: Hook, MCP, and cross-session flows operate on macOS

- **WHEN** Nergal runs on macOS and an agent session emits hook events, the MCP daemon serves a `tools/call`, and a cross-session message is delivered
- **THEN** each SHALL succeed over the Unix implementation of the transport, with no Windows-specific code required

#### Scenario: Socket path fits the macOS AF_UNIX length bound

- **WHEN** the system derives an endpoint path from `std::env::temp_dir()` on macOS (a long per-user `/var/folders/.../T/` path)
- **THEN** the transport seam SHALL guarantee at bind time that the resulting `AF_UNIX` path stays within the platform `sun_path` limit (~104 bytes on macOS) — shortening the endpoint name deterministically (e.g. a short stable hash) when the derived path would overflow — rather than relying on a human to notice a runtime `bind` failure

### Requirement: Per-platform peer-authentication boundary

The transport SHALL enforce a per-platform peer-authentication boundary on the security-sensitive sockets — both the MCP daemon socket AND the hook event socket. On Unix the boundary SHALL be the peer-credential uid check obtained through `tokio`'s `peer_cred()` (backed by `SO_PEERCRED` on Linux and by `LOCAL_PEERCRED` / `getpeereid` on macOS): a connection from a process owned by a different uid SHALL be rejected, regardless of whether the OS additionally exposes a peer PID (macOS does not — the boundary SHALL rely on the uid, never on a peer PID). On Windows the boundary SHALL be the named-pipe owner-only security descriptor (SDDL `D:P(A;;GA;;;<user-SID>)`) AND a client-SID check performed in the accept loop: the server extracts the connected client's user SID via `ImpersonateNamedPipeClient` → token query → `RevertToSelf` and rejects the connection unless that SID `EqualSid` the server process's owner SID. Both layers SHALL be enforced (ACL restricts who can open the pipe at all; the SID check is defense in depth, mirroring the Unix `0700` dir + peer-cred pairing).

The hook event socket — which today authenticates by file mode alone and reads fire-and-forget newline-delimited events with no credential check — SHALL additionally reject foreign-principal connections in its accept loop, closing the create-then-`chmod` window. On Unix the per-user `0700` IPC directory makes this window unreachable structurally; on Windows the owner-only security descriptor makes the pipe un-openable by a foreign user from creation; the peer check on accept is defence in depth on both.

#### Scenario: Different-uid peer is rejected on Unix

- **WHEN** a process owned by a different uid connects to the MCP daemon socket or the hook event socket on Unix (Linux or macOS)
- **THEN** the transport SHALL reject the connection via the peer-credential uid check, and SHALL log the rejected uid

#### Scenario: Different-SID peer is rejected on Windows

- **WHEN** a process owned by a different user SID connects to the MCP daemon or hook named pipe on Windows
- **THEN** the transport SHALL reject the connection — the owner-only security descriptor SHALL block the open, and as defense in depth the accept-loop client-SID check SHALL reject any connection whose client SID does not `EqualSid` the process owner SID, with `RevertToSelf` invoked on every path (including errors), and the rejected SID SHALL be logged

#### Scenario: macOS peer uid is extracted correctly (committed real foreign-uid harness)

- **WHEN** a tiny connector binary, staged in a world-execable location (`/tmp`, `/usr/local/bin`), is run as the always-present unprivileged `nobody` account (`sudo -u nobody`, NOT a mock and NOT a setuid helper) and connects to a **dedicated test socket bound in a deliberately-traversable parent** (`0711`/`0755`) — NOT the production socket inside the `0700` per-user dir, which a foreign uid cannot traverse
- **THEN** the foreign peer SHALL actually reach `accept()`, `peer_cred().uid()` SHALL return nobody's real uid (via `LOCAL_PEERCRED` / `getpeereid`), and the boundary SHALL reject it post-accept; the harness SHALL distinguish an `EACCES`-at-`connect` (directory blocked the peer — `peer_cred` NOT exercised → test INVALID) and an `EACCES`-at-`exec` (connector not traversable by `nobody` → harness not runnable) from a genuine peer-cred rejection, recording an un-runnable harness or unavailable `sudo` as explicitly UNVERIFIED-pending rather than as a pass or as covered by the mocked comparison-branch test

---

### Requirement: Constrained Windows loopback fallback

If the deferred Windows transport adopts the TCP-loopback-plus-token fallback instead of the recommended named pipe, the token mechanism SHALL meet ALL of the following, so that the fallback is not materially weaker than the Unix uid wall: (a) the token SHALL be generated from a cryptographically secure RNG with at least 128 bits of entropy; (b) the token SHALL be delivered to the peer over a channel not readable by other local users (NOT a world-readable file and NOT a process environment variable that another process can inspect via process introspection); (c) the token comparison SHALL be constant-time; (d) the token SHALL be validated and the unauthenticated peer rejected BEFORE any request payload is read or dispatched. The named-pipe variant remains the recommended and default Windows transport precisely because it avoids these in-band-secret hazards by using an OS-enforced owner ACL.

#### Scenario: Loopback token meets the hardening floor (deferred)

- **WHEN** the Windows iteration implements the loopback fallback
- **THEN** the per-session token SHALL be ≥128-bit CSPRNG, delivered over a non-world-readable channel, compared in constant time, and enforced before any dispatch — otherwise the loopback fallback SHALL NOT be used and the named-pipe variant SHALL be required

### Requirement: Platform-gated socket permissions

The system SHALL gate all Unix-only permission calls behind `#[cfg(unix)]` so the crate compiles under a non-Unix target. On Unix the hook server's `set_permissions(socket, 0o600)` block SHALL be `#[cfg(unix)]`-gated (matching the MCP transport), and because the endpoint lives inside the per-user `0700` IPC directory, the socket is never group/world-connectable even before the per-socket `0600` chmod takes effect. On Windows owner-only access SHALL be provided by the named-pipe security descriptor (SDDL `D:P(A;;GA;;;<user-SID>)`) attached at pipe creation via `create_with_security_attributes_raw`, NOT by a POSIX file mode — the pipe is un-openable by a foreign user from the instant it is created, so there is no create-then-restrict window on Windows.

#### Scenario: Hook server permissions are gated on Unix

- **WHEN** the hook server binds its socket on Unix
- **THEN** it SHALL set mode `0600` inside a `#[cfg(unix)]` block, the socket SHALL reside inside the per-user `0700` IPC directory so no different-uid process can connect at any instant, and the same code SHALL compile (excluding the POSIX mode call) under a Windows target

#### Scenario: Windows pipe is owner-restricted at creation

- **WHEN** the hook or MCP named pipe is created on Windows
- **THEN** its security descriptor SHALL grant access only to the current user's SID (no create-then-chmod window), and a foreign-user open attempt SHALL be denied by the OS before any accept

---

### Requirement: Unified blocking request/response primitive

The blocking plan-review and ask-user round-trips SHALL be expressible over the same `PlatformStream` primitive as the rest of IPC, rather than depending on a separate `mkfifo` mechanism that does not exist on Windows. The observable blocking semantics SHALL be preserved: the hook CLI blocks until the GUI writes a decision/answer, then proceeds. The blocking round-trip carries an authorization decision (plan-review feeds the human approval gate on agent-spawned worktrees), so it SHALL enforce the SAME owner-only boundary as the rest of IPC: the only process permitted to write the decision/answer is one owned by the current principal — on Unix enforced structurally by the per-user `0700` IPC directory, on Windows enforced by the named-pipe owner-only security descriptor and the client-SID check.

The blocking rule SHALL be the **same liveness-aware model on both families**, differing only in transport. The GUI writes a `gui.pid` pid+start-time token; the hook CLI hosts the endpoint and waits with a bounded timeout tick, checking GUI liveness each tick; detected GUI death (pid gone / start-time mismatch) → safe deny; a live GUI with a pending human decision SHALL NOT be force-denied; a human-scale wall-clock backstop bounds the connect-never case. On Unix the endpoint is the existing `mkfifo` FIFO relocated into the per-user `0700` IPC directory, polled non-blocking (`O_NONBLOCK`, ignore `POLLHUP`, read on `POLLIN`). On Windows the endpoint is a CLI-hosted named-pipe server (`CreateNamedPipeW`, owner-only SD, `first_pipe_instance(true)`) accepted with overlapped `ConnectNamedPipe` + a bounded wait (the named-pipe analog of the FIFO `O_NONBLOCK`+`poll`); the GUI connects and writes the decision at submit time over a `PlatformStream` (owner-verified), not `std::fs::write` against a pipe path. The CLI SHALL create the pipe BEFORE the plan-review notification is sent, so the GUI never races a not-yet-created endpoint. The endpoint SHALL be treated as hostile if pre-existing (the Unix FIFO is unlinked-on-entry; the Windows server uses `first_pipe_instance(true)` so a pre-seeded pipe is rejected) so a stale or pre-seeded decision cannot be read as a live answer.

#### Scenario: Plan-review blocks and resolves on Unix

- **WHEN** an agent triggers a plan review and the user submits a decision in the GUI
- **THEN** the hook CLI SHALL block until the decision arrives and then emit the corresponding allow/deny output, using the POSIX FIFO path inside the per-user `0700` IPC directory on Linux and macOS

#### Scenario: Only a same-principal writer can resolve the gate

- **WHEN** a different-principal local process attempts to write a forged `allow` decision (or an answer) into the blocking endpoint before the GUI does
- **THEN** it SHALL be unable to open the endpoint for writing — on Unix because the endpoint lives inside the per-user `0700` IPC directory, on Windows because the named pipe's owner-only security descriptor denies the open — so the approval gate SHALL only ever be resolved by a process owned by the current principal

#### Scenario: Blocking round-trips work without mkfifo on Windows

- **WHEN** plan-review or ask-user runs on Windows
- **THEN** the round-trip SHALL be performed over a CLI-hosted named-pipe server (no dependency on `mkfifo`), preserving block-until-decision and the owner-only boundary; the GUI SHALL connect and write the decision at submit time over an owner-verified `PlatformStream`, and the CLI SHALL create the pipe before sending the plan-review notification so the GUI never races a missing endpoint

#### Scenario: Dead GUI resolves to a safe deny; live deliberation does not

- **WHEN** the GUI process dies while a decision is pending (on both Unix and Windows the CLI's per-tick liveness check sees the `gui.pid` pid gone OR a start-time-token mismatch)
- **THEN** the blocking round-trip SHALL resolve to a safe deny (plan rejected / no answer) rather than blocking the agent loop forever, and the event SHALL be logged
- **WHEN** the GUI is still alive (pid AND start-time token both match) and the human has simply not decided yet
- **THEN** the round-trip SHALL keep waiting (no silent deny mid-deliberation); a human-scale wall-clock backstop SHALL bound the wait (covering the case where no GUI ever connects) and SHOULD surface a re-arm rather than auto-rejecting the plan

---

### Requirement: IPC security observability

The transport SHALL emit a logged audit line for every security-relevant IPC event so a bypassed or silently-broken boundary is detectable rather than indistinguishable from a quiet system: every rejected peer connection (with the rejected uid), every endpoint bind failure, every IPC-directory validation refusal, and every dead-peer deny. The best-effort non-blocking sends (the `notification` and `ask_user` notifier connects in the hook CLI) SHALL at minimum log on failure rather than silently swallowing the error. Repeated rejections from the same uid SHALL be rate-limited or coalesced (e.g. a count plus first/last timestamp per window) so that audit logging cannot itself be turned into a local disk-fill / log-rotation-thrash DoS by a hostile process spamming rejected connects.

#### Scenario: Rejected peer is logged, rate-limited

- **WHEN** the peer-credential boundary rejects a foreign-uid connection, or the IPC-directory validation refuses a foreign-owned directory, or a bind fails
- **THEN** the system SHALL emit a logged audit line identifying the event (and the rejected uid where applicable), so the condition is observable in logs

#### Scenario: Rejection logging cannot be weaponised

- **WHEN** a hostile process spams a high volume of rejected connections from the same foreign uid
- **THEN** the audit logging SHALL rate-limit or coalesce the repeated rejections rather than writing one unbounded line per attempt, so the logging cannot be used to fill the disk or drown the genuine signal

### Requirement: No Linux regression

The abstraction SHALL preserve all existing Linux behaviour for the hook, MCP, and cross-session flows: the same length-framing, the same `0600` socket permissions, and the same peer-uid boundary semantics. The one intentional, security-motivated change to observable state is the endpoint LOCATION: endpoints move from `/tmp/nergal*.{sock,fifo}` (shared sticky `/tmp`) to the per-user IPC directory rooted at `$XDG_RUNTIME_DIR/nergal/` (with the home/state fallback when `$XDG_RUNTIME_DIR` is absent). Because the endpoints are ephemeral runtime artifacts recreated on each launch (not persisted state), and because a single shared resolver derives the path identically on the binding and connecting sides, this relocation is transparent to every flow and introduces no data migration. Aside from that relocation, introducing the seam SHALL NOT change any observable behaviour of the hook, MCP, or cross-session flows on Linux.

#### Scenario: Existing Linux flows are unchanged

- **WHEN** the seam is in place and Nergal runs on Linux
- **THEN** the hook event socket, the MCP daemon, cross-session messaging, and the plan-review / ask-user round-trips SHALL behave exactly as before (framing, `0600` permissions, and peer-uid boundary unchanged) except for the endpoints now living inside the per-user IPC directory rooted at `$XDG_RUNTIME_DIR/nergal/`, and the full machine-check suite SHALL pass

#### Scenario: Peer-uid comparison branch is covered by an automated CI test

- **WHEN** the seam refactor repoints the MCP and hook flows through the abstraction
- **THEN** an automated CI test SHALL inject a foreign uid into the peer-credential comparison (mock `peer_uid`) and assert the connection is rejected, so a refactor that silently drops the only enforced access control fails the suite rather than passing every existing check — this test covers the comparison branch ONLY and SHALL NOT be claimed to verify the OS-level `peer_cred()` extraction (which the real foreign-uid acceptance harness covers separately)

### Requirement: Windows transport contract

On Windows the transport SHALL be implemented via a named pipe at `\\.\pipe\nergal-<user-SID>-*` using `tokio::net::windows::named_pipe` (`ServerOptions`/`ClientOptions`; `NamedPipeServer`/`NamedPipeClient` already implement `AsyncRead + AsyncWrite`), with an owner-only security descriptor built from the SDDL `D:P(A;;GA;;;<user-SID>)` via `ConvertStringSecurityDescriptorToSecurityDescriptorW` and passed to `create_with_security_attributes_raw`. Peer identity SHALL be the client user SID (`ImpersonateNamedPipeClient` → `OpenThreadToken(openasself=true)` → `GetTokenInformation(TokenUser)` → `RevertToSelf`), mirroring `SO_PEERCRED`. The TCP-loopback + per-session-token variant remains a documented fallback (subject to the Constrained Windows loopback fallback requirement) and SHALL NOT be adopted unless a named-pipe edge case forces it. `PlatformStream` SHALL connect to the pipe and satisfy the same accept/connect/frame contract and owner-only boundary as the Unix implementation.

#### Scenario: Windows listener binds an owner-only named pipe

- **WHEN** the Windows transport binds the hook or MCP endpoint
- **THEN** `PlatformListener` SHALL create a named pipe under `\\.\pipe\nergal-<user-SID>-*` with the owner-only SDDL security descriptor and `first_pipe_instance(true)`, and `PlatformStream` SHALL connect to it, satisfying the same accept/connect/frame contract and owner-only boundary as the Unix implementation

#### Scenario: Crate still compiles and the Unix path is unchanged

- **WHEN** the crate is compiled for `aarch64-apple-darwin` / `x86_64-unknown-linux-gnu`
- **THEN** the Unix transport SHALL be byte-identical to before (the named-pipe body is `#[cfg(windows)]`-only), and the macOS/Linux flows SHALL be unaffected

---

### Requirement: Cross-platform opaque peer identity

The transport SHALL expose peer identity from `PlatformListener::accept` as an opaque `PeerIdentity` value rather than a bare `u32` uid, so the accept-loop access-control check is portable. `PeerIdentity` SHALL carry the Unix peer uid on Unix and the Windows client user SID on Windows, and SHALL expose a `matches_current_process()` predicate that is true iff the peer is the same security principal as the running process (Unix: `uid == getuid()`; Windows: the client SID `EqualSid` the process owner SID). The hook server and MCP daemon accept loops SHALL gate on `matches_current_process()`, never on a raw integer comparison, so the same access-control code path compiles and enforces on both families.

#### Scenario: Accept loop uses the opaque predicate on every platform

- **WHEN** the hook server or MCP daemon accepts a connection on Unix or Windows
- **THEN** it SHALL obtain a `PeerIdentity` from `accept` and reject the connection unless `matches_current_process()` is true — and on Unix the predicate SHALL evaluate `uid == getuid()`, preserving the existing uid-wall behaviour byte-for-byte

#### Scenario: Peer identity never collapses to a peer PID

- **WHEN** peer identity is computed on any platform
- **THEN** it SHALL derive from the peer's security principal (uid / user SID), never from a peer process id (macOS exposes no peer PID; the boundary must not depend on one)

---

### Requirement: Windows per-user pipe naming and anti-squat

On Windows, IPC endpoints SHALL be named `\\.\pipe\nergal-<user-SID-string>-<endpoint>`, where `<user-SID-string>` is the current user's SID rendered via `ConvertSidToStringSidW`. Embedding the per-user SID gives the cross-user isolation that the per-user `0700` directory gives on Unix (a different user's Nergal binds a different pipe name; no collision). The server SHALL create the first instance with `first_pipe_instance(true)` so that if a hostile process has pre-created a pipe of the same name, `create` fails (`PermissionDenied`) and the server SHALL refuse to start and log the condition (a squat is a security alert, not a silently-rebound endpoint). The connecting side SHALL, after opening the pipe, verify via `GetSecurityInfo(SE_KERNEL_OBJECT, OWNER_SECURITY_INFORMATION)` that the pipe's owner SID `EqualSid` the current user's SID before sending any payload, rejecting the connection otherwise (anti-impersonation — a different user on a multi-session host cannot lure the client into a same-named hostile pipe).

#### Scenario: Squatted pipe name is detected, not rebound

- **WHEN** a hostile process pre-creates `\\.\pipe\nergal-<sid>-hook` before Nergal's hook server binds
- **THEN** the server's `create` (with `first_pipe_instance(true)`) SHALL fail with `PermissionDenied`, and the server SHALL refuse to start the hook surface and log the squat rather than binding a name an attacker controls

#### Scenario: Client rejects a foreign-owned pipe

- **WHEN** the shim or hook CLI opens a pipe whose server end is owned by a different user's SID
- **THEN** the client SHALL detect the owner mismatch via `GetSecurityInfo` + `EqualSid` and refuse to send any request payload over it

---

