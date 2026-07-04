## 1. Pi tail

- [x] 1.1 `src-tauri/src/agents/pi/jsonl_tail.rs` `read_appended` (131-157): switch to `Vec<u8>` + `read_to_end`; find the last `\n`; parse/emit lines up to it (`str::from_utf8` per line, skip invalid with a `tracing::warn!`); return `offset + bytes_consumed` (position after that `\n`). Partial tail (no trailing `\n`) is left for the next read.

## 2. Codex rollout tail

- [x] 2.1 `src-tauri/src/agents/codex/rollout_tail.rs` `read_appended` (230-253): identical restructure (acc.ingest per complete line).

## 3. Tests

- [x] 3.1 Both test modules: (a) half-line write → read → nothing emitted, offset unchanged past previous line; complete the line → read → exactly one record; (b) multi-byte UTF-8 char split across the read boundary → no loss, no panic; (c) regression: N whole lines appended at once still emit N records with offset = EOF.

## 4. Verification

- [x] 4.1 `cd src-tauri && cargo clippy -- -D warnings && cargo test && cargo fmt --check`
