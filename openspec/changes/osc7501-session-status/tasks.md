## 1. Scanner and report parser

- [ ] 1.1 Test (Red): `src-tauri/src/terminal/program_status.rs` tests — every scenario of `program-status-protocol` "Status reports are parsed into one root status per session" (working, blocked with kind and decoded msg `"Allow Bash?"` from `QWxsb3cgQmFzaD8=`, child report ignored, `state=clear` removes, `state=clear:id=task-1` ignored, unknown state ignored, invalid msg dropped, report split across two `feed` calls, 5000-byte body discarded and the following `state=done` kept); BEL and `ESC \` terminators; `ESC ] 7501 ; ?` yields `Query` also when split as `ESC ] 75` + `01 ; ? BEL`; OSC 8 and OSC 11 sequences around a report yield nothing extra. Lock the tests.
- [ ] 1.2 Builder: `terminal/program_status.rs` (`ProgramStatusScanner`, `Osc7501`, `parse_report`, `ReportAction`, `ProgramStatus`, `RootState`, `MAX_BODY = 4096`), declared in `terminal/mod.rs`.

## 2. Emulator reply and consumption

- [ ] 2.1 Test (Red): `terminal/session.rs` tests with the captured writer — the four scenarios of `terminal-wezterm` "The emulator answers the OSC 7501 query" (reply bytes exactly `ESC ] 7501 ; ? ESC \`; reply then DA1 `ESC [ ? 65 ; 4 ; 6 ; 18 ; 22 c` for one chunk with both queries; split query answered once; a report writes nothing) and "OSC 7501 sequences are never drawn" (`ok` at column 0); `advance_bytes` returns the report bodies. Lock the tests.
- [ ] 2.2 Builder: `TerminalSession` reply writer and scanner; `advance_bytes` returns the bodies; `build_terminal_session` (`pty.rs:144`) passes the second `SharedWriterAdapter`; every `advance_bytes` caller still compiles.

## 3. Root status store and MCP mode

- [ ] 3.1 Test (Red): `agents/state.rs` tests — `apply_program_status` returns a change only when the value differs (set, same value again → no change, clear → `Some(None)`, clear when empty → no change); `clear_program_status`; `combined_mode` for every scenario of `session-directory` "Descriptor mode follows the reported root status" (working after Stop → `running`; blocked without kind/msg → `needs_attention` + `dialog`; blocked with msg → msg as waiting_for, a 300-char msg cut to 200; `kind` `auth` → `login`; error → `idle`; hook `needs_attention` with `background agent` wait wins over `done`; no root → hook mode). `pty.rs` test for the pure event helper: a changed status yields one payload `{session_id, state, kind, msg}`, an unchanged one yields none, removal yields `state: null`, `agent_session = false` yields none. Lock the tests.
- [ ] 3.2 Builder: `AgentRuntimeState.program_status` and methods; `combined_mode`; `mcp/directory.rs:245` uses it in the live branch; reader thread applies reports and emits `session:program-status` outside the session mutex; `on_agent_pty_eof` path clears and emits `state: null`.

## 4. Indicator

- [ ] 4.1 Test (Red): `src/lib/sessionIndicatorState.test.ts` — every scenario of the MODIFIED `session-state-indicators` requirement (no-root idle/thinking/working, hook attention over everything, blocked root → attention, working root after Stop → working, working root with mode `active` → thinking, done root with mode `Read` → done, error root → error, idle root with mode `active` → idle, removed root with mode `Bash` → working, completed → completed); `src/stores/programStatus.test.ts` — `applyProgramStatus` sets, replaces, and removes on `state: null`. Lock the tests.
- [ ] 4.2 Builder: `src/stores/programStatus.ts`; listener in `src/stores/hooks.ts`; clear on `session_end`; `src/lib/sessionIndicatorState.ts`; `SessionIndicator.tsx` uses it and renders `done` (green ring) and `error` (red dot).

## 5. Verification

- [ ] 5.1 `cd src-tauri && cargo clippy --all-targets -- -D warnings && cargo test`, `rustfmt --edition 2024 --check` on the changed `.rs` files, `npx tsc --noEmit`, `pnpm test`.
- [ ] 5.2 `~/.claude/skills/work/scripts/work-gates.sh main osc7501-session-status` passes.
- [ ] 5.3 Manual walk: with the new deb installed and Nergal restarted, a new Claude Code session logs `probe: OSC 7501 reply` in `claude --debug` output (or `~/.claude/debug/<session>.txt`), and no OSC 7501 text appears in the terminal.
- [ ] 5.4 Manual walk: a prompt shows working while Claude Code runs; a permission prompt shows attention; finishing the task shows the green ring (done) in the tab, sidebar row and collapsed sidebar.
- [ ] 5.5 Manual walk: a prompt that starts a background command (`run_in_background`) keeps the session working after the turn ends, and the MCP `list_sessions` reports it `running`.
- [ ] 5.6 Manual walk: opening Claude Code in a new, untrusted folder shows attention while the folder-trust prompt is open.
- [ ] 5.7 Manual walk: interrupting a turn with Esc shows idle; a failed turn (for example a request the API rejects) shows the red dot (error) when it can be triggered.
- [ ] 5.8 Manual walk: a quake shell running `printf '\033]7501;state=working\033\\'` shows no text and changes no session indicator. (The fallback for Claude Code without OSC 7501 is covered by the no-root tests of 4.1.)
