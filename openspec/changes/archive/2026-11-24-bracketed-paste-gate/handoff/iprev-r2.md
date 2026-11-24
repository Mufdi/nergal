## Re-review: bracketed-paste-gate (FRI-507), round 2

Every finding from round 1 is fixed. I re-checked the fixes against `src-tauri/src/pty.rs:1946-2090`, `src/components/git/PrViewer.tsx:479-501`, `src-tauri/src/lib.rs:532`, `src-tauri/build.rs`, and the current `openspec/specs/terminal-wezterm/spec.md:90-104,226-239`.

**Round-1 findings, confirmed fixed**
- **#1 PrViewer delivery:** fixed. `owningSessionId` (`PrViewer.tsx:479`) is an agent session, so `paste_to_session(.., false)` sends the same bytes `terminal_paste` sent before. Its `::` rejection (`pty.rs:2031`) does not affect this path. `build.rs` is a plain `tauri_build::build()` with no command manifest, so registering the command in `lib.rs` is all `terminal_deliver` needs.
- **#2 mode-to-encoding link:** fixed. `user_paste_payload(&TerminalSession, text)` tested with and without `ESC[?2004h` covers it.
- **#3 marker-stripping requirement:** fixed. The MODIFIED requirement now covers both encoders. I traced the plain scenario `a` U+009B `201~b ESC[20 ESC[201~ 1~c`: no newlines to replace, the first pass gives `ab ESC[201~ c`, the second pass gives `abc`. That matches the spec.
- **#4 order:** fixed. Newlines are replaced first, then markers removed, as wezterm does. No `\n` can survive the replacement, so removing a marker cannot rebuild a CRLF. `\r` + `ESC[201~` + `\n` now gives `\r\r`, the same as wezterm. The `\r\r\n` test case is present.
- **#6 lock scope:** fixed. One session-lock scope, dropped before the writer lock. The lock order matches today's aux branch.

**Remaining issues**

1. **Low: the aux tracker reset has no stated value and no test.** Design Risks and implementation step 3 say `terminal_paste` "resets the tracker's anchor" after a plain paste that contains a newline. Neither says what it is reset to.
   - The only correct value is `start_col = None` plus `prefill_chars = 0`, applied after the write succeeds. The column of the next prompt is not known when the paste is written. The Ctrl+C/U branch (`pty.rs:1699`) already resets this way.
   - An implementer could instead re-anchor to the current `cursor_col`. That would repeat the stale-anchor bug this step is meant to fix.
   - Task 1.1 has no test for this, even though the plan adds the behaviour.
   - There is also a remaining gap that Risks should name. With `a\nb`, the trailing fragment `b` sits unanchored, so a later Enter at `pty.rs:1707-1710` takes the cursor column and drops `b` from the reported command.

   Fix: write "set `start_col = None`, `prefill_chars = 0`" in step 3. Add a tracker assertion to 1.1, or extract the reset into a pure helper and test that.

2. **Low: the scenarios kept in "A paste body cannot end the bracketed paste" have no precondition.** "Embedded end marker", "Marker rebuilt by a first removal" and "Color sequences survive" (`spec.md:229-239`, copied unchanged into the delta) still expect `ESC[200~…ESC[201~` on the PTY. Under the new gate that only holds when 2004 is on, or on the delivery path. Read literally, they contradict "Plain paste without DECSET 2004".

   Fix: add `GIVEN the app enabled bracketed paste (or the body is an agent delivery)` to those three scenarios, so each scenario has one outcome.

3. **Low: plain mode is cosmetic, not a security boundary, and the plan does not say so.** In plain mode, removing markers no longer protects anything: every pasted newline becomes `\r` and runs, so `safe\nrm -rf ~` executes in `sh` without 2004. That is standard xterm/wezterm behaviour, and it was already true for readline with 2004 off. D2 is honest about it ("harmless but useless"). The requirement title still reads as a guarantee, though. Fix: add one line to Risks so nobody later treats the plain-path stripping as protection.

**Unchanged and still correct:** D3 (keeping agent deliveries bracketed) is right. Concurrency is fine: the TOCTOU is negligible and admitted in Risks. Rollback is a single revert, with no config, schema or IPC contract change; `terminal_deliver` is additive.

VERDICT: APPROVED
