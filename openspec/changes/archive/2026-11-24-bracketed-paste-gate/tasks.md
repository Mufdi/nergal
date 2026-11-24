## 1. User paste follows DECSET 2004

- [x] 1.1 Test (Red): Rust, in a `#[path]` test file: `encode_user_paste("a\nb", true)` is `ESC[200~a\nbESC[201~`; `encode_user_paste("a\r\nb\nc\x1b[201~d", false)` is `a\rb\rcd`; a lone `\r` stays one `\r`; `\r\r\n` becomes `\r\r`; `encode_user_paste("a\u{9b}201~b\x1b[20\x1b[201~1~c", false)` is `abc`; `user_paste_payload` on a session fed `ESC[?2004h` brackets and on a fresh session does not; the tracker helper after a plain paste with a newline returns `start_col = None` and `prefill_chars = 0`, and leaves the tracker as it was after a bracketed paste or a plain paste without a newline; `TerminalSession` reports `bracketed_paste_enabled()` false initially, true after `ESC[?2004h`, false after `ESC[?2004l`; `write_bracketed` (agent path) still wraps `x\ny`.
- [x] 1.2 Add `TerminalSession::bracketed_paste_enabled()`, `encode_user_paste` and `user_paste_payload`; make `terminal_paste` use them in one session-lock scope and reset the aux tracker anchor after a plain paste with a newline.
- [x] 1.3 Add the `terminal_deliver` command (backed by `paste_to_session`, registered in `lib.rs`) and switch `PrViewer.tsx`'s "Apply with Claude" to it.

## 2. Review follow-ups (Revision 1)

- [x] 2.1 Test: user bracketed-paste marker stripping cases in `src-tauri/src/pty_user_paste_tests.rs` (regression guards for a coverage gap).
- [x] 2.2 `terminal_paste` reads the session through `PoisonError::into_inner` instead of falling back to a plain paste; rewrap the `write_bracketed` doc comment.

## 3. Verification

- [x] 3.1 `cd src-tauri && cargo clippy --all-targets -- -D warnings && cargo test && cargo fmt --check`, then `npx tsc --noEmit` and `pnpm test`.
- [x] 3.2 `~/.claude/skills/work/scripts/work-gates.sh mufdidev/fri-506-mouse-tracking-clicks bracketed-paste-gate` passes.
- [x] 3.3 Manual walk: in a quake shell run `cat`, paste two lines with Ctrl+Shift+V: no `^[[200~` shows and each line echoes as if typed; Ctrl+D ends it. In the zsh prompt (2004 on), paste two lines: they land as one paste and do not run until Enter.
- [x] 3.4 Manual walk: in a Claude Code session, paste multi-line text: it lands as one paste; a Linear "send to session" of a multi-line issue still arrives as one turn; in the PR viewer, "Apply with Claude" with two annotations lands as one paste in Claude Code.
