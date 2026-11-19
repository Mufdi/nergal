# Evidence: terminal-cc-compat

- iprev: 2 rounds, sonnet (opus weekly limit). R1 REVISE (12: shared cell raced in tests, wait loop under-specified, scanner bound, startup race); R2 APPROVED with task-level tightenings applied (check order, ESC c split, no unwrap, cap-passed note).
- Red: `session_compat_tests.rs` (12), `emitter_sync_tests.rs` (5), `cssColor.test.ts` (11), separate test-writer, locked; Rust failed to compile on the missing locked items, vitest on the missing module.
- Builder (sonnet subagent): config cell + command, matcher + `SyncState`, emitter bounded wait, `cssColorToRgb` + `syncDefaultColors` (async, try/catch, so mocked `invoke` cannot throw at import).
- Gates (main thread re-run): clippy `--all-targets -D warnings` clean; `cargo test` 1153 passed, 1 ignored; rustfmt check clean on changed files; `tsc` clean; `pnpm test` 453 passed (42 files); `work-gates.sh`: 3 locks unchanged, gitleaks clean.
- Walks 3.2 and 3.3 passed on the 21:39 `.deb` (user, 2026-11-19).
