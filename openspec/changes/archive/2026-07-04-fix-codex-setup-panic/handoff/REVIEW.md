# REVIEW — fix-codex-setup-panic

## Orchestrator vet (XS ceremony: no reviewer spawn; vet substitutes) · 2026-07-03

**Verdict: PASS** (after one orchestrator polish).

- Builder (haiku) replaced the reachable `.unwrap()` panic with a normalize-and-reborrow
  match, but its fallback arm used `unreachable!()` — still a panic macro on the exact
  path this change exists to de-panic (and the sibling pattern at the per-event arrays
  ends in a pre-existing `.unwrap()`, so "mirror the sibling" reproduced the class).
- Orchestrator polish: truly panic-free shape — `if !matches!(get("hooks"),
  Some(Object))` → insert `json!({})`, then `let Some(Value::Object(..)) = get_mut
  else { return root }` (provably-unreachable degrade to no-op merge, WHY-commented).
  Setup can now never crash on this path, satisfying the repo's no-panic convention
  strictly rather than by "provably impossible" argument.
- tasks.md 2.1 discrepancy resolved: a `mod tests` DID already exist (proposal was
  stale on "none exists today"); the conservative-merge regression is covered by the
  pre-existing `merge_preserves_user_entries`, plus 4 more pre-existing merge tests.
  2 new tests added (`merge_normalizes_hooks_array`, `merge_normalizes_hooks_string`).
- Pre-existing panic-adjacent sites NOT touched (root `expect` at fn head, per-event
  `.unwrap()` at the array branch): both provably-unreachable post-normalization,
  out of scope per the proposal's surgical boundary. Candidates for the Band-C/D
  "discarded Result / panic sweep" if it ever lands.

## Gates

- Gate 1-3: PASS (clippy clean, 744 tests incl. 2 new + 6 total in setup.rs, fmt
  clean).
- Gate 6: 1 file = estimate.
