Linear: FRI-613

## Why

Modern terminal tools such as chafa and yazi draw images with the kitty graphics protocol. Nergal's emulator, wezterm-term, implements kitty graphics but Nergal keeps it off. Turning it on as it ships is unsafe: any program output (a `cat`, a `curl`) could make Nergal read any file, delete files under `/tmp`, unlink shared memory, exhaust memory, or crash the whole app with a division by zero. This change turns kitty on behind a gate that only lets safe kitty actions reach the emulator.

## What Changes

- Nergal parses the PTY output itself, with the same termwiz parser the emulator uses, and applies the parsed actions to the emulator. A gate between the two lets only safe kitty graphics actions through: direct RGB or RGBA data of a checked size, placements that cannot divide by zero, and deletes. It drops every other kitty action, assembles chunked transfers itself, and forwards every action in quiet mode, so the emulator never replies.
- Kitty images stay within a per-terminal budget of decoded bytes, live placements and covered cells; the oldest image is deleted when a new one would pass it.
- The kitty query (`a=q`) is dropped and gets no answer, so no program, Claude Code included, sees kitty support yet. FRI-624 adds Unicode placeholders and then answers the query.
- The emulator config turns kitty graphics on.
- The canvas draws kitty placements in z order, in kitty's three layers: under the cell background, over the background and under text, and over text.
- Placing, replacing, moving and deleting a placement shows on the canvas, in the main terminal and in the quake terminal.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `terminal-wezterm`: Nergal parses the PTY output and applies the actions; adds the kitty gate, the kitty memory budget, kitty placements on the canvas, and the three image layers.

## Impact

- Rust: new `src-tauri/src/terminal/kitty_gate.rs`; `src-tauri/src/terminal/session.rs` (owns a termwiz parser; `advance_bytes` parses, gates and calls `perform_actions`); `src-tauri/src/terminal/config.rs` (`enable_kitty_graphics`).
- React: a pure layer module next to `src/components/terminal/imagePlacement.ts`; `src/components/terminal/terminalService.ts` (`paintRow` draws three layers in z order).
- No new dependency (termwiz is already a direct dependency), no IPC change, no schema change.
- Behavior for sixel and iTerm2 images is unchanged.
