# Implementation Plan: kitty-graphics-canvas

> Grounded in main `23470f5` (branch `mufdidev/fri-613-kitty-graphics`), symbols verified 2026-11-26. wezterm source `WZ` = `~/.cargo/git/checkouts/wezterm-26cdb9e734a97642/5046fc2`. Full analysis: `~/.claude/projects/-home-felipe-Projects-cluihud/work/kitty-graphics/analysis.md`.

## Verified codebase facts (do not re-assume)

- `TerminalSession::advance_bytes(&mut self, bytes) -> Vec<Vec<u8>>` at `src-tauri/src/terminal/session.rs:254-272`: `track_sync_mode(bytes)`, then `status_scanner.feed(bytes)` (OSC 7501, answering queries through `status_reply` and setting `query_seen`), then `self.terminal.advance_bytes(bytes)`, returning the 7501 report bodies. The pattern for a scanner with state across chunks is `ProgramStatusScanner` (`src-tauri/src/terminal/program_status.rs`).
- `NergalTerminalConfig` implements `TerminalConfiguration` at `src-tauri/src/terminal/config.rs:70` (`enable_csi_u_key_encoding` `:79`, `enable_kitty_keyboard` `:83`); wezterm's default `enable_kitty_graphics` is false (`WZ/term/src/config.rs:175`), checked at `WZ/term/src/terminalstate/kitty.rs:176`.
- wezterm kitty parsing: `KittyImage::parse_apc` (`WZ/termwiz/src/escape/apc.rs:1064`), a key without `=` drops the sequence (`:1073-1076`), `a` defaults to `t` (`:1080`); media loader `load_data` (`:181-265`); query always verbose (`:1055`); accumulator for `m=1` (`WZ/term/src/terminalstate/kitty.rs:22, 211, 228`) coalesced at `:855-909`; per-image check `w*h*4 <= 100_000_000` (`WZ/term/src/terminalstate/image.rs:273-290`); the `w=0` + `c=` division at `image.rs:98`.
- Release profile aborts on panic (`src-tauri/Cargo.toml:169`).
- Snapshot handles several images per cell and z: `grid_snapshot` (`session.rs:530-630`, images loop `:569-578`), run merging `push_image_cell_hashed` (`:640-684`); `ImageRun.z` (`src-tauri/src/terminal/types.rs`); differ hashes z (`differ.rs:103-123`); `Rgba8` served as kind 0 (`image_fetch.rs:120-125`).
- Canvas: `paintRow` (`src/components/terminal/terminalService.ts:1435-1487`) clears the row, `drawImageRuns(..., (z) => z < 0)` (`:1445`), then per cell background fill (`:1461-1464`) and glyph, then `drawImageRuns(..., (z) => z >= 0)` (`:1486`); `drawImageRuns` (`:1501`) draws in run order. Pure geometry lives in `src/components/terminal/imagePlacement.ts` (`runRects`), tested by `imagePlacement.test.ts`; `terminalService.ts` has no tests.
- Test fixtures and helpers: `src-tauri/src/terminal/image_runs_tests.rs` (`session()` at 800x480 px), `TerminalSession::new_with_pixels(80, 24, Some(800), Some(480), Box::new(std::io::sink()))`; `chafa -f kitty` emits `a=T,f=32,s=..,v=..,c=..,r=..,m=1` then 4096-byte `m=1` chunks, direct, no `o=z`.

## Execution order

1. `terminal/kitty_filter.rs` (pure, with tests).
2. `advance_bytes` uses it; `enable_kitty_graphics` true; snapshot tests for placement, move, delete.
3. `src/components/terminal/imageLayers.ts` (pure, with tests) and `paintRow` wiring.

## Plan

### 1. `KittyFilter` (`src-tauri/src/terminal/kitty_filter.rs`, declared in `terminal/mod.rs`)

- `pub const MAX_SEQUENCE: usize = 64 * 1024; pub const MAX_TRANSFER: usize = 128 * 1024 * 1024;`
- `#[derive(Default)] pub struct KittyFilter { ... }` with `pub fn filter(&mut self, bytes: &[u8]) -> Vec<u8>` returning the bytes for the emulator. States: pass (copy bytes; on `ESC` look ahead, carrying a trailing `ESC` or `ESC _` to the next call); in-kitty (after `ESC _ G`, collect until `ESC \` or BEL; CAN/SUB abort → drop held bytes, emit the CAN/SUB; over `MAX_SEQUENCE` → switch to skip); skip (discard until terminator or CAN/SUB). An APC whose first byte is not `G` is copied through as-is (switch back to pass after its `ESC _` prefix).
- Verdict for a complete kitty sequence via a pure `fn verdict(control: &[u8], payload_len: usize, transfer: &mut TransferState) -> Verdict` with `Verdict::{Forward, Drop}`: drop if any key lacks `=`, `t` not `d`, `o` present, `a` in `f|c|q`, `U` present, `w=0` or `h=0`; transfer bookkeeping: the first chunk of a transfer (`m=1` with an `a`/`t`/... control) starts a transfer whose verdict applies to its continuation chunks until `m=0`; payload sum over `MAX_TRANSFER` drops the rest and emits `ESC _ G m=0 ESC \` once to end wezterm's accumulator.
- Tests: every scenario of "Only safe kitty graphics sequences reach the emulator", plus BEL terminator, CAN abort, trailing `ESC` split, key without `=`.

### 2. Session wiring

- `TerminalSession` gets `kitty_filter: KittyFilter`; `advance_bytes` does `let bytes = self.kitty_filter.filter(bytes);` first and uses `&bytes` for `track_sync_mode`, `status_scanner.feed` and `terminal.advance_bytes`.
- `config.rs`: `fn enable_kitty_graphics(&self) -> bool { true }` with a WHY comment only if needed (the filter is the reason it is safe).
- Tests (new `#[path]` test file): the file-medium scenario through `advance_bytes` with a real temp file (exists and unchanged afterwards; nothing written to the reply writer); the query gets no reply (captured writer stays empty); `w=0,c=1` does not panic; the 2x2 image yields one z=0 run on the cursor row; `p=7` move; `a=d` delete; chafa-like chunked transfer (two `m=1` chunks + `m=0`) yields runs.

### 3. Layers (`src/components/terminal/imageLayers.ts`)

- `export const KITTY_UNDER_BG_Z = -1073741824;` `export function imageLayers(runs: ImageRun[]): { underBackground: ImageRun[]; underText: ImageRun[]; overText: ImageRun[] }` — split by z, each stably sorted ascending by z.
- `terminalService.ts` `paintRow`: compute layers once; draw `underBackground` right after the row clear; keep the per-cell background loop separate from the glyph loop (or a two-pass per row) so `underText` sits between backgrounds and glyphs; draw `overText` at the end. `drawImageRuns` takes the already-ordered list instead of a predicate.
- Tests: `imageLayers.test.ts` with the three spec scenarios plus empty input.

## Builder coordination

- Group A (Rust: steps 1-2) and group B (frontend: step 3) share no files.
- Interface for the test-writer: `terminal::kitty_filter::{KittyFilter, MAX_SEQUENCE, MAX_TRANSFER}` with `KittyFilter::default()` and `filter(&mut self, &[u8]) -> Vec<u8>`; `imageLayers(runs)` and `KITTY_UNDER_BG_Z`.
- Trap: the filter output, not the raw bytes, must feed every consumer in `advance_bytes`.
- Trap: never strip keys (design D5); drop whole sequences.
- Trap: locked test files must be rustfmt-clean before hashing; run rustfmt with `--config skip_children=true` on production files that `#[path]`-include test modules.

## Per-phase risk

- [Filter drops ordinary output] → tests for text around dropped sequences, non-kitty APC, split prefixes.
- [wezterm keeps a partial transfer] → the synthetic `m=0` end; a test that a cut transfer leaves no image runs and that a later transfer still works.
- [Painter regression for sixel/iTerm2 (z=0)] → layer tests keep z=0 in `overText`, which is where they are drawn today.

## Verification

- `cd src-tauri && cargo clippy --all-targets -- -D warnings && cargo test && cargo fmt --check`; `npx tsc --noEmit`; `pnpm test`.
- `~/.claude/skills/work/scripts/work-gates.sh main kitty-graphics-canvas`.
- Manual walk (tasks group Verification).
