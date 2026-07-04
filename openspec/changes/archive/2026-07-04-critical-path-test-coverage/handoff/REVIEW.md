# REVIEW — critical-path-test-coverage

## Orchestrator vet (M additive tests + 2 verbatim seams; done directly) · 2026-07-04

**Verdict: PASS.** cargo 807 + vitest 101, all green.

- **migrate_legacy seams verbatim**: `migrate_config_dir()`/`remove_legacy_cache_dir()`
  now delegate to `_at(base)`/`_at(cache)` with the `dirs::*` lookup; the body moved
  verbatim into the `_at` fn. Behavior-preserving — the 7 pre-existing pure-helper tests
  + the whole suite (807) still pass; a mis-move would have broken them. 17 new tempdir
  tests: fresh no-op, legacy→migrated (incl. `cluihud.db`→`nergal.db` rename), idempotent
  re-run, partial source, corrupt/unreadable source (`#[cfg(unix)]` chmod 0) → no panic +
  source preserved, never-overwrite. The destructive/non-destruction/idempotence contract
  is the point and it's covered.
- **Hook router (D2 escape valve, accepted)**: no `route_event` seam — `process_event` is
  AppHandle-entangled with effects interleaved through every match arm (a ~400-line
  non-behavior-preserving lift), and the crate has zero `tauri::test` precedent (Cargo.toml
  `tauri` `features=[]`). Instead 18 tests on the PURE decision helpers the router
  delegates to: `resolve_active_plan` (7 — found/absent/ambiguous/inline-fallback/empty/
  legacy-mtime/none), `FrontendHookEvent::from_hook` (the event→emitted-action mapping for
  all 16 variants — this IS the spec's "observable effect: event emitted" requirement),
  `session_log_line` (6), `file_path_from_tool_input` (2). The untested side-effecting
  dispatch is documented inline with why (needs tauri::test — its own change).
- **Frontend gap only**: confirmed vitest + deepLinkRouter.test + keymap.test already
  existed (prior changes); added only the genuinely-missing `shortcuts.test.ts` (registry-
  wide `comboSignature` collision scan — the documented bug class — + id-uniqueness,
  green = regression guard) and the hostile-input deep-link cases (malformed/wrong-scheme/
  oversized/traversal → safe).

## Gates

- Gate 1-3: PASS (clippy --all-targets clean, 807 cargo tests, fmt clean, tsc clean, 101
  vitest).
- Gate 6 (scope): 2 backend test-seams (verbatim) + 3 test files; additive, low risk.
