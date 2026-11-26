# Implementation Plan: kitty-graphics-canvas

> Grounded in main `23470f5` (branch `mufdidev/fri-613-kitty-graphics`), symbols verified 2026-11-26. `WZ` = `~/.cargo/git/checkouts/wezterm-26cdb9e734a97642/5046fc2`. Analysis: `~/.claude/projects/-home-felipe-Projects-cluihud/work/kitty-graphics/analysis.md`; plan review: `handoff/iprev/`.

## Verified codebase facts (do not re-assume)

- `TerminalSession::advance_bytes(&mut self, bytes) -> Vec<Vec<u8>>` (`src-tauri/src/terminal/session.rs:254-272`): `track_sync_mode(bytes)`, OSC 7501 `status_scanner.feed(bytes)` (answers queries through `status_reply`), then `self.terminal.advance_bytes(bytes)` (`:270`).
- wezterm `Terminal::advance_bytes` = `self.parser.parse(bytes, |action| performer.perform(action))` (`WZ/term/src/terminal.rs:152-162`); `Terminal::perform_actions(Vec<Action>)` runs the same performer (`:164-173`) and is public.
- termwiz parser: `termwiz::escape::parser::Parser::parse(&mut self, bytes, FnMut(Action))` (`WZ/termwiz/src/escape/parser/mod.rs:86`), `parse_as_vec` (`:157`). `termwiz` is a direct dependency at the same tag (`src-tauri/Cargo.toml:85`).
- Kitty action: `Action::KittyImage(Box<KittyImage>)` (`WZ/termwiz/src/escape/mod.rs:47`); `KittyImage::{TransmitData{transmit, verbosity}, TransmitDataAndDisplay{transmit, placement, verbosity}, Display{image_id, image_number, placement, verbosity}, Delete{what, verbosity}, Query{transmit}, TransmitFrame{..}, ComposeFrame{..}}` (`WZ/termwiz/src/escape/apc.rs:1012-1049`). `KittyImageTransmit{format: Option<KittyImageFormat>, data: KittyImageData, width, height, image_id, image_number, compression, more_data_follows, ..}` (`:526+`); `KittyImageData::{Direct(String), DirectBin(Vec<u8>), File{..}, TemporaryFile{..}, SharedMem{..}}` (`:25+`); `KittyImageFormat::{Rgb, Rgba, Png}` (`:470-477`, `f` absent = None, which kitty treats as RGBA); `KittyImageCompression::{None, Deflate}` (`:500-504`); `KittyImageVerbosity::{Verbose, OnlyErrors, Quiet}` (`:440-444`); `KittyImagePlacement{x, y, w, h, x_offset, y_offset, columns, rows, placement_id, do_not_move_cursor, z_index}` (`:585+`); `KittyImageDelete::{All{delete}, ByImageId{image_id, placement_id, delete}, ..}` (`:661+`). Verify exact field names in the source before use.
- Kitty enable: wezterm default `enable_kitty_graphics` false (`WZ/term/src/config.rs:175`), checked at `WZ/term/src/terminalstate/kitty.rs:176`; `NergalTerminalConfig` impl at `src-tauri/src/terminal/config.rs:70`.
- Replacement rule: `kitty_img_place` removes the (id, p) placement, or all placements of the id when `p` is absent, before placing (`kitty.rs:103-105`); delete by id `kitty.rs:241-272`.
- Degenerate placement division at `WZ/term/src/terminalstate/image.rs:81-114` (width/height 0 from `w=0` or `x >= W`, same for `y`/`h`/`r`); release aborts on panic (`src-tauri/Cargo.toml:169`).
- Snapshot/runs/differ already handle several images per cell and z (`session.rs:530-684`, `differ.rs:103-123`); `Rgba8` served as kind 0 (`image_fetch.rs:120-125`).
- Canvas `paintRow` (`src/components/terminal/terminalService.ts:1435-1487`) and `drawImageRuns` (`:1501`); pure helpers in `imagePlacement.ts` with tests.

## Execution order

1. `terminal/kitty_gate.rs` (pure, with tests).
2. `TerminalSession` owns a termwiz `Parser`; `advance_bytes` parses → gate → `perform_actions`; `enable_kitty_graphics` true.
3. `src/components/terminal/imageLayers.ts` and `paintRow`.

## Plan

### 1. `KittyGate` (`src-tauri/src/terminal/kitty_gate.rs`, declared in `terminal/mod.rs`)

- `pub struct GateLimits { pub max_side: u32, pub max_bytes: u64, pub max_cells: u64, pub max_transmissions: u32, pub max_placements: u32, pub max_deletes: u32 }` with `Default` = 4096, 256 MiB, 131,072, 4096, 1024, 4096; per-transmission minimum charge 4 KiB; deletes keep passing after the other caps until `max_deletes`; `pub fn take_capped_notice(&mut self) -> bool` returns true once, the first time a cap is reached; `NERGAL_DISABLE_KITTY=1` (read once when the gate is built) drops every kitty action; `KittyGate::with_limits(limits)` and `KittyGate::default()`.
- `pub fn admit(&mut self, actions: Vec<Action>, cols: usize, rows: usize) -> Vec<Action>`: non-kitty actions pass in order; kitty actions follow design D2-D4. Small pure functions per rule (`check_transmit`, `check_placement`, `continuation`) so each rule has a direct test.
- State: the in-flight transfer (first chunk's typed fields, decoded bytes so far); lifetime counters (bytes, cells, transmissions, placements, deletes); a `capped` flag that, once set, drops every later transmission and placement (and any in-flight transfer at once, without decoding further chunks) while deletes pass until `max_deletes`; it logs once (`tracing::warn!`). Reaching `max_deletes` also sets `capped`. No image dimension map (design D2).
- Count before forwarding: transmissions add `max(width * height * 4, 4096)` bytes and 1 transmission; placements add `columns * rows` cells and 1 placement; deletes add 1 delete; an `a=T` must fit both; if anything would pass a cap, drop the action and set `capped`.
- Placements: `x` and `y` absent or 0. Rebuild every forwarded placement (`KittyImage::Display` and the placement of `TransmitData`) with `placement_id: None` (design Revision 1; doubling at `WZ/termwiz/src/surface/line/vecstorage.rs:19-30` + `WZ/term/src/terminalstate/image.rs:190-216`).
- Forward rebuilt actions with `verbosity: KittyImageVerbosity::Quiet`; every transmission (single chunk or assembled) is forwarded with `data: KittyImageData::DirectBin(decoded)` and `more_data_follows: false`. Decode with `base64::engine::general_purpose::STANDARD` (canonical padding, no trailing bits).
- Deletes: forward only `KittyImageDelete::All { .. }` and `ByImageId { .. }`, the latter with `placement_id: None`; drop other variants.
- String terminator residue: drop an `Action::Esc(Esc::Code(EscCode::StringTerminator))` that directly follows a kitty action the gate dropped or consumed as a chunk; keep a flag across `admit` calls.

### 2. Session wiring (`session.rs`, `config.rs`, `clippy.toml`)

- Fields `parser: termwiz::escape::parser::Parser` and `kitty_gate: KittyGate`. In `advance_bytes`, keep `track_sync_mode` and the OSC 7501 scanner on the raw bytes, then `let actions = self.parser.parse_as_vec(bytes); let actions = self.kitty_gate.admit(actions, self.cols as usize, self.rows as usize); self.terminal.perform_actions(actions);`. Never call `self.terminal.advance_bytes` again.
- `config.rs`: `fn enable_kitty_graphics(&self) -> bool { true }`.
- `src-tauri/clippy.toml` (new): `disallowed-methods = [{ path = "wezterm_term::Terminal::advance_bytes", reason = "PTY bytes go through TerminalSession's parser and KittyGate" }]`. Check the crate path name with `cargo tree` (`wezterm-term` exposes `wezterm_term`) and that clippy picks the file from `src-tauri/`.

### 3. Snapshot run index and the cap notice

- `session.rs` `push_image_cell_hashed` (`:640-684`): replace the reverse linear search of open runs (`:663`) with a multimap `HashMap<MergeKey, Vec<usize>>` keyed by the merge key (hash, z, end column, u1, v0, v1, padding); each image of a cell takes one matching open run out of the list (so 1024 identical stacked images extend 1024 runs, not one), and the extended run is re-inserted under its new key. Each image is O(1).
- The PTY reader thread (`pty.rs`, after `advance_bytes`, with the session guard held only to read the flag): when `take_capped_notice()` is true, emit `terminal:kitty-capped` with `{ session_id, pty_id }` outside the lock. Frontend (`src/stores/hooks.ts` setup, or the terminal store that owns toasts): one toast, neutral Spanish, e.g. "Imágenes kitty pausadas en esta terminal. Ábrela de nuevo para volver a verlas."

### 4. Layers

- `export const KITTY_UNDER_BG_Z = -1073741824;` `export function imageLayers(runs)` → `{ underBackground, underText, overText }`, stable ascending z (`src/components/terminal/imageLayers.ts`).
- `paintRow`: row clear → `underBackground` → backgrounds pass (all cells, keep the wide-cell skip) → `underText` → glyph/underline pass → `overText`. `drawImageRuns` takes an ordered list.

## Builder coordination

- Group A (Rust: plan sections 1-3, including the snapshot index and the `terminal:kitty-capped` emit) and group B (frontend: the toast in section 3 and the layers in section 4) share no files.
- Interface for the test-writer: `terminal::kitty_gate::{KittyGate, GateLimits}` with `admit(Vec<Action>, cols, rows) -> Vec<Action>`, `take_capped_notice()`, and `GateLimits { max_side, max_bytes, max_cells, max_transmissions, max_placements, max_deletes }`; `KittyGate::with_limits_and_switch(limits, disabled: bool)` for the kill-switch test; a session-level way to inject `GateLimits` for the oracle tests (e.g. `TerminalSession::set_kitty_limits`); tests build actions with `termwiz::escape::parser::Parser::new().parse_as_vec(bytes)` so they exercise real parsing. Frontend: `imageLayers(runs)`, `KITTY_UNDER_BG_Z`.
- Trap: the emulator's own parser must never be fed again (state would split between two parsers).
- Trap: forward rebuilt typed actions, never the original verbosity and never a placement id (design Revision 1).
- Trap: locked test files must be rustfmt-clean before hashing; `rustfmt --edition 2024 --config skip_children=true` on production files.

## Per-phase risk

- [Parser swap changes ordinary output] → a comparison test feeds a mixed byte corpus (text, CSI, OSC, DCS sixel, iTerm2 OSC 1337, split reads) through both paths and compares grids.
- [A gate rule misses a bypass] → a deterministic fuzz test (seeded generator, no new dependency) builds random kitty key sets and byte noise around them, parses, gates, and asserts every forwarded kitty action satisfies the allowlist and the forwarded totals stay under the caps.
- [Memory bound] → the oracle test counts image cells inside wezterm, in both screens and the scrollback, after line-moving actions and a resize; the private parts (`id_to_data`, image cache) rest on the arithmetic in design D4.
- [CPU] → the worst-stacking test (1024 distinct images placed once each at 16x8 on one cursor, three snapshots, `a=d,d=a`) under a time bound in release profile.
- [Placement-id doubling, Revision 1] → a regression test with 30 placements of one image under distinct `p` at `ESC [H`, plus a seeded property test over random placements (with and without `p`, overlapping, both screens) asserting the emulator's held image entries never exceed the forwarded placed cells.
- [Painter regression for sixel/iTerm2 (z=0)] → z=0 stays in `overText`.

## Verification

- `cd src-tauri && cargo clippy --all-targets -- -D warnings && cargo test && cargo fmt --check`; `npx tsc --noEmit`; `pnpm test`.
- `~/.claude/skills/work/scripts/work-gates.sh main kitty-graphics-canvas`.
- Manual walk (tasks group Verification).
