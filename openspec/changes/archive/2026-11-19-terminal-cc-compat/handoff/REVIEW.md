# Review: terminal-cc-compat

Scope: working tree vs HEAD. Ran `cargo test --lib terminal::` (58 passed) and vitest on cssColor (passed).

## Bugs

### Critical
None found.

### Important
1. **Hold can starve under sustained streaming.** The cap is measured from `since`, and `since` restarts on every `l` then `h` pair. `emitter.rs:130-139` re-reads the state only after each wake. A read that ends with `l` plus the start of the next frame leaves the state `Some(new since)`. The emitter then holds again with a fresh 150 ms budget. With frames shorter than 150 ms and chunks that always straddle frames, the emitter never sends and the view freezes until output pauses. Fix: track the first-held instant in the emitter (or keep the oldest unflushed `since`) and cap against that. Add a test that feeds `l h BBB` every 50 ms for 500 ms and asserts an update arrives. Found by reading the code; not reproduced.
2. **Theme colors can fail silently and never retry.** `terminalService.ts:111-122` returns without logging when `cssColorToRgb` yields `null` (for example `#rrggbbaa`, `rgb(0 0 0 / 50%)`, `oklch`). A failed `invoke` is only logged. The next send waits for a `data-theme` flip. Sessions keep answering the previous theme (or black) until then. Log the unparsable token; consider one retry.
3. **No ordering guarantee between the first color report and agent spawn.** The send happens at `terminalService.ts` module load (line 132), and Claude Code asks `OSC 11` at startup. If a session spawns before the invoke lands, Claude Code gets the default black palette and never re-queries. Not verified against the app boot order.

## Spec gaps
- Spec "Colors set by an application keep precedence" covers only the background. wezterm forks the whole palette on the first app `OSC 10/11` (`term/src/terminalstate/mod.rs:670-675`), so the foreground also freezes. Spec and tests are silent on this.
- Proposal Impact says the command lives in `src-tauri/src/commands/`. It is in `src-tauri/src/pty.rs:1941-1944`.
- Task 2.1 asks for a repeated-`h` test with 100 ms spacing and bounds from `advance_bytes`. Done. No test covers a partial prefix followed by a new ESC (for example `ESC [ ? 2 0` then `ESC [ ? 2026 h`). The code handles it (`ends_with` on the joined window, `session.rs:155-172`), but nothing pins it.
- Tasks 3.1 to 3.3 are unchecked (expected at this stage).

## Nits
- Matcher false positive: tmux-style DCS passthrough `ESC P tmux; ESC ESC [ ? 2026 h ESC \` matches `SYNC_ON` (`session.rs:20-36`). Also `ESC ESC c` matches `FULL_RESET`. Rare; mention in a comment or ignore.
- Combined params such as `CSI ? 1049;2026 h` are not tracked. wezterm ignores 2026 anyway, so this only means no hold.
- Scroll and resize wakes also wait out the hold (up to 150 ms) because the wait sits before the subscriber check. Acceptable; note it.
- `emitter_sync_tests.rs` asserts wall-clock bounds (< 100 ms, 140-600 ms). Expect flakes on loaded CI.
- `session_compat_tests.rs:20-27`: `drain` returns on the first non-empty write. If wezterm writes the OSC answer in several `write` calls, the assert sees a fragment and the leftover pollutes the next drain. Passed once; could flake.
- `default_colored_cells_snapshot_without_fg_or_bg` passes without the change too. It is a regression guard, not proof of the feature.
- `terminalService.ts:111-122` splits the `entries`/`pending` and `failureToasted` declarations. Move `syncDefaultColors` below them.
- `cssColor.ts` ignores alpha and rejects 8-digit hex without saying so. Fine for the current tokens (`globals.css`), all 6-digit hex.
- Comments: no clear WHAT comment. Borderline: `session.rs:144` ("Matches that finish inside the carried tail were already handled") and `emitter_sync_tests.rs:35` (how a glyph is detected). Consider dropping both.

## Verified OK
- Matcher carry: tail of 7 bytes is enough for the 8-byte `SYNC_ON`; the `end <= carried` skip is correct because matches ending in the tail were seen with full left context.
- Wait loop: `notify_one` permits are consumed once per iteration, so no busy loop. A permit stored during the coalescing sleep costs one extra re-check only. `l` landing during the sleep is seen by the post-sleep read. Shutdown is re-checked on each wake and `Drop` aborts the task.
- Lock use: no `unwrap`/`expect` outside tests; poisoned sync lock fails open (no hold); lock order session then sync, never reversed.
- Palette: `SrgbaTuple::from((u8,u8,u8))` divides by 255; tests confirm `ffff` and `1818/1b1b` answers. wezterm reads `config.color_palette()` on each query when no app override exists (`terminalstate/mod.rs:660-665`), so live theme flips work.

Verdict: sound feature with one real stall risk (hold cap restarts per frame); fix Important 1 before merge.

## Resolution (main thread, 2026-11-18)

- Important 1 (hold starved by `l` + `h` per chunk): **fixed**. The hold ends when `since` differs from the frame the hold began with. Regression test `emitter_review_tests.rs` (separate test-writer) fails on the old loop and passes now, verified by swapping the old loop back temporarily.
- Important 2 (silent skip on an unparsable token or failed invoke): **accepted**. Theme tokens are hex today; a failure is logged and the next theme flip resends.
- Important 3 (first `OSC 11` before the first report): **accepted**, as design D4 states. The send fires at module load, before any session can spawn.
- Spec gaps: the command location is corrected in the proposal; the app-set foreground freeze is wezterm behavior and stays as designed.
