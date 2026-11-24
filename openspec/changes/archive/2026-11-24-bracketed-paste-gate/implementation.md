# Implementation Plan: bracketed-paste-gate

> Grounded in current codebase, symbols verified 2026-11-23.

## Verified codebase facts (do not re-assume)

- `terminal_paste` (`src-tauri/src/pty.rs:1953-1990`): resolves the PTY instance, locks `instance.terminal.session` only for aux shells (`::` ids) to read `cursor_col()`, then writes with `write_bracketed(&mut *w, &text, false)` at `:1987`.
- `write_bracketed(w, text, submit)` (`pty.rs:1995-2003`) and `strip_paste_markers` (`:2008-2017`); `paste_to_session` (`:2025-2045`) also calls `write_bracketed`. Existing encoder tests in `mod tests` (`pty.rs:~2540-2630`).
- Frontend callers of `terminal_paste`: the canvas paste (`src/components/terminal/terminalService.ts:1038`) and the PR viewer's "Apply with Claude" (`src/components/git/PrViewer.tsx:501`, a multi-line prompt to an agent session). `paste_to_session` callers: `src-tauri/src/linear/mod.rs:474`, `src-tauri/src/clickup/mod.rs:852`, `src-tauri/src/mcp/delivery.rs:94`, `pty.rs:2085`.
- Aux line tracker: `terminal_paste` anchors `start_col` (`pty.rs:1978-1983`); the tracker's Enter handling runs only on key events (`pty.rs:1704-1712`).
- `TerminalSession` wraps a wezterm `Terminal` in `self.terminal` (`src-tauri/src/terminal/session.rs`, e.g. `is_alt_screen_active` at `:388`); wezterm exposes `bracketed_paste_enabled()` (`term/src/terminalstate/mod.rs:756-758` in the cargo git checkout).

## Execution order

1. `TerminalSession::bracketed_paste_enabled()` accessor.
2. Pure `encode_user_paste(text: &str, bracketed: bool) -> String` in `pty.rs`: bracketed → today's bytes; plain → newlines to `\r` (`\r\n` first), then `strip_paste_markers`. And `user_paste_payload(session: &TerminalSession, text: &str) -> String` = `encode_user_paste(text, session.bracketed_paste_enabled())`, the link the tests pin.
3. `terminal_paste`: one session-lock scope reads the mode (and `cursor_col` for aux shells), then drops it; writes `user_paste_payload`'s result under the writer lock; for an aux shell, after a successful plain paste containing a newline, set the tracker's `start_col = None` and `prefill_chars = 0` (never re-anchor to `cursor_col`: the next prompt's column is unknown). Put that decision in a pure helper (e.g. `tracker_after_paste(tracker, bracketed, text)`) so it is testable.
4. New `#[tauri::command] terminal_deliver(session_id, text)` → `paste_to_session(&state, &session_id, &text, false)`, registered in `src-tauri/src/lib.rs` next to `terminal_paste`; `PrViewer.tsx:501` invokes it.

## Per-phase risk

- Canonicalization order: replace `\r\n` before lone `\n`, or a CRLF becomes two CRs.
- The session lock must drop before the writer lock (same order as today's aux branch) to avoid a lock-order inversion with the emitter.

## Verification

- `cd src-tauri && cargo clippy --all-targets -- -D warnings && cargo test && cargo fmt --check`, `npx tsc --noEmit`, `pnpm test`.
- `~/.claude/skills/work/scripts/work-gates.sh mufdidev/fri-506-mouse-tracking-clicks bracketed-paste-gate`.
- Manual walk on the checkpoint-1 `.deb` (tasks group 2).
