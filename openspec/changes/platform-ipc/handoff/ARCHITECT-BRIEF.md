# Architect Brief — platform-ipc

**Project mission**: Nergal — Linux desktop wrapper for the Claude Code CLI. Tauri 2 + React 19. The agent CLI runs in a real PTY; React panels mirror state via Jotai atoms fed by the hook pipeline and transcript watchers. **Now porting to macOS + Windows, macOS first.** ALL IPC (hooks, MCP server, cross-session messaging) runs over Unix domain sockets + FIFOs.

## Control metadata
- **Tier** L (macOS executable cut is S) · **ceremony** deep · **risk** **critical** · **files** ~8 · **visibility** public
- **Capability**: `platform-ipc-transport` (new)
- **Port sequence**: 4 of 5.

## What this change owns
Define the `PlatformListener`/`PlatformStream` abstraction over unix-socket vs windows-named-pipe. **macOS cut (now)**: `#[cfg(unix)]` already covers macOS, so this is mostly *verify + gate* — move (not rewrite) the current code behind the trait, gate `hooks/server.rs:207` chmod, keep POSIX FIFOs. **Windows (deferred)**: named-pipe body (design recommends named-pipe over TCP-loopback to preserve the `SO_PEERCRED` peer-auth via client SID + owner ACL) + unifying the blocking FIFOs onto the same primitive.

## Why risk = critical
Touches the IPC **security boundary**: the MCP daemon socket is `0600` + `SO_PEERCRED` peer-uid checked. Any Windows transport MUST reproduce those guarantees, or the cross-session/agent surface is exposed. `security-reviewer` is **mandatory** at Mode B.

## Dependencies & sequencing
- **Depends on**: `platform-compile`. Independent of proc/desktop.
- **Blocks**: nothing (Windows iteration is a separate follow-up on this same change).

## Gating decision (Mode B)
- iprev **STRONGLY recommended** (risk critical) → opus evaluator before any build.
- Reviewer escalation: **security-reviewer mandatory**. Gates: compile + test + lint + security.

## Reference skills (A6)
- `tauri-app-dev` (IPC/commands), `rust-desktop-applications`. Rust IPC: `tokio::net::windows::named_pipe` for the deferred Windows body.

## Constraint
macOS iteration must not regress the real hook / MCP / cross-session flows; the trait seam is additive. Note macOS `sun_path` length limit (104 vs Linux 108) — verify socket paths under `temp_dir()` fit.
