## 1. Load-path fix

- [x] 1.1 `src-tauri/src/config.rs` `Config::load` (433-440): split the `unwrap_or_default()` — on `serde_json::from_str` error, `tracing::error!` the serde message + path, copy the file to `config.json.corrupt` (same dir; log if the copy itself fails, no panic, no `unwrap`), then fall back to `Self::default()`. Missing-file branch unchanged.

## 2. Tests

- [x] 2.1 Unit tests (tempdir-scoped config path — add a testable seam for `config_path` if needed, e.g. an internal `load_from(path)` the public `load()` delegates to): valid file loads; missing file → defaults + no backup; corrupt file → defaults + backup bytes equal original + error logged.

## 3. Verification

- [x] 3.1 `cd src-tauri && cargo clippy -- -D warnings && cargo test && cargo fmt --check`
- [ ] 3.2 Manual: hand-corrupt `~/.config/nergal/config.json` (add a stray brace), launch → app boots on defaults, `.corrupt` sibling exists with the broken content.
