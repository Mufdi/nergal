Linear: FRI-613

## Why

Modern terminal tools such as chafa and yazi draw images with the kitty graphics protocol. Nergal's emulator, wezterm-term, implements kitty graphics but Nergal keeps it off. Turning it on as it ships is unsafe: any program output (a `cat`, a `curl`) could make Nergal read any file, delete files under `/tmp`, unlink shared memory, exhaust memory, or crash the whole app with a division by zero. This change turns kitty on behind a filter that only lets safe kitty sequences reach the emulator.

## What Changes

- A byte-stream filter runs before the emulator, and only safe kitty graphics sequences pass it. A safe sequence carries direct data only, stays under a size cap, uses no compression, no animation and no Unicode placeholders, and carries no explicit zero width or height. Any other kitty sequence is dropped. APC sequences that are not kitty pass unchanged.
- The kitty query (`a=q`) is dropped and gets no answer, so no program, Claude Code included, sees kitty support yet. FRI-624 adds Unicode placeholders and then answers the query.
- The emulator config turns kitty graphics on.
- The canvas draws kitty placements in z order, in kitty's three layers: under the cell background, over the background and under text, and over text.
- Placing, replacing, moving and deleting a placement shows on the canvas, in the main terminal and in the quake terminal.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `terminal-wezterm`: adds the kitty graphics filter, kitty placements on the canvas, and the three image layers.

## Impact

- Rust: new `src-tauri/src/terminal/kitty_filter.rs`; `src-tauri/src/terminal/session.rs` (`advance_bytes` feeds the filtered bytes to the emulator and the existing scanners); `src-tauri/src/terminal/config.rs` (`enable_kitty_graphics`).
- React: a pure layer module next to `src/components/terminal/imagePlacement.ts`; `src/components/terminal/terminalService.ts` (`paintRow` draws three layers in z order).
- No new dependency, no IPC change, no schema change.
- Behavior for sixel and iTerm2 images is unchanged.
