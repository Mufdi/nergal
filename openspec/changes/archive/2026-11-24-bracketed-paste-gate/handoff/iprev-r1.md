**Review: bracketed-paste-gate (FRI-507)**

I checked the plan against `src-tauri/src/pty.rs:1946-2045`, every caller of `terminal_paste`, `paste_to_session` and `write_bracketed`, and wezterm's `send_paste` and `NewlineCanon` in the pinned checkout.

1. **High: a missed caller of `terminal_paste` sends agent deliveries down the user-paste path.** The proposal (Impact) says "No frontend change", and implementation.md lists only `terminal_paste` and `paste_to_session` as encoder consumers. But `src/components/git/PrViewer.tsx:501` ("Apply with Claude") calls `invoke("terminal_paste", { sessionId: sid, text: prompt })` with a multi-line prompt built by `.join("\n")` (`:491-499`) for an agent session. That is an agent delivery in every sense D3 means. After this change, if the session has not enabled 2004, the prompt goes out plain with each `\n` turned into `\r`. Claude Code would then submit "Apply the following review feedback to PR #N…" as a turn of its own and split the rest into more turns. That is exactly the failure D3 uses to keep `paste_to_session` bracketed. The plan's own reasoning puts this caller on the always-bracketed side, but the plan neither moves it nor mentions it. The spec delta's "Agent deliveries stay bracketed" scenario also names only `paste_to_session`, so the spec stays silent on this path. Fix: route PrViewer through a command backed by `paste_to_session` (or `paste_and_submit_settled`), or add an explicit "delivery" flag. Then add a spec scenario and a walk step (task 2.4) for "Apply with Claude".

2. **Medium: nothing tests that `terminal_paste` actually uses the mode.** Task 1.1 tests the pure encoder and the `TerminalSession` accessor separately. The new behaviour is the link between them (read the mode under the session lock, then pick the encoding), and no test covers it. A bug like inverting the bool or reading the wrong instance would pass every listed test. `pty_pixel_tests.rs:65-75` already builds a `PtyManager` and reaches `instances[..].terminal.session`. Two options:
   - Extract `fn user_paste_bytes(instance: &PtyInstance, text) -> Vec<u8>`, or a helper taking `&PtyManager, session_id`.
   - Assert the bytes written for a session fed `ESC[?2004h` versus a fresh one, using a capture writer.

   Without this, criterion 4 (a deterministic test per scenario) is not met for the core scenario.

3. **Medium: the existing marker-stripping requirement is left inconsistent.** `openspec/specs/terminal-wezterm/spec.md:226-227` ("A paste body cannot end the bracketed paste") says the encoder removes markers "before wrapping it". The plain path does not wrap, and the delta only touches "Selection and clipboard". Its new sentence ("embedded bracketed-paste markers removed") does not say whether the plain path also removes the U+009B forms and repeats until stable. D1 says Nergal keeps its stricter stripping and D2 says the text is still de-fanged, so the spec should say so. Either MODIFY the requirement at :226 to cover both encodings, or add a plain-mode scenario with U+009B and a rebuilt marker. Task 1.1 should also test `encode_user_paste("a\u{9b}201~b\x1b[20\x1b[201~1~c", false)`.

4. **Low: plain mode confirmed equivalent to wezterm on Unix, with one ordering difference.**
   - `NewlineCanon::CarriageReturn.canonicalize` (`term/src/config.rs:25-40`) maps `\n` to `\r`, and `\r` to `\r` while consuming a following `\n`. Replacing `\r\n` before lone `\n` gives the same result, including `\r\r\n` → `\r\r`.
   - Inside brackets wezterm uses `NewlineCanon::None` (`terminalstate/mod.rs:825-829`), which matches the plan keeping today's bytes and the "Bracketed paste" scenario's LF.
   - The plan strips markers then canonicalizes; wezterm canonicalizes then strips (`:831-832`). They differ only on input like `\r` + `ESC[201~` + `\n`: the plan emits one `\r`, wezterm emits two. This is harmless, but the "as wezterm does" claim in the proposal and D2 is not literally true. Either canonicalize first to match, or note the difference.
   - Add a `\r\r\n` case and a lone-`\r` case to task 1.1. The lone-`\r` case is listed; `\r\r\n` is not.

5. **Low: in plain mode, a pasted `\r` acts as Enter but the aux line tracker never sees it.** In an aux shell without 2004 (dash, `sh`, a REPL, bash with bracketed paste off), the pasted text now runs line by line. `terminal_paste` anchors `t.start_col` (`pty.rs:1978-1983`) but never resets it, and the Enter handling in the tracker (`pty.rs:1704-1712`) runs only on key events. Two effects:
   - The commands run by the paste are not reported.
   - The next real Enter uses a stale anchor from the first pasted line.

   Before this change the same shells got literal markers, and readline still ran the lines, so the tracker gap is not entirely new. Still, plain mode makes this path the expected one. At minimum, document it in Risks. Better: reset `start_col` when a plain-mode paste contains a newline.

6. **Low: concurrency is acceptable, but one detail should be pinned.** The resulting order is instances → session → (drop) → writer, with instances held throughout. Elsewhere the order is instances → session (`pty.rs:901-903`, resize) and writer → (drop) → session (`:2159-2167`). Since the plan drops the session lock before taking the writer, there is no inversion. Two points:
   - Agent sessions now take the session lock on every paste, where today only aux shells do. Implementation should read the mode and `cursor_col` in a single lock scope rather than two acquisitions.
   - The TOCTOU described in Risks is real but negligible: the reader thread advances immediately (`pty.rs:824`), so the mode does not lag output.

7. **Low: D3 itself is right.** Keeping `paste_to_session` (`linear/mod.rs:474`, `clickup/mod.rs:852`, `mcp/delivery.rs:94`, `pty.rs:2085`) always bracketed is correct. These are machine deliveries to Claude Code, which handles the bracketed form, and their submit logic (`paste_and_submit_settled`, the 250 ms settle) assumes that paste-mode exit. Gating them would turn a delivery that arrives during launch into several turns. The issue is only finding 1: one delivery bypasses this function.

8. **Low: rollback is fine.** The change is confined to `pty.rs` and `session.rs`, with no config, IPC, schema or data change, so a single revert undoes it.

VERDICT: REVISE
