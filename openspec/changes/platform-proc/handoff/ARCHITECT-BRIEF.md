# Architect Brief — platform-proc

**Project mission**: Nergal — Linux desktop wrapper for the Claude Code CLI. Tauri 2 + React 19. The agent CLI runs in a real PTY; React panels mirror state via Jotai atoms fed by the hook pipeline and transcript watchers. **Now porting to macOS + Windows, macOS first.**

## Control metadata
- **Tier** L · **ceremony** deep · **risk** medium · **files** ~5 · **visibility** public
- **Capability**: `platform-process-inspection` (new)
- **Port sequence**: 2 of 5.

## What this change owns
The **functional** macOS port of every `/proc`-based subsystem behind one `platform_proc` module: process tree (PPID walk), process cwd, and listening-TCP-port discovery + owning pid/exe/cmdline/cwd. Powers the ports status-bar chip / live-preview (browser.rs), quake-shell cwd (pty.rs), and cross-session Codex env recovery (shim.rs). **Recommendation (design D2a, spike-gated)**: adopt `sysinfo` + `netstat2`/`listeners` to collapse 3 per-OS impls into 1, rather than hand-rolling libproc/sysctl. Linux behavior preserved bit-for-bit.

## Dependencies & sequencing
- **Depends on**: `platform-compile` (HARD — needs `libc` → `cfg(unix)` to compile).
- **Blocks**: nothing directly; bundle-ci depends on it transitively.

## Gating decision (Mode B)
- iprev **triaged** (L + deep + files ≥ 5) → recommended, Claude evaluator, 3 rounds.
- Reviewer: single-sequential (code-quality; spec-reviewer if the module boundary drifts). Gates: compile + test + lint.
- **Spike suggested** before committing the crate choice (D2a): validate `sysinfo` gives PPID + cwd on both Linux and macOS, and `netstat2`/`listeners` enumerates listeners with owning pid.

## Reference skills (A6)
- `tauri-app-dev`, `rust-desktop-applications` — Rust cross-platform process/IO patterns.

## Constraint
No Linux regression: the ports chip (3s scan), quake cwd, and Codex ancestry must behave identically on Linux post-refactor; macOS adds the same behaviors.
