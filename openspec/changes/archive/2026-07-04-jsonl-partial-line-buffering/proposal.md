## Why

Both file-tail readers silently lose records when a `notify::Modify` fires while the agent CLI is mid-write. `read_appended` in `src-tauri/src/agents/pi/jsonl_tail.rs:131-157` and its twin in `src-tauri/src/agents/codex/rollout_tail.rs:230-253` read to EOF, iterate `buf.lines()`, and set `new_offset = offset + n` **regardless of whether the read ended mid-line**. A partial final line fails to parse and is discarded — and because the offset has advanced past its bytes, the next read starts mid-JSON, so the *completed* record is also unparseable and lost. Silent gaps in Modified Files / status panels for both the Pi and Codex adapters.

## What Changes

- **Advance the offset only past fully-terminated lines**: read appended bytes, find the last `\n`, parse/emit only up to it, and return `offset + consumed_bytes` so the trailing partial line is re-read (now complete) on the next modify event. This makes the reader byte-oriented (`Vec<u8>` + `read_to_end`), which also fixes a second latent bug: `read_to_string().unwrap_or(0)` currently discards the entire read when the file ends mid-UTF-8-sequence.
- Apply the identical fix to both copies (`pi/jsonl_tail.rs`, `codex/rollout_tail.rs`); they stay separate (each has different parse/sink plumbing) but the offset arithmetic follows one documented rule.
- **No cross-read state needed**: re-reading the partial tail on the next event (offset-based) is simpler than buffering the fragment in memory and is idempotent across watcher restarts.

## Capabilities

### New Capabilities

_None._

### Modified Capabilities

- `pi-adapter`: the "Tail-f semantics with offset tracking" requirement now guarantees a record split across reads is never dropped (offset advances only past terminated lines).
- `codex-adapter`: same guarantee added for the rollout tail.

## Impact

- **`src-tauri/src/agents/pi/jsonl_tail.rs`** `read_appended` (131-157) and **`src-tauri/src/agents/codex/rollout_tail.rs`** `read_appended` (230-253): byte-read + last-`\n` split + partial-tail offset hold.
- **Tests**: both files have existing `#[cfg(test)]` coverage (`rollout_tail.rs:255+`); add cases — write half a JSON line, read (nothing emitted, offset unchanged past the previous line), complete the line, read again (one record emitted); multi-byte UTF-8 char split across the boundary.
- **Risk**: MEDIUM impact (silent data gaps) fixed with LOW-complexity, well-testable arithmetic; behavior for well-formed whole-line appends is unchanged.
- **Out of scope**: deduplicating the two `read_appended` implementations into a shared helper (worthwhile but optional — noted as a stretch task); CC/OpenCode transports (different mechanisms, not offset-tailed this way).
