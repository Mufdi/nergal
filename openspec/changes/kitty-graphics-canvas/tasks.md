## 1. Kitty filter

- [ ] 1.1 Test (Red): `src-tauri/src/terminal/kitty_filter_tests.rs` — every scenario of "Only safe kitty graphics sequences reach the emulator" at the filter level (direct image passes unchanged; `t=f`, `t=t`, `t=s` dropped; `a=q` dropped; `o=z`, `a=f`, `a=c`, `U=1` dropped; `w=0,c=1` dropped; a sequence over 64 KiB dropped and the following `ok` kept; a transfer over 128 MiB cut with one `ESC _ G m=0 ESC \` emitted and later chunks of it dropped; split sequence filtered whole; `ESC _ X hello ESC \` passes) plus BEL terminator, CAN abort inside a kitty sequence (held bytes dropped, CAN kept), a trailing `ESC` at the end of a read, a key without `=` dropped, a dropped first chunk drops its continuation chunks. Lock the tests.
- [ ] 1.2 Builder: `terminal/kitty_filter.rs` (`KittyFilter`, `MAX_SEQUENCE`, `MAX_TRANSFER`).

## 2. Emulator wiring

- [ ] 2.1 Test (Red): new `#[path]` test file for `session.rs` — through `advance_bytes`: a `t=f` and a `t=t` sequence pointing at a real temp file leave it present and unchanged and write nothing to the reply writer; `a=q` writes nothing to the PTY; `w=0,c=1` does not panic; the 2x2 RGBA image yields one z=0 run on the cursor row; `i=1,p=7` placed then re-placed with `a=p` moves its runs; `a=d,d=i,i=1` removes them; a chunked transfer (`m=1`, `m=1`, `m=0`) yields runs; a cut oversized transfer leaves no runs and a following small transfer still yields runs. Lock the tests.
- [ ] 2.2 Builder: `advance_bytes` feeds the filtered bytes to every consumer; `enable_kitty_graphics` true.

## 3. Image layers

- [ ] 3.1 Test (Red): `src/components/terminal/imageLayers.test.ts` — the three scenarios of "Images are drawn in kitty's three layers" plus an empty row. Lock the tests.
- [ ] 3.2 Builder: `imageLayers.ts`; `paintRow` draws under-background, backgrounds, under-text, glyphs, over-text.

## 4. Verification

- [ ] 4.1 `cd src-tauri && cargo clippy --all-targets -- -D warnings && cargo test && cargo fmt --check`, `npx tsc --noEmit`, `pnpm test`.
- [ ] 4.2 `~/.claude/skills/work/scripts/work-gates.sh main kitty-graphics-canvas` passes.
- [ ] 4.3 Manual walk: in a Nergal session terminal, `chafa -f kitty <image>` draws the image; scrolling and new output keep it in place; text printed over part of it replaces only those cells.
- [ ] 4.4 Manual walk: the same in the quake terminal.
- [ ] 4.5 Manual walk: sixel (`chafa -f sixels`) and iTerm2 (`chafa -f iterm`) images still draw as before.
- [ ] 4.6 Manual walk: `printf` of a kitty sequence with `t=f` and the base64 of an existing file path shows nothing, and the file is untouched; the app keeps running after `printf` of a `w=0,c=1` placement.
- [ ] 4.7 Manual walk: a Claude Code session still logs `kittyGraphics=no` in `claude --debug` (the query stays unanswered until FRI-624).
