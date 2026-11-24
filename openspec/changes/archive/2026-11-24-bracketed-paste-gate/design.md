## Context

The user paste and the agent deliveries share one encoder that always brackets (`src-tauri/src/pty.rs:1995-2003`). wezterm brackets only under DECSET 2004 and canonicalizes newlines to `\r` on Unix otherwise (`term/src/terminalstate/mod.rs:820-843`, `term/src/config.rs:100-125`).

## Goals / Non-Goals

**Goals:** a user paste into a program without 2004 shows no markers and behaves like typed input.

**Non-Goals:** changing agent deliveries; a newline setting like wezterm's `canonicalize_pasted_newlines`.

## Decisions

### D1. Gate the user paste on the terminal's own mode
`terminal_paste` reads `bracketed_paste_enabled()` from the session's wezterm terminal under the same session lock it already takes for the aux line tracker. Alternative: call wezterm's `send_paste`. Rejected: Nergal writes to the PTY writer directly, keeps its own stricter marker stripping (U+009B forms, repeated removal, `pty.rs:2008-2017`), and would lose it.

### D2. Plain paste: strip markers, newlines as CR
Without 2004 every newline becomes `\r` and then the text is de-fanged with the same stricter stripping (a literal `ESC[201~` in a plain paste is harmless but useless); that order matches wezterm (`terminalstate/mod.rs:831-832`). The newline is the Enter key's byte (`nano` treats `\n` as justify; wezterm's Unix default). Alternative: send the text untouched. Rejected: xterm and wezterm both convert, and a `\n` reaches line-mode programs differently from Enter.

### D3. Agent deliveries keep the markers
The PR viewer's "Apply with Claude" (`src/components/git/PrViewer.tsx:491-501`) is an agent delivery too, but it called `terminal_paste`; it moves to a new `terminal_deliver` command backed by `paste_to_session`. `paste_to_session` can run before Claude Code enables 2004 (a launch-time delivery). Without markers a multi-line body would submit line by line as separate turns. Claude Code handles the bracketed form, so the delivery stays bracketed. Alternative: gate it too. Rejected for that reason.

## Risks / Trade-offs

- [A plain paste into an aux shell runs its lines, but the aux line tracker sees no key event] → the commands run by the paste are not reported (true before too, when readline ran the lines despite the literal markers); `terminal_paste` sets the tracker's `start_col = None` and `prefill_chars = 0` after a successful plain paste that contains a newline (as the Ctrl+C/U branch does, `pty.rs:1699`), so the next real Enter does not reuse the first pasted line's column. Remaining gap: a trailing fragment after the last pasted newline (`b` in `a\nb`) is unanchored, so a later Enter reports the command without it.
- [Plain-mode marker stripping is cosmetic, not a security boundary] → without 2004 every pasted newline runs (`safe\nrm -rf ~` executes in `sh`), as in xterm and wezterm; only the bracketed path's stripping protects anything.

- [A program enables 2004 a moment after the paste] → the paste goes out plain; the next paste brackets. Same as wezterm.
- [Mode read and write are not atomic] → the session lock is released before the writer lock, so a mode switch between them can mismatch one paste. Accepted: it needs the app to toggle 2004 within microseconds of a paste.

## Revision 1: review follow-ups (2026-11-23, user: fix every finding)

- The user's bracketed paste gets its own marker-stripping tests (embedded end marker, rebuilt marker, U+009B, color sequences), so dropping `strip_paste_markers` from that branch fails a test.
- A poisoned session lock no longer downgrades a paste to plain: `terminal_paste` reads the mode through `PoisonError::into_inner`. Release builds abort on panic (`src-tauri/Cargo.toml:167`), so this only matters in dev builds.
- The plain paste sends `\r` on every platform. wezterm sends CRLF on Windows (`term/src/config.rs:113-114`); Windows is frozen (focus freeze 2026-10-04), so the single rule is deliberate and revisited if Windows work resumes.
