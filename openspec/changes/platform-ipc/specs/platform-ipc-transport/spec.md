## ADDED Requirements

### Requirement: Platform transport abstraction

The system SHALL define a `PlatformListener` / `PlatformStream` transport abstraction through which the hook server, the MCP daemon, cross-session messaging, and the blocking request/response round-trips bind, accept, and connect. The hook server and MCP daemon SHALL obtain their listener and streams from this abstraction rather than referencing `std::os::unix::net` or `tokio::net::Unix*` types directly at the call site. The Unix implementation SHALL wrap the existing `UnixListener` / `UnixStream` code with no behavioural change; the existing length-framing helpers (`read_frame` / `write_frame`, generic over async reader/writer) SHALL remain unchanged and continue to operate over the abstracted stream.

#### Scenario: Hook server binds through the abstraction

- **WHEN** the hook server starts and binds its socket
- **THEN** it SHALL obtain the listener via the `PlatformListener` abstraction and accept connections as `PlatformStream` values, with no raw `unix::net` type at the call site

#### Scenario: MCP daemon binds through the abstraction

- **WHEN** the MCP daemon starts and binds its socket
- **THEN** it SHALL obtain the listener via the `PlatformListener` abstraction, and the length-framed read/write helpers SHALL operate unchanged over the resulting stream

#### Scenario: Connect side uses the abstraction

- **WHEN** the hook CLI or the MCP shim connects to a daemon socket
- **THEN** it SHALL connect via the `PlatformStream` abstraction rather than calling `UnixStream::connect` directly at the call site

### Requirement: macOS runs the POSIX path unchanged

Because `#[cfg(unix)]` is true on macOS, the system SHALL run the existing Unix-socket and FIFO transport on macOS through the abstraction without a rewrite. The real hook event flow, the MCP directory/messaging flow, and the cross-session messaging flow SHALL function on macOS via the Unix implementation of the transport.

#### Scenario: Hook, MCP, and cross-session flows operate on macOS

- **WHEN** Nergal runs on macOS and an agent session emits hook events, the MCP daemon serves a `tools/call`, and a cross-session message is delivered
- **THEN** each SHALL succeed over the Unix implementation of the transport, with no Windows-specific code required

### Requirement: Per-platform peer-authentication boundary

The transport SHALL enforce a per-platform peer-authentication boundary on the security-sensitive MCP socket. On Unix the boundary SHALL be the peer-credential uid check (`SO_PEERCRED`): a connection from a process owned by a different uid SHALL be rejected. On Windows the boundary SHALL be a named-pipe security descriptor (owner-only ACL) or, for the TCP-loopback variant, a per-session authentication token presented on connect; this Windows behaviour is the target contract and is deferred to the Windows iteration.

#### Scenario: Different-uid peer is rejected on Unix

- **WHEN** a process owned by a different uid connects to the MCP daemon socket on Unix (Linux or macOS)
- **THEN** the transport SHALL reject the connection via the peer-credential uid check

#### Scenario: Windows peer authentication (deferred)

- **WHEN** the Windows transport iteration is implemented
- **THEN** the named-pipe variant SHALL restrict access via an owner-only security descriptor, OR the loopback variant SHALL require a valid per-session token on connect, denying unauthenticated peers — matching the owner-only boundary the Unix uid check provides today

### Requirement: Platform-gated socket permissions

The system SHALL gate all Unix-only permission calls behind `#[cfg(unix)]` so the crate compiles under a non-Unix target. Specifically the hook server's `set_permissions(socket, 0o600)` block SHALL be `#[cfg(unix)]`-gated (matching the already-gated MCP transport at `mcp/transport.rs:83`). On Windows, owner-only access SHALL be provided by the named-pipe security descriptor instead of POSIX file mode (deferred).

#### Scenario: Hook server permissions are gated on Unix

- **WHEN** the hook server binds its socket on Unix
- **THEN** it SHALL set mode `0600` inside a `#[cfg(unix)]` block, so other local users cannot inject spoofed hook events, and the same code SHALL compile (excluding the POSIX mode call) under a Windows target

### Requirement: Unified blocking request/response primitive

The blocking plan-review and ask-user round-trips SHALL be expressible over the same `PlatformStream` primitive as the rest of IPC, rather than depending on a separate `mkfifo` mechanism that does not exist on Windows. The observable blocking semantics SHALL be preserved: the hook CLI blocks until the GUI writes a decision/answer, then proceeds. On Linux and macOS the existing FIFO implementation (`mkfifo`, POSIX) SHALL continue to satisfy this contract for the macOS iteration; the migration of these round-trips onto the unified primitive is the defined target, implemented when the Windows path lands.

#### Scenario: Plan-review blocks and resolves on Unix

- **WHEN** an agent triggers a plan review and the user submits a decision in the GUI
- **THEN** the hook CLI SHALL block until the decision arrives and then emit the corresponding allow/deny output, using the POSIX FIFO path on Linux and macOS

#### Scenario: Blocking round-trips work without mkfifo (deferred Windows target)

- **WHEN** the Windows transport iteration is implemented
- **THEN** plan-review and ask-user SHALL perform the same blocking request/response over the `PlatformStream` primitive (named-pipe or loopback), with no dependency on `mkfifo`, preserving the block-until-decision semantics

### Requirement: Windows transport contract (deferred)

On Windows the transport SHALL be implemented via a named pipe at `\\.\pipe\nergal-*` (using `tokio::net::windows::named_pipe`) or via TCP loopback with a per-session auth token. This requirement defines the target cross-platform contract; the macOS iteration SHALL implement only the Unix path plus the trait seam, and the Windows implementation SHALL be a follow-up iteration. The crate SHALL nonetheless compile under a Windows target (all Unix-only primitives gated), even though the Windows transport body is not yet provided.

#### Scenario: Windows listener binds a named pipe or loopback (deferred)

- **WHEN** the Windows transport iteration is implemented
- **THEN** `PlatformListener` SHALL bind a named pipe under `\\.\pipe\nergal-*` (owner-ACL secured) or a loopback TCP socket (token-secured), and `PlatformStream` SHALL connect to it, satisfying the same accept/connect/frame contract as the Unix implementation

### Requirement: No Linux regression

The abstraction SHALL preserve all existing Linux behaviour: the same socket paths (derived from `std::env::temp_dir()`), the same length-framing, the same `0600` permissions, and the same `SO_PEERCRED` uid boundary. Introducing the seam SHALL NOT change any observable behaviour of the hook, MCP, or cross-session flows on Linux.

#### Scenario: Existing Linux flows are unchanged

- **WHEN** the seam is in place and Nergal runs on Linux
- **THEN** the hook event socket, the MCP daemon, cross-session messaging, and the plan-review / ask-user round-trips SHALL behave exactly as before (paths, framing, permissions, and peer-uid boundary unchanged), and the full machine-check suite SHALL pass
