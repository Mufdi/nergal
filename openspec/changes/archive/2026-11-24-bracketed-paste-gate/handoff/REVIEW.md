# REVIEW: bracketed-paste-gate (FRI-507)

## Adversarial

Scope: `git diff mufdidev/fri-506-mouse-tracking-clicks -- src src-tauri` plus `src-tauri/src/pty_user_paste_tests.rs`. `cargo test --lib user_paste`: 12 passed.

### Bugs

No blocking bugs found.

- Checked: newline handling matches wezterm `NewlineCanon::CarriageReturn` (`config.rs:25-45`) for `\r\n`, `\n\r`, `\r\r\n` and a lone `\r`.
- Checked: the order is newlines first, then markers (`pty.rs:2017`), the same as wezterm (`terminalstate/mod.rs:831-832`). After the replace no `\n` is left, so stripping cannot rebuild a CRLF.
- Checked: `bracketed` and `payload` come from one session guard (`pty.rs:1969-1976`). The guard drops at the end of the `let`, before the writer lock (`pty.rs:1990`).
- Checked: lock order instances, then session, then trackers matches `terminal_input` (`pty.rs:2159-2191`). No inversion.
- Checked: the aux tracker resets only after a write succeeds (`pty.rs:1995-1997`). The trailing-fragment gap is documented in design.md Risks.
- Checked callers. `terminal_paste` has one caller, the canvas Ctrl+V (`terminalService.ts:679`, `:1038`).
- Checked callers. `terminal_deliver` has one caller, PR viewer (`PrViewer.tsx:501`), with `owningSessionId`, an agent session.
- Checked callers. `paste_to_session` and `paste_and_submit_settled` serve Linear (`linear/mod.rs:452,472,474`), ClickUp (`clickup/mod.rs:828,850,852`) and MCP (`mcp/delivery.rs:94`).
- No agent delivery uses the user-paste path. No user paste uses the delivery path.
- **Nit** `pty.rs:1977`: a poisoned session lock now falls back to a plain paste, which fails open. Before, every paste was bracketed. `Cargo.toml:167` sets `panic = "abort"`, so only dev builds can reach this. Read the guard with `PoisonError::into_inner` instead.

### Security

- Bracketed user path: it reaches `strip_paste_markers` (`pty.rs:2015`), the same function the agent path uses (`pty.rs:2040`). It is airtight today.
- **Important** `pty_user_paste_tests.rs:22-24`: nothing tests that the user bracketed branch (`pty.rs:2015`) strips markers. Only `write_bracketed` has stripping tests (`pty.rs:2585-2610`).
- That makes `pty.rs:2015` a second bracketed encoder whose stripping no test covers. If someone drops `strip_paste_markers` there, all tests still pass.
- Fix: add `encode_user_paste("safe\x1b[201~rm -rf ~\r", true)`, a rebuilt-marker case and a U+009B case. Or route `pty.rs:2015` through `write_bracketed` into a `Vec`.
- Plain path: design.md Risks says marker stripping there is cosmetic, and that is correct. Without DECSET 2004 the app has no paste mode to escape. Pasted lines run, as in xterm and wezterm.
- Mode lag: if the parser lags the PTY, a paste right after `ESC[?2004h` goes out plain. wezterm has the same lag. Accepted in design.md.
- `terminal_deliver` (`pty.rs:2004-2011`) gives the webview nothing new. `write_to_session_pty` (`pty.rs:1768-1774`) already writes any bytes to any session.
- `terminal_deliver` is strictly narrower: agent sessions only (`pty.rs:2074`), markers stripped, no submit.
- **Nit** `pty.rs:2017`: the plain branch sends `\r` on every target. wezterm sends CRLF on Windows (`config.rs:113-114`). Windows is frozen, but neither the design nor a comment says the `\r` is deliberate.

### Compliance

Delta spec `specs/terminal-wezterm/spec.md`, mapped to code, tests or walks:

- Selection and clipboard / Bracketed paste: `pty.rs:1969-1975`; `bracketed_user_paste_wraps_text_in_markers`, `user_paste_payload_brackets_when_session_enabled_2004`; walks 2.3 and 2.4.
- Plain paste without DECSET 2004: `pty.rs:2017`; `plain_user_paste_turns_line_endings_into_carriage_returns`, `user_paste_payload_is_plain_on_fresh_session`, `session_reports_bracketed_paste_mode_transitions`; walk 2.3.
- Agent deliveries stay bracketed: `pty.rs:2010`, `:2087`; `agent_write_bracketed_still_wraps_multiline_text`. No test covers `terminal_deliver` itself; walk 2.4 does.
- Plain paste removes the same markers: `plain_user_paste_strips_markers_that_rejoin_after_removal`.
- Drag to select, Copy selection: not changed. The existing implementation still applies.
- **Important**: Embedded end marker, Marker rebuilt by a first removal and Color sequences survive have tests only for agent deliveries (`pty.rs:2585-2610`). Their "app enabled bracketed paste" case has no test (see Security).
- **Important** `tasks.md:9-14`: tasks 2.1-2.4 are unchecked. Gates ran green in evidence.md, but walks 2.3 and 2.4 have not run.
- Walk 2.4 is the only check that Claude Code enables DECSET 2004. If it does not, every multi-line user paste into Claude Code would now submit line by line.
- Comments follow the WHY-only rule. No `unwrap` outside tests. No doc drift: `docs/` lists no terminal commands, and the ClickUp/Linear comments (`clickup.ts:607`, `linear.ts:596`) still hold.
- **Nit** `pty.rs:2035`: the `write_bracketed` doc comment's first line is about 120 columns. The rest of the block wraps at about 80.
- **Nit** delta spec "A paste body cannot end the bracketed paste" says "Both paste encoders". The code has three: `write_bracketed`, plus the bracketed and plain branches of `encode_user_paste`.

**Verdict:** Correct and wezterm-faithful. Before archive, add the user-bracketed stripping test and run walks 2.3 and 2.4.

## Frontend

Scope: PrViewer.tsx diff vs fri-506 branch, plus terminalService.ts paste path.

### 🔴
None.

### Important
None.

### Nit
- src/components/git/PrViewer.tsx:476-500: `applyWithClaude` targets `owningSessionId`, picked from `ws.sessions` (PrViewer.tsx:265-273). These are agent session ids, never `::` aux ids. `terminal_deliver` rejects `::`, so it cannot fail for that reason.
- src/components/git/PrViewer.tsx:501-516: the success toast ("Sent N annotations to the session terminal") stays accurate. The error toast prints `String(err)`, which now can read "paste_to_session accepts agent sessions only" or "no PTY for session". Both are reachable only if the session has no live PTY. Acceptable.
- src/components/git/PrViewer.tsx:501: the delivery stays bracketed and has no submit `\r` (`submit: false`), same as the old `terminal_paste` behaviour on an agent session. The user still presses Enter. Toast wording ("Sent") remains true.
- src/components/terminal/terminalService.ts:1038: the canvas paste is the only other `terminal_paste` call in `src/`. It is a user paste, so gating on DECSET 2004 is intended. Target is `entry.sessionId`, which may be an aux shell, which `terminal_paste` still supports.
- Fallback at terminalService.ts:1041-1042 (empty clipboard sends `\x16`) is unchanged and unaffected.
- Programmatic text elsewhere: grep of `src/` finds no other `terminal_paste` or `terminal_deliver` caller. Other sends use `write_to_session_pty` and are out of scope.
- Behaviour change of Ctrl+Shift+V: the handler (terminalService.ts:676) matches any Ctrl+V. In a program without 2004 the paste now has no markers and newlines become CR. No tooltip, label or UI copy promises otherwise. `src/stores/shortcuts.ts:600` only comments on routing.
- Docs: `docs/shortcuts.md` has no paste text, so no doc change is needed. Its pending diff is an unrelated vault-path edit.
- No frontend test covers the PrViewer invoke name. A rename regression would only show at runtime. Optional.

Verdict: frontend change is correct and minimal; no blocking findings.

## Orchestrator ruling

- Every finding is fixed (user preference): Revision 1 in design.md, tasks group 2. The frontend nit (no test for the terminal_deliver invoke name) is covered by walk 3.4.
