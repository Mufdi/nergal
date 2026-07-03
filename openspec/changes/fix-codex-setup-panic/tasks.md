## 1. Fix the panicking merge

- [ ] 1.1 `src-tauri/src/agents/codex/setup.rs:49-53` — replace `hooks_obj.entry("hooks").or_insert_with(|| json!({})).as_object_mut().unwrap()` with a non-panicking normalization: if the `"hooks"` entry is present but `as_object_mut()` is `None`, overwrite it with `json!({})` and re-borrow (mirror the per-event array shape at `setup.rs:59-65`). No bare `unwrap()` remains on this path.

## 2. Tests

- [ ] 2.1 Add `#[cfg(test)] mod tests` to `setup.rs` (none exists today): `merge_nergal_entries(json!({"hooks": []}))` and `merge_nergal_entries(json!({"hooks": "x"}))` do not panic and produce the canonical nergal entries under `hooks.<event>`; a doc with existing user entries is preserved (conservative-merge regression).

## 3. Verification

- [ ] 3.1 `cd src-tauri && cargo clippy -- -D warnings && cargo test && cargo fmt --check`
