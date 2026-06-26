# Design — Platform IPC transport

Records the technical decisions behind the `platform-ipc-transport` capability. Part of the macOS-first multiplatform port (vault: `Projects/nergal/Multiplatform port scoping.md`, cluster A1/A2). macOS is the first target; Windows is a later iteration.

## Context

All of Nergal's IPC runs over Linux primitives:

- **Hook event socket** (`hooks/server.rs`): `tokio::net::UnixListener::bind` (`:203`), fire-and-forget newline-delimited messages, `set_permissions(0o600)` in a bare (ungated) block (`:206-209`). The hook CLI (`hooks/cli.rs:3`) imports `std::os::unix::net::UnixStream` and connects at `:16,47,178,250,292`.
- **MCP daemon** (`mcp/transport.rs`): `UnixSocketTransport` already wraps `UnixListener`/`UnixStream` with length-framed (4-byte LE) messages. Its perms (`:83`) and `peer_uid` via `SO_PEERCRED` (`:113-124`) are **already `#[cfg(unix)]`-gated**, and the module doc already states the intent that "a future Windows named-pipe transport drops in without touching dispatch." This is the reference pattern for the rest of the surface.
- **Blocking request/response** uses POSIX FIFOs: plan-review creates `/tmp/nergal-plan-{pid}.fifo` via `mkfifo` (`hooks/cli.rs:142,146`) and blocks on `read_to_string`; ask-user uses `nergal-ask-*` FIFOs (`agents/claude_code/adapter.rs` `register_pending_ask_fifo` / `submit_ask_answer`). `mkfifo` has no Windows equivalent.

Crucial fact for scoping: **`#[cfg(unix)]` is true on macOS.** `UnixStream`, `UnixListener`, `mkfifo`, and `SO_PEERCRED` all exist and work on macOS. So the macOS port is *verify the unix path runs* + *introduce the seam* + *gate the few ungated unix-only spots so the crate can compile under `#[cfg(windows)]`* — **not a rewrite**. Windows named-pipe / loopback work is deferred.

The socket base path is already portable: both `config.rs:305` and `mcp/mod.rs:46` derive from `std::env::temp_dir()`, not a hardcoded `/tmp`. No change needed there (confirmed).

## Goals / Non-Goals

**Goals:**

- Introduce a `PlatformListener` / `PlatformStream` seam that the hook server and MCP daemon use, so the Windows iteration drops in without touching the call sites or dispatch logic.
- Move (not rewrite) the existing Unix code behind the seam; zero behaviour change on Linux.
- Gate the remaining ungated Unix-only primitives (`hooks/server.rs` perms, `hooks/cli.rs` `unix::net` import + connects) behind `#[cfg(unix)]` so the crate compiles under a Windows target.
- Define the full cross-platform contract in the spec: per-platform peer auth, the unified blocking primitive, the Windows named-pipe/loopback target.
- Verify the real hook / MCP / cross-session flows on macOS.

**Non-Goals:**

- Implementing the Windows transport body (named-pipe or loopback). Deferred; only the seam + the contract land now.
- Migrating the plan-review / ask-user FIFOs off `mkfifo` for Linux/macOS. POSIX works on both; the unification is the documented target, implemented with the Windows path.
- Any frontend change (IPC is backend-only), any DB/migration, any new runtime dependency.
- The other multiplatform clusters (A3 `/proc`→sysinfo, B xdg/notify→Tauri plugins, C bundle+CI+signing). Tracked separately.

## Decisions

### Decision 1: A trait seam (`PlatformListener` / `PlatformStream`), not per-call-site `#[cfg]`

**Chosen:** Define two abstractions in a new `src-tauri/src/platform/ipc.rs` (or `ipc/mod.rs`) module — a listener that binds + accepts, and a stream that connects + reads/writes — with a Unix implementation that wraps today's `UnixListener`/`UnixStream`. The hook server and MCP daemon program against the abstraction.

**Alternatives considered:**

- *Sprinkle `#[cfg(unix)]` / `#[cfg(windows)]` at every call site.* Rejected: the connect sites alone are five points in `hooks/cli.rs` plus the MCP handlers; duplicating bind/accept/connect logic per-OS at each site is unmaintainable and is exactly what the MCP module doc already warns against. A single seam localises the per-OS code.
- *Adopt an existing crate (e.g. `interprocess`).* `interprocess` does offer a `LocalSocket` abstraction over unix-socket + named-pipe. Attractive, but (a) it adds a dependency and a migration of the already-working, already-`cfg`-gated MCP transport; (b) its framing/credential model differs from the hand-rolled length-framing + `SO_PEERCRED` we rely on; (c) the macOS iteration does not need it (unix path works). **Decision: hand-roll the thin seam now; re-evaluate `interprocess` if/when the Windows body proves it carries its weight.** Noted as an open question.

**Shape:** mirror the existing `UnixSocketTransport` (`mcp/transport.rs:70`), which already has the right surface (`bind` → `accept` → stream + peer identity, `connect`). Generalise its async-reader/writer-generic framing helpers — they are already platform-agnostic and need no change. The hook socket (newline-delimited, fire-and-forget, no response path) and the MCP socket (length-framed, request/response) are different protocols over the **same** transport primitive; the seam is the listener/stream, not the protocol.

### Decision 2: macOS-iteration cut — Unix impl + seam only; Windows deferred

The trait gets exactly one implementation this iteration (Unix). The Windows impl is a stub/absent module gated by `#[cfg(windows)]`. The crate must still **compile** under a Windows target (CI doesn't build Windows yet, but `cargo check --target` should not blow up on ungated `unix::net`), which is why the gating tasks (Decision 4) matter even before the Windows body exists.

Rationale: the user decision is explicit — macOS first, Windows deferred. Building the Windows body now would be speculative (named-pipe vs loopback is itself unresolved, Decision 3) and untestable without a Windows CI target.

### Decision 3: Windows transport — named-pipe vs TCP-loopback+token (deferred, recommendation recorded)

Both are viable; recording the trade-off so the deferred iteration starts from a decision, not a blank page.

| Aspect | Named pipe (`\\.\pipe\nergal-*`) | TCP loopback + token |
|---|---|---|
| Access control | OS-native security descriptor (owner-only ACL) — maps directly to today's uid wall | Any local process can connect to the port; a per-session secret token must be checked in-band |
| Peer authentication | `GetNamedPipeClientProcessId` / SID on the pipe — true peer identity, mirrors `SO_PEERCRED` | No OS peer identity; the token *is* the auth. Token must be delivered to the shim out-of-band (env var / file) and not leak |
| Tokio support | `tokio::net::windows::named_pipe` (in-tree, no new dep) | `tokio::net::TcpListener` (already used) |
| Path model | Pipe name string — analogous to a socket path | Host+ephemeral port; port must be discovered/published to the shim |
| Firewall / surface | No network surface | Binds a loopback port; AV/firewall noise possible |

**Recommendation:** named pipe. It preserves the security model we have on Unix (OS-enforced owner-only access + real peer identity via the pipe's client SID), needs no new dependency (`tokio` ships the named-pipe types), and maps cleanly onto the existing path-string abstraction (`std::env::temp_dir()`-style name → pipe name). Loopback+token is the fallback only if a named-pipe edge case (e.g. a sandboxed agent CLI that can't open pipes) forces it; it trades OS-enforced ACL for an in-band secret we'd have to manage and protect. The MCP socket is security-sensitive (today: `0600` + uid check), so the ACL-backed option is the safer default.

### Decision 4: Gate the ungated Unix-only spots

Two spots are currently ungated and will break a Windows compile:

- `hooks/server.rs:206-209` — `set_permissions(0o600)` in a bare `{ }` block. Wrap in `#[cfg(unix)]` (the MCP transport at `:83` is the exact pattern to copy).
- `hooks/cli.rs:3` — top-level `use std::os::unix::net::UnixStream;` and the five connect sites. These move behind the seam (`PlatformStream::connect`); the `unix::net` import disappears from the call site and lives only inside the Unix impl module.

The MCP transport's perms (`:83`) and `peer_uid` (`:115-124`, with a `#[cfg(not(unix))]` stub) are already gated — leave them, they are the template.

### Decision 5: Unify the blocking FIFOs onto the primitive — contract now, Linux/macOS keep FIFO

The blocking plan-review / ask-user round-trips are the one place that uses `mkfifo`, which has no Windows analogue. The spec defines that these round-trips move onto the `PlatformStream` primitive (a short-lived per-request connection carrying the decision/answer back), unifying the mechanism. **But for the macOS iteration the existing FIFO path stays** — POSIX `mkfifo` works on both Linux and macOS, the code is proven, and rewriting it buys nothing for the macOS target. The unification is sequenced with the Windows iteration, where it becomes necessary. The seam is designed so this is a drop-in: the blocking response is just another `PlatformStream` round-trip.

Rationale for not unifying now: scope discipline. The macOS acceptance criterion ("real flows pass on macOS, no Linux regression") is met without touching the FIFO code; doing the unification now would be churn against a working path with no testable Windows target to validate it against.

## Risks / Trade-offs

- **[Seam leaks Unix assumptions]** → Model the trait on `UnixSocketTransport`, whose surface (`bind`/`accept`→(stream, peer-id)/`connect`) is already platform-neutral and whose author already intended a Windows drop-in. Peer identity is returned as an opaque value (uid today; pipe SID later), not a `u32` baked into the signature where avoidable.
- **[Hook socket vs MCP socket are different protocols]** → The seam abstracts the *transport* (listener/stream), not the protocol. Newline-delimited fire-and-forget (hook) and length-framed request/response (MCP) both ride the same `PlatformStream`; no attempt to unify the protocols, only the primitive.
- **[Windows compile rot]** → Without a Windows CI target, ungated `unix::net` can creep back. Mitigation: the gating tasks land the `#[cfg(unix)]` discipline now; a follow-up can add `cargo check --target x86_64-pc-windows-msvc` to CI when the Windows body starts.
- **[Hand-rolled seam later duplicates `interprocess`]** → Accepted: the seam is thin (≈ one module). If the Windows body shows the abstraction is non-trivial, swapping the internals for `interprocess` is localised to the impl module, not the call sites. Recorded as an open question.
- **[macOS behavioural surprise despite `#[cfg(unix)]`]** → `mkfifo`, `SO_PEERCRED`, `temp_dir()` are POSIX/portable but macOS edge cases exist (e.g. `temp_dir()` returns a long per-user `/var/folders/...` path — socket path length limit `sun_path` ~104 bytes on macOS vs ~108 on Linux). Mitigation: explicit macOS verification task for the real flows, and a check that the derived socket path fits the macOS `sun_path` bound.

## Migration Plan

1. Land the seam module + Unix impl (wrapping existing code), no call-site behaviour change.
2. Repoint hook server + MCP daemon + hook CLI connect sites through the seam.
3. Gate the ungated Unix-only spots (`hooks/server.rs` perms, `hooks/cli.rs` import).
4. Verify Linux full-check green (no regression) — this is the rollback gate: if any Linux flow changes, revert to step 0.
5. Verify the real flows on macOS.
6. (Deferred, separate iteration) Implement the Windows named-pipe impl + migrate the blocking FIFOs onto the primitive.

Rollback: the change is additive + a move; reverting the seam restores the direct `unix::net` usage. No data/schema involved, so rollback is code-only.

## Open Questions

- **`interprocess` crate vs hand-rolled seam for the Windows body** — re-evaluate when the Windows impl starts; the seam is designed so the decision is localised to the impl module.
- **Peer-identity type in the trait signature** — uid (`u32`) on Unix vs a pipe client SID on Windows. Resolve when the Windows body lands; for now the Unix impl returns the uid as it does today, and the trait can expose an opaque `PeerId` if needed.
- **macOS `sun_path` length** — confirm the `temp_dir()`-derived socket path stays within the macOS `AF_UNIX` path limit during the macOS verification task; if not, shorten the socket name.
- **Whether the hook socket should also adopt length-framing** when unified — out of scope; the hook protocol stays newline-delimited fire-and-forget, only the transport primitive is shared.
