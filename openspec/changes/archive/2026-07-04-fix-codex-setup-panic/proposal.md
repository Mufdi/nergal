## Why

`merge_nergal_entries` panics on a malformed `~/.codex/hooks.json` whose top-level `"hooks"` key holds a non-object (e.g. `[]`, a string, a number). `hooks_obj.entry("hooks").or_insert_with(|| json!({})).as_object_mut().unwrap()` (`src-tauri/src/agents/codex/setup.rs:49-53`) returns the existing value when the key is present, so `or_insert_with` never fires, `as_object_mut()` yields `None`, and `.unwrap()` panics — crashing Codex setup. Reachable on any legacy, hand-edited, or truncated hooks file, and the bare `unwrap` also violates the "no `unwrap()`/`expect()` outside tests" convention.

## What Changes

- Replace the panicking `entry(...).or_insert_with(...).as_object_mut().unwrap()` chain at `setup.rs:49-53` with a match on `as_object_mut()` that, on `None` (key present but not an object), overwrites `hooks_obj["hooks"]` with `json!({})` and re-borrows — the same normalization shape already used for the per-event array a few lines below (`setup.rs:59-65`).
- Net effect: a non-object `"hooks"` value is treated like a missing one and replaced with a fresh object, so setup proceeds instead of crashing. No other behavior changes.

## Capabilities

### New Capabilities

<!-- none -->

### Modified Capabilities

- `codex-adapter`: the `setup_agent('codex')` merge requirement is strengthened to mandate robust handling of a malformed `hooks.json` — a non-object top-level `"hooks"` value SHALL be normalized rather than panic.

## Impact

- **`src-tauri/src/agents/codex/setup.rs`**: `merge_nergal_entries` lines 49-53 — swap the `.unwrap()` chain for the match-and-renormalize pattern already established at 59-65.
- **Tests**: add a `#[cfg(test)] mod tests` to `setup.rs` (none exists today) with a unit test asserting `merge_nergal_entries` on `{"hooks": []}` (and another non-object variant) returns without panicking and produces the canonical entries.
- **Risk**: LOW — single-function change, no schema or API surface touched; the correct behavior is already demonstrated by the sibling array branch.
- **Out of scope**: broader validation/repair of user-authored hook entries; changes to the atomic-write or obsolete-entry-pruning logic.
