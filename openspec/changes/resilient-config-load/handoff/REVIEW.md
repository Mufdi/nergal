# REVIEW — resilient-config-load

## Orchestrator vet (S quick-fix, done directly) · 2026-07-03

**Verdict: PASS.**

- `Config::load` now delegates to a testable `load_from(path)` seam. The `Ok(contents)`
  branch matches `serde_json::from_str` explicitly: on `Err`, `tracing::error!` the serde
  message + path, `std::fs::copy` the original to `<path>.corrupt` (a second
  `tracing::error!` if the copy itself fails — no panic, no `unwrap`), then `default()`.
  Missing-file branch unchanged (silent defaults, no backup). Happy path unchanged.
- The original file is NOT overwritten at load — only the next `save()` would, and by
  then the `.corrupt` backup preserves the recoverable bytes.
- **Nuance discovered during impl** (documented in the test): `Config` has 6 required
  fields (no `#[serde(default)]`), so a partial hand-edited JSON that drops one of them
  parses as an error → treated as corrupt (backup + defaults). This is consistent with
  the change's goal (never silently lose recoverable settings) — a config the app itself
  saved always has every field, so only a destructive hand-edit hits this, and backing
  it up is the desired outcome.
- 3 tests (tempdir-scoped `load_from`): valid full config round-trips + no backup;
  missing file → defaults + no backup; corrupt file → defaults + `.corrupt` bytes ==
  original + source file untouched.

## Gates

- Gate 1-3: PASS (clippy `--all-targets` -D warnings clean, 31 config tests, fmt clean,
  tsc clean).
- Gate 6: 1 file = files_estimate.
