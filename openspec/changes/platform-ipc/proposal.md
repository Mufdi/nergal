## Why

Every inter-process channel in Nergal — the hook event socket, the MCP daemon, cross-session messaging, and the blocking plan-review / ask-user round-trips — is wired directly to Linux primitives: `std::os::unix::net::UnixStream`, `tokio::net::UnixListener`, `mkfifo`, and `SO_PEERCRED`. To port Nergal to macOS (first) and Windows (later), these call sites need a transport seam instead of hard-coded `unix::net` imports. On macOS the work is mostly verification — `#[cfg(unix)]` is true, so `UnixStream`/`UnixListener`/`mkfifo` already compile and run — but the seam must exist before the Windows iteration can drop in named-pipe / loopback support, and a few `unix`-only paths must be gated so the codebase even compiles under `#[cfg(windows)]`.

## What Changes

- **New `PlatformListener` / `PlatformStream` transport abstraction** in `src-tauri` that both the hook server and the MCP daemon bind/accept/connect through. The Unix implementation **moves** (does not rewrite) the existing `UnixListener`/`UnixStream` code behind the trait; the framing helpers (`mcp/transport.rs` `read_frame`/`write_frame`, already generic over `AsyncRead`/`AsyncWrite`) stay unchanged.
- **Gate the ungated Unix-only call sites** behind `#[cfg(unix)]`: the hook server's `set_permissions(socket, 0o600)` block (`hooks/server.rs:206-209`) and the raw `std::os::unix::net::UnixStream` import + connect sites in `hooks/cli.rs`. The MCP transport's perms and `peer_uid` are already gated (`mcp/transport.rs:83,115-124`) and serve as the reference pattern.
- **Unify the blocking request/response FIFOs onto the transport seam**: plan-review (`hooks/cli.rs:142,146` `mkfifo /tmp/nergal-plan-{pid}.fifo`) and ask-user (`nergal-ask-*` FIFOs, registered via `register_pending_ask_fifo`) currently use `mkfifo`, which does not exist on Windows. The contract moves these blocking round-trips onto the same `PlatformStream` primitive so a single transport works everywhere. **macOS-iteration cut**: Linux/macOS keep the existing FIFO path (POSIX, works today); the unification is the defined target and is implemented when the Windows path lands. The seam makes it a drop-in.
- **Define the per-platform peer-authentication contract**: Unix uses `SO_PEERCRED` uid check (the only enforced MCP access control today, `mcp/transport.rs:113-119`); Windows uses a named-pipe security descriptor or a loopback auth token. Windows behavior is specified as the target contract; **tasks implement only the Unix path + the trait seam this iteration.**
- The socket base path is already cross-platform (`config.rs:305`, `mcp/mod.rs:46` use `std::env::temp_dir()`, not hardcoded `/tmp`) — no change needed, confirmed.

## Capabilities

### New Capabilities

- `platform-ipc-transport`: A cross-platform IPC transport abstraction (`PlatformListener` / `PlatformStream`) over which the hook server, MCP daemon, cross-session messaging, and the blocking plan-review / ask-user round-trips operate. Covers the trait seam, the Unix implementation (moved existing code), per-platform peer-authentication and socket-permission gating, the unified blocking request/response primitive, the macOS verification contract, and the deferred Windows named-pipe / loopback contract.

### Modified Capabilities

<!-- None. The transport change is implementation-level: the requirement-level behaviour of nergal-mcp-server, cross-session-messaging, plan-panel-multi-agent, and session-launch-options is unchanged (same blocking semantics, same access boundary, same socket paths). Only the underlying primitive is abstracted. -->

## Impact

- **Backend (`src-tauri/src/`)**:
  - `hooks/cli.rs` — `use std::os::unix::net::UnixStream` (line 3); connect sites at lines 16, 47, 178, 250, 292; FIFO creation at 142/146.
  - `hooks/server.rs` — `UnixListener` import (line 6), `bind` (203), ungated `set_permissions 0o600` (206-209).
  - `mcp/mod.rs` — socket path (46), `UnixStream` handlers (608, 660).
  - `mcp/transport.rs` — `UnixListener`/`UnixStream` imports (18), `UnixSocketTransport` (70-104), already-gated perms (83) and `peer_uid` (113-124); the module doc already anticipates a Windows named-pipe drop-in.
  - `mcp/shim.rs` — relay (112).
  - `agents/claude_code/adapter.rs` — `pending_plan_fifos` / `pending_ask_fifos` writers (74, 80, 296-298, 307-313).
- **No frontend changes** — IPC is entirely backend.
- **No new external dependency this iteration** (Unix path uses std/tokio). The Windows iteration will use `tokio::net::windows::named_pipe` (already in `tokio`, behind a target cfg) — noted in design, not added now.
- **No DB / migration impact.**

## Build contract

### Qué construyo

- A `PlatformListener` / `PlatformStream` trait seam in `src-tauri` with a Unix implementation that moves the existing `UnixListener`/`UnixStream` code unchanged behind it.
- `#[cfg(unix)]` gating for the currently-ungated Unix-only call sites (hook server perms, hook CLI unix-stream import/connects) so the crate compiles under a Windows cfg.
- Spec-level definition of the unified blocking request/response primitive and the per-platform peer-authentication contract; Unix behaviour wired, Windows behaviour documented as the deferred target.

### Cómo verifico

- `cd src-tauri && cargo clippy -- -D warnings && cargo test && cargo fmt --check && cd .. && npx tsc --noEmit`
- macOS iteration acceptance: the real hook, MCP, and cross-session flows pass on macOS through the seam; the trait exists; no Linux regression.

### Criterio de done

- The hook server and MCP daemon bind/accept/connect through the transport seam, not raw `unix::net` at the call site.
- All Unix-only primitives are `#[cfg(unix)]`-gated; the crate compiles cleanly with the existing Linux target and the abstraction does not regress Linux behaviour.
- macOS runs the unix path; the Windows contract (named-pipe/loopback, pipe-ACL/token auth, FIFO unification) is specified and marked deferred.
- All machine checks green.

### Estimated scope

- files_estimate: 7
- risk_tier: medium
- tags: [refactor, platform, foundation]
- visibility: public
- spec_target: platform-ipc-transport
