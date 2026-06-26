# Tasks — Platform IPC transport

Phased; each phase independently verifiable (`cd src-tauri && cargo clippy -- -D warnings && cargo test && cargo fmt --check && cd .. && npx tsc --noEmit`). macOS-first: implement the Unix path + the trait seam now; Windows tasks are marked **(deferred)** and are NOT executed this iteration.

## 1. Transport seam + Unix implementation

- [ ] 1.1 Create `src-tauri/src/platform/ipc.rs` defining the `PlatformListener` / `PlatformStream` abstraction: a listener that binds a path (removing any stale endpoint), accepts into `(stream, peer_identity)`, and a stream that connects a path. Model the surface on `mcp/transport.rs:70` `UnixSocketTransport`. Register the module (`mod platform;`) in `lib.rs`.
- [ ] 1.2 Provide the `#[cfg(unix)]` Unix implementation, **moving** (not rewriting) the existing bind/accept/connect logic. Expose both an async listener/stream (for the hook server + MCP) and a sync connect (for the blocking hook CLI). Carry peer identity as the existing uid, surfaced through the seam so a future pipe-SID drop-in needs no call-site change.
- [ ] 1.3 Add a `#[cfg(windows)]` placeholder/stub module so the file's shape is ready for the deferred Windows body and the crate's module tree is platform-complete. **(deferred body)**
- [ ] 1.4 Verify: `cargo check` + `cargo clippy -- -D warnings` clean on Linux; the seam unit-tests against in-memory duplex pipes (reuse the `mcp/transport.rs` test style).

## 2. MCP daemon onto the seam

- [ ] 2.1 Make `mcp/transport.rs` `UnixSocketTransport` the canonical Unix impl of the seam (or have the seam re-export it). Do NOT touch `read_frame`/`write_frame` (`:32,53`), the already-gated `set_permissions(0o600)` (`:83`), or `peer_uid` (`:115-124`).
- [ ] 2.2 Repoint the MCP handlers (`mcp/mod.rs:608,660`) and the shim relay (`shim.rs:112`) to obtain streams via the seam's stream type instead of naming `tokio::net::UnixStream` at the call site.
- [ ] 2.3 Verify: MCP unit tests green; manual `tools/call` round-trip via the shim works on Linux unchanged.

## 3. Hook server onto the seam + gating

- [ ] 3.1 `hooks/server.rs`: bind via the seam (`:203`); keep the `accept` loop + `BufReader::lines()` newline framing unchanged.
- [ ] 3.2 Wrap the ungated `set_permissions(socket, 0o600)` block (`:206-209`) in `#[cfg(unix)]`, copying the `mcp/transport.rs:83` pattern.
- [ ] 3.3 Verify: hook event round-trip (session-state update, attention indicator) behaves identically on Linux.

## 4. Hook CLI onto the seam

- [ ] 4.1 `hooks/cli.rs`: remove the top-level `use std::os::unix::net::UnixStream;` (`:3`); route the five connect sites (`:16,47,178,250,292`) through the seam's sync `PlatformStream::connect`.
- [ ] 4.2 Leave the plan-review (`:142,146,188`) and ask-user (`agents/claude_code/adapter.rs` register/`submit_ask_answer`) POSIX FIFOs as-is for Linux+macOS (the unified-primitive contract is satisfied at spec level; implementation is sequenced with Windows). Add a code comment pointing at the deferred unification (WHY).
- [ ] 4.3 Verify: plan review blocks then resolves; ask-user blocking round-trip resolves; notification + rescan best-effort sends still work on Linux.

## 5. Linux no-regression gate

- [ ] 5.1 Confirm no ungated `std::os::unix` / `tokio::net::Unix*` reference remains outside the `#[cfg(unix)]` seam impl (grep `src-tauri/src`).
- [ ] 5.2 Full check green: `cd src-tauri && cargo clippy -- -D warnings && cargo test && cargo fmt --check && cd .. && npx tsc --noEmit`.
- [ ] 5.3 Manual Linux walk of all four real flows (hook event, plan review, MCP tools/call, cross-session message) — behaviour unchanged.

## 6. macOS verification

- [ ] 6.1 Build and run on macOS; confirm the `std::env::temp_dir()`-derived socket path binds within the macOS `AF_UNIX` `sun_path` length limit (log + shorten the socket filename if it overflows).
- [ ] 6.2 Walk the four real flows on macOS (hook event, plan review, MCP tools/call, cross-session message) through the seam; all pass over the Unix implementation with no Windows-specific code.

## 7. Windows transport (deferred — not executed this iteration)

- [ ] 7.1 **(deferred)** Implement the `#[cfg(windows)]` `PlatformListener`/`PlatformStream` body via `tokio::net::windows::named_pipe` at `\\.\pipe\nergal-*` (recommended per design Decision 3) with an owner-only security descriptor; fallback to TCP-loopback + per-session token if a named-pipe edge case forces it.
- [ ] 7.2 **(deferred)** Map peer authentication: named-pipe client SID (mirrors `SO_PEERCRED`) or token validation for the loopback variant.
- [ ] 7.3 **(deferred)** Migrate the plan-review + ask-user blocking round-trips off `mkfifo` onto the `PlatformStream` primitive (no `mkfifo` on Windows), preserving block-until-decision semantics.
- [ ] 7.4 **(deferred)** Add `cargo check --target x86_64-pc-windows-msvc` to CI to prevent ungated `unix::net` regressions.

## 8. Verification

- [ ] 8.1 Full machine check green: `cd src-tauri && cargo clippy -- -D warnings && cargo test && cargo fmt --check && cd .. && npx tsc --noEmit`.
- [ ] 8.2 macOS acceptance met: real hook / MCP / cross-session / plan-review flows pass on macOS; the trait seam exists; no Linux regression.
- [ ] 8.3 Run `openspec validate platform-ipc --strict` and confirm it passes.
