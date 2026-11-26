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

- `pub struct GateLimits { max_side: u32, max_decoded: usize, max_placements: usize, max_cells: usize }` with `Default` = 4096, 256 MiB, 1024, 131072; `KittyGate::with_limits(limits)` and `KittyGate::default()`.
- `pub fn admit(&mut self, actions: Vec<Action>, cols: usize, rows: usize) -> Vec<Action>`: non-kitty actions pass in order; kitty actions follow design D2-D4. Keep the decision logic in small pure functions (`check_transmit`, `check_placement`) so each rule has a direct test.
- State: an in-flight transfer (first chunk's transmit + optional placement, collected payload, cap); a tracked-image table keyed by image id (width, height, decoded bytes, forwarding order, placements with their cells); a private id counter in a reserved range (e.g. starting at `0xFF00_0000`) for transmissions without an id.
- Eviction emits `Action::KittyImage(Box::new(KittyImage::Delete { what: KittyImageDelete::ByImageId { image_id, placement_id: None, delete: true }, verbosity: KittyImageVerbosity::Quiet }))` before the action that needs room.
- Every forwarded kitty action is rebuilt with `verbosity: Quiet`; a completed chunked transfer is forwarded as one action with `more_data_follows: false` and the full payload.

### 2. Session wiring (`session.rs`, `config.rs`)

- Field `parser: termwiz::escape::parser::Parser` and `kitty_gate: KittyGate`. In `advance_bytes`, keep `track_sync_mode` and the OSC 7501 scanner on the raw bytes, then `let actions = self.parser.parse_as_vec(bytes); let actions = self.kitty_gate.admit(actions, cols, rows); self.terminal.perform_actions(actions);` (use the session's current `cols`/`rows`). Never call `self.terminal.advance_bytes` again.
- `config.rs`: `fn enable_kitty_graphics(&self) -> bool { true }`.

### 3. Layers (`src/components/terminal/imageLayers.ts`)

- `export const KITTY_UNDER_BG_Z = -1073741824;` `export function imageLayers(runs)` → `{ underBackground, underText, overText }`, stable ascending z.
- `paintRow`: row clear → `underBackground` → backgrounds pass (all cells, keep the wide-cell skip) → `underText` → glyph/underline pass → `overText`. `drawImageRuns` takes an ordered list.

## Builder coordination

- Group A (Rust: 1-2) and group B (frontend: 3) share no files.
- Interface for the test-writer: `terminal::kitty_gate::{KittyGate, GateLimits}` with `admit(Vec<Action>, cols, rows) -> Vec<Action>`; tests build actions with `termwiz::escape::parser::Parser::new().parse_as_vec(bytes)` so they exercise real parsing. Frontend: `imageLayers(runs)`, `KITTY_UNDER_BG_Z`.
- Trap: the emulator's own parser must never be fed again (state would split between two parsers).
- Trap: forward rebuilt typed actions, never the original verbosity.
- Trap: locked test files must be rustfmt-clean before hashing; `rustfmt --edition 2024 --config skip_children=true` on production files.

## Per-phase risk

- [Parser swap changes ordinary output] → a comparison test feeds a mixed byte corpus (text, CSI, OSC, DCS sixel, iTerm2 OSC 1337, split reads) through both paths and compares grids.
- [A gate rule misses a bypass] → a deterministic fuzz test (seeded generator, no new dependency) builds random kitty key sets and byte noise around them, parses, gates, and asserts every forwarded kitty action satisfies the allowlist and the budget.
- [Painter regression for sixel/iTerm2 (z=0)] → z=0 stays in `overText`.

## Verification

- `cd src-tauri && cargo clippy --all-targets -- -D warnings && cargo test && cargo fmt --check`; `npx tsc --noEmit`; `pnpm test`.
- `~/.claude/skills/work/scripts/work-gates.sh main kitty-graphics-canvas`.
- Manual walk (tasks group Verification).
