## 1. Extract shared helper

- [ ] 1.1 Create `src/tracker_shared/context_budget.rs` with `pub const DESCRIPTION_TRUNC_MARKER` + `pub fn head_tail_truncate(desc: &str, remove: usize) -> String` (verbatim from the current copies) + the `head_tail_truncate_is_char_boundary_safe` test moved into its `#[cfg(test)] mod tests`
- [ ] 1.2 Declare `pub mod context_budget;` in `src/tracker_shared/mod.rs`

## 2. Migrate call sites

- [ ] 2.1 `clickup/integration.rs`: delete local `head_tail_truncate` + `DESCRIPTION_TRUNC_MARKER` + the local char-boundary test; import from `crate::tracker_shared::context_budget` (fn call site in `fit_to_budget`, const use in `render`/tests)
- [ ] 2.2 `linear/integration.rs`: same deletion + import
- [ ] 2.3 Confirm no other references to the removed local symbols remain (grep both files)

## 3. Verify

- [ ] 3.1 `cargo clippy --all-targets -- -D warnings` + `cargo test` + `cargo fmt --check` green (existing `fit_to_budget` + char-boundary tests still pass)
- [ ] 3.2 `tsc --noEmit` green (no frontend change)
