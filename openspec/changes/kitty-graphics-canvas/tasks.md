## 1. Kitty gate

- [ ] 1.1 Test (Red): `src-tauri/src/terminal/kitty_gate_tests.rs`, building actions with termwiz's `Parser::parse_as_vec` — every scenario of "Only safe kitty graphics actions reach the emulator" (direct RGBA forwarded with Quiet verbosity; `t=f`/`t=t`/`t=s` as transmissions, queries and continuation chunks dropped; `a=q`, `I=5` dropped and `q=0` forwarded as Quiet; `o=z`, `a=f`, `a=c`, `f=100` dropped; degenerate placements `w=0`, `x=5`, `y=5`, `w=00`, `c=4000000000`, missing `c`, missing `r`, `a=p,i=0` dropped; size mismatch dropped; three-chunk transfer with padding in a middle chunk forwarded as one `DirectBin` action; oversized, interrupted and non-direct-continuation transfers dropped whole; unknown image 99 dropped; unimplemented delete variants dropped); every scenario of "Kitty memory stays under a lifetime cap per terminal" at the gate level with injected small `GateLimits` (cells cap stops placements, bytes cap stops retransmissions to the same id and to new ids, a capped gate drops later kitty actions, a second gate is independent); bypass attempts a byte filter would miss (BEL inside the keys, a nested `ESC _ G`, a C1 `0x9c` terminator, ignored bytes before `_` and inside the APC, duplicate `t=d,t=f`); a seeded fuzz loop (no new dependency) over random kitty keys and byte noise asserting every forwarded kitty action satisfies the allowlist and the forwarded totals never pass the caps. Lock the tests.
- [ ] 1.2 Builder: `terminal/kitty_gate.rs` (`KittyGate`, `GateLimits`).

## 2. Parser ownership and emulator wiring

- [ ] 2.1 Test (Red): new `#[path]` test file for `session.rs` — "Nergal parses the PTY output and applies the actions" (a mixed corpus of text, SGR, OSC title, sixel, iTerm2 image and a split CSI gives the same grid and title as a session using the emulator's own parser); through `advance_bytes`: `t=f` and `t=t` naming a real temp file leave it present and unchanged and write nothing to the reply writer; `a=q` writes nothing; degenerate placements do not panic; every scenario of "Kitty placements are drawn on the canvas" (one z=0 run for the 2x2 image; `p=7` move; `a=d` delete; text over a placement); the lifetime-cap oracle on wezterm itself: after repeated placements and retransmissions mixed with `ESC [?47h`/`ESC [?47l`, `ESC [?1049h`/`l`, IL/DL, DECSTBM with LF and a resize, the image cells held by the emulator in both screens and the scrollback stay at most the cell cap and the distinct image bytes stay at most the byte cap (small injected limits). Lock the tests.
- [ ] 2.2 Builder: the session owns a termwiz `Parser`; `advance_bytes` parses, gates and calls `perform_actions`; `enable_kitty_graphics` true; `src-tauri/clippy.toml` disallows `Terminal::advance_bytes`.

## 3. Image layers

- [ ] 3.1 Test (Red): `src/components/terminal/imageLayers.test.ts` — the three scenarios of "Images are drawn in kitty's three layers" (including the exact `-1073741824` and `-1073741825` borders) plus an empty row. Lock the tests.
- [ ] 3.2 Builder: `imageLayers.ts`; `paintRow` draws under-background, backgrounds, under-text, glyphs, over-text.

## 4. Verification

- [ ] 4.1 `cd src-tauri && cargo clippy --all-targets -- -D warnings && cargo test && cargo fmt --check`, `npx tsc --noEmit`, `pnpm test`.
- [ ] 4.2 `~/.claude/skills/work/scripts/work-gates.sh main kitty-graphics-canvas` passes.
- [ ] 4.3 Manual walk: in a Nergal session terminal, `chafa -f kitty <image>` draws the image; scrolling and new output keep it in place; text printed over part of it replaces only those cells.
- [ ] 4.4 Manual walk: the same in the quake terminal.
- [ ] 4.5 Manual walk: sixel (`chafa -f sixels`) and iTerm2 (`chafa -f iterm`) images, yazi previews, colors, titles and Claude Code's TUI still behave as before.
- [ ] 4.6 Manual walk: `printf` of a kitty sequence with `t=f` and the base64 of an existing file path shows nothing and the file is untouched; the app keeps running after `printf` of a `w=0,c=1` placement.
- [ ] 4.7 Manual walk: a Claude Code session still logs `kittyGraphics=no` in `claude --debug` (the query stays unanswered until FRI-624).
