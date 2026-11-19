## 1. Theme colors in OSC 10/11 (FRI-504)

- [x] 1.1 Test (Red): `session.rs` tests on sessions built with `NergalTerminalConfig::with_colors(private cell)`: after `set_colors(fg #18181b, bg #ffffff)`, `OSC 11 ; ?` answers `rgb:ffff/ffff/ffff` and `OSC 10 ; ?` answers `rgb:1818/1818/1b1b`; a later `set_colors` with bg `#09090b` changes the same session's answer; an app-set `OSC 11` keeps precedence; default cells snapshot with `fg`/`bg` `None`. `src/components/terminal/cssColor.test.ts`: `#fff`, `#FFFFFF`, `rgb(255, 255, 255)`, `rgba(9, 9, 11, 1)` parse; `oklch(…)`, `""` and `#ggg` return `null`.
- [x] 1.2 Add `ColorCell`, `shared_colors`, `set_colors` and `with_colors` in `config.rs`, the `set_terminal_default_colors` command, and its registration.
- [x] 1.3 Add `cssColorToRgb`; after each `refreshTermTheme()` in `terminalService.ts` (module load and theme flip), send `TERM_THEME.foreground`/`background` when both parse, and log a rejected `invoke` (fire-and-forget with `.catch`).

## 2. Synchronized output (FRI-505)

- [x] 2.1 Test (Red): `session.rs`: `sync_output_since` is `Some` after `CSI ? 2026 h`, `None` after `l` and after `ESC c`, `Some` for the sequence split at each of its 7 inner positions, `None` after `ESC c` split as `ESC` + `c`, and unchanged by a repeated `h`. `emitter.rs`: with mode on, `AAA` then 20 ms later `BBB` + `l` → no update has `AAA` without `BBB`; `l` flushes in < 100 ms; unclosed mode → an update between 140 and 600 ms; mode off → update in < 200 ms; `h`+`AAA`+`BBB`+`l` inside one coalescing window → update in < 100 ms. Bounds are measured from the relevant `advance_bytes` call.
- [x] 2.2 Add the matcher in `TerminalSession::advance_bytes`, `SyncState`, `sync_output_since` and `sync_state`.
- [x] 2.3 In `run_emitter`, after the coalescing sleep, wait on `notify` with a timeout of the time left while the shared `SyncState` is younger than `SYNC_OUTPUT_CAP`, re-checking `shutdown` on each wake. Order: read the state, wait only if it is `Some` and younger than the cap, re-read after every wake (an `l` that landed during the coalescing sleep must not cost a wait). Read `SyncState` and `ColorCell` with `.lock().ok()` / `.read().ok()`; no `unwrap` outside tests.

## 3. Verification

- [x] 3.1 `cd src-tauri && cargo clippy --all-targets -- -D warnings && cargo test`, `rustfmt --edition 2024 --check <changed .rs files>`, `npx tsc --noEmit && pnpm test`, `~/.claude/skills/work/scripts/work-gates.sh main terminal-cc-compat`.
- [x] 3.2 Manual walk: with the light theme, `printf '\e]11;?\e\\'; cat -v` in a session prints `rgb:ffff/ffff/ffff`; after switching to the dark theme, the same session prints the dark background.
- [x] 3.3 Manual walk: in a Claude Code session with a long streaming answer, resize the window; no half-drawn frames appear.
