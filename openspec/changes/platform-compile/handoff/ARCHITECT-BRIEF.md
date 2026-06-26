# Architect Brief — platform-compile

**Project mission**: Nergal — Linux desktop wrapper for the Claude Code CLI. Tauri 2 + React 19. The agent CLI runs in a real PTY; React panels mirror state via Jotai atoms fed by the hook pipeline and transcript watchers. **Now porting to macOS + Windows, macOS first.**

## Control metadata
- **Tier** M · **ceremony** standard · **risk** medium · **files** ~8 · **visibility** public
- **Capability**: `platform-compat` (new)
- **Port sequence**: 1 of 5 — **the foundation; blocks all siblings.**

## What this change owns
Make the Rust crate **compile** on macOS (and lay Windows groundwork). NOT functional behavior. Key move: `Cargo.toml` `libc` from `[target.'cfg(target_os="linux")']` → `[target.'cfg(unix)']` (POSIX `getuid`/`statvfs`/`kill`/`setsid` then compile on macOS since `#[cfg(unix)]` is true there). Plus: `which`-crate swap, `/etc/os-release` cfg-gate, opencode config path via `dirs`, and the **`cargo check --target` CI gate** (enforcement arm of the cross-platform invariant). Windows `std::os::unix` gating recorded as deferred.

## Dependencies & sequencing
- **Depends on**: nothing — start here.
- **Blocks**: platform-proc, platform-desktop, platform-ipc, platform-bundle-ci (all assume the crate compiles on macOS).

## Gating decision (Mode B)
- iprev **triaged** (files ≥ 5) → recommended at Mode B entry, Claude evaluator, 3 rounds.
- Reviewer: single-sequential (code-quality). Gates: compile + test + lint. The cross-target `cargo check` IS the headline acceptance.

## Reference skills (A6)
- `tauri-app-dev` and `rust-desktop-applications` skills — read at implementation time for Tauri 2 cross-platform bundling + Rust `#[cfg]` quarantine patterns. Lazy library (`~/.claude/skill-library/`) has no Rust-port match (frontend/API/db focused).

## Constraint
Implementer must NOT regress the Linux build. Acceptance = `cargo check --target aarch64-apple-darwin` green (with sibling stubs) AND full Linux verify unchanged.
