# Windows Port Walk — Session Handoff (walk-3)

> Self-contained context for continuing the Nergal Windows port walk from a
> **native Windows checkout** (repo cloned on the Windows partition, Claude Code
> running on Windows). Written 2026-06-29 from the Linux dev session. The CC
> auto-memory and the Obsidian vault live on the Linux machine and do NOT travel
> with the clone — this doc carries everything you need. Code stays in sync via
> git `main` (push fixes from Windows; the Linux session pulls).

## Mission

Nergal = Linux/macOS/Windows desktop wrapper for the Claude Code CLI (Tauri 2 +
React 19). The agent CLI runs in a real PTY; React panels mirror state via Jotai
atoms fed by the **hook pipeline** + transcript watchers. Nergal runs *around*
the agent, augmenting the loop — it does not reimplement agent primitives.

**Goal of this walk**: validate the 5 implemented `windows-*` OpenSpec changes on
real Windows, fix the runtime bugs the compile gates can't catch, until the walk
passes. THEN archive the 5 changes and (LATER, not yet) cut a 3-platform release.

## Standing constraints (do NOT violate)

- **Do NOT delete any session from the DB** — the user has accumulated work to
  resume after the port closes.
- **Do NOT cut the real 3-platform release yet** — unrelated (non-port) bugs come
  first, by explicit user decision.
- **No silent `~/.claude/settings.json` mutation** — hook registration is
  one-click *consented* (the user runs it / clicks it), never silent on startup.
- **setup-wizard is PARKED** — documented in `openspec/changes/setup-wizard/`
  (tier L, deep ceremony), implement AFTER the walk closes. Onboarding findings
  from the walk fold into THAT change, don't create a new one.

## Native-Windows dev advantage (new this session)

Running on Windows unlocks what the Linux host could not do (ring/MSVC blocked
cross-compile):

- `cd src-tauri && cargo check --target x86_64-pc-windows-msvc` — validates the
  `#[cfg(windows)]` branches locally (was CI-only).
- `cd src-tauri && cargo test` — runs the Windows-arm tests, incl. windows-ipc
  task **9.2** (`peer_identity_rejects_foreign_sid`) that was walk-pending.
- `pnpm tauri dev` — live repro of the runtime bugs.
- Hook chain works in dev: `nergal hook setup` run from the dev binary registers
  the dev exe path (`current_exe()`), so hooks fire against the running dev app.

Prereqs on Windows: Rust (MSVC toolchain, `rustup default stable-msvc`), Visual
Studio Build Tools (C++ / MSVC linker — `ring` needs it), Node + `pnpm`, WebView2
runtime (preinstalled on Win11), git. Then `pnpm install` once.

## State of the 5 changes

- **windows-compile** — ARCHIVED. Crate builds on Windows.
- **windows-ipc** — code-complete, CI-green. Real **named-pipe transport**
  (`\\.\pipe\nergal-<SID>-<endpoint>`) replaces the Unix sockets/FIFOs for the
  hook server, MCP daemon, plan-review + ask-user blocking round-trips. Owner-only
  security descriptor + client-SID peer auth. Tasks 9.2 (`cargo test` on Windows)
  + 9.5 (runtime walk) were the only unchecked items → **this walk closes them**.
- **windows-proc** — code-complete. `sysinfo` + `listeners` for ports/proc;
  `OpenProcess`/`TerminateProcess` kill path. The port noise-filter is what
  walk-3 found still wrong (see below).
- **windows-desktop** — code-complete. Detach/reveal, deep-link.
- **windows-bundle-ci** — code-complete. NSIS+MSI bundle, 4-job release pipeline.

## Confirmed working (walk-1/2, validated on real Windows)

Shell detection (`config::resolve_pty_shell`), no console-window flashes
(`CREATE_NO_WINDOW` via `platform_spawn::NoWindow`), shell-aware agent boot
command (PowerShell `;`+`&`, cmd `cd /d`+`&&`, `\r` submit — no `&&` error),
session persistence across restart, Windows log file at
`%LOCALAPPDATA%\nergal\nergal.log`, cmd.exe as shell.

## Walk-3 findings + status

### Group 1 — hook-driven panels dead (status bar Claude data, sidebar dots, idle/working, plan panel, askuser tint)

**Root cause (confirmed)**: two compounding gaps.
- **Gap B (Windows port bug — FIX SHIPPED in `382f949`)**: hook commands were
  bare `nergal hook send …`, which needs `nergal.exe` on PATH. The NSIS installer
  adds **no PATH entry** (`where.exe nergal` → not found). Contrast in the log:
  the MCP shim works because it pins the absolute exe path (`registration.rs:37`);
  hooks failed because they didn't. Fix: `setup.rs` now writes
  `"<abs>\nergal.exe" hook send …` on Windows (mirrors the MCP shim); the matcher
  recognizes the new form for idempotent cleanup. Unix unchanged (bare, on PATH).
- **Gap A (universal — BUG-01 / setup-wizard, PARKED)**: nothing auto-registers
  the hooks on a fresh install. For the walk, register manually:
  `$nergal = (Get-Process nergal | Select -First 1).Path; & $nergal hook setup`
  then **restart the claude session** (hooks load at session start).

**PENDING**: after Gap-B build + manual `hook setup`, retest the 5 panels. If they
light up → windows-ipc named-pipe transport is VALIDATED (closes task 9.5).

### Group 2 — MCP `nergal` (cross-session reply)

Send WORKS (agent A called `send_to_session`). Agent B received the message but
**lacked the `send_to_session` tool** → it was launched/resumed BEFORE the MCP got
registered (`.claude.json` write happens ~22s into startup; MCP servers load at
session start). **Not necessarily a bug** — retest with TWO **fresh** sessions
started after registration. If B still lacks the tool with a fresh session, then
it's a real bug.

### Group 3 — ports filter (windows-proc)

The noise-filter (`platform_proc::keep_owner`) is wrong **both ways**:
- Port 8421 (agent's `HttpListener`) is owned by **PID 4 (System)** via http.sys
  → the `pid <= 4` guard hides the user's real dev port (false negative).
- Ports 5040/5939 (svchost services) still show → likely `listeners` returns an
  **empty/unresolvable path** for them, and `keep_owner` keeps empty-path owners
  (false positive).

**DIAGNOSTIC SHIPPED (`382f949`)**: `platform_proc::diagnose_listeners()` logs
`port-diag: port=… pid=… name=… path=… kept=…` on every ports-change. **Do NOT
guess the filter a third time** — capture real data first:
1. Reproduce (raise 8421), read the `port-diag:` lines from `nergal.log`.
2. Cross-check with PowerShell ground truth:
   ```powershell
   Get-NetTCPConnection -State Listen | ? {$_.LocalPort -ge 1024 -and $_.LocalPort -le 32767} |
     % { $p=Get-Process -Id $_.OwningProcess -EA SilentlyContinue;
         [PSCustomObject]@{Port=$_.LocalPort;PID=$_.OwningProcess;Name=$p.ProcessName;Path=$p.Path} } |
     Sort Port | Format-Table -Auto
   ```
Then write the filter against the real `(pid, path)` shape. Remove the temporary
`diagnose_listeners()` + its `browser.rs` call site once the filter is fixed.

### Group 4 — terminal alt-screen scroll + reflow (BUG-02, NEW, deferred)

With claude (alt-screen TUI) inside PowerShell inside Nergal: (a) scroll works in
one session, not in an identical one (stale alt-screen / mouse-mode flag?);
(b) TUI text garbles/overlaps when the canvas width changes (repro: "after closing
the right panel" = resize). Linux/WebKitGTK never showed it; Windows uses
**WebView2**. Suspect: alt-screen handling in the wheel-handler
(`terminalService.ts`) + grid reflow on resize. `resize_pty` (`pty.rs:479`) syncs
PTY+emulator+`differ.invalidate()`, so look at the canvas render / wheel routing.
**Deferred follow-up — do not fix blind.** Needs repro: display scaling
(100/125/150%), whether garble is resize-only, whether re-resizing fixes it.

## Verification

Full check (run after changes): `cd src-tauri && cargo clippy -- -D warnings &&
cargo test && cargo fmt --check && cd .. && npx tsc --noEmit`. On Windows, also
`cargo check --target x86_64-pc-windows-msvc` (now local).

`382f949` is Linux-green (clippy + fmt + 708 tests). The `#[cfg(windows)]` branches
need a Windows `cargo check` — **do that first** on the cloned checkout.

## Suggested next-step order

1. `cargo check --target x86_64-pc-windows-msvc` + `cargo test` on the clone
   (validates `382f949` Windows branches + windows-ipc task 9.2).
2. `pnpm tauri dev`, run `nergal hook setup`, restart a claude session → retest
   the 5 hook panels (Group 1). Green = windows-ipc validated.
3. Retest MCP reply with 2 fresh sessions (Group 2).
4. Capture `port-diag` + PowerShell port data → fix `keep_owner` → remove the
   temporary diagnostic (Group 3).
5. Repro + investigate BUG-02 (Group 4).
6. When the walk is green: archive the 5 `windows-*` changes via `/openspec-sync`
   (reconcile windows-ipc tasks 9.2/9.5). THEN implement setup-wizard. THEN
   unrelated bugs. THEN (last) the 3-platform release.
