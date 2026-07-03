## Why

The app's most critical paths have zero regression protection. Verified: `src-tauri/src/hooks/server.rs` — 1471 lines, the hook event router (`resolve_active_plan` at 601, `process_event` at 641, plan-review resolution) — has **no** tests module (its siblings `hooks/cli.rs:724` and `hooks/events.rs:258` do); `src-tauri/src/migrate_legacy.rs` performs destructive fs operations (`remove_legacy_cache_dir` at 47, `migrate_config_dir` at 69) while its tests (394-471, 7 cases) cover only the pure string helpers; the frontend has **zero** tests and no runner in `package.json` across ~47k LOC — with `src/stores/shortcuts.ts` (43.7 KB, conflict detection with a documented history of collision bugs), `src/lib/deepLinkRouter.ts` (URL parsing of untrusted input), and `src/lib/keymap.ts` as the highest-value cheap targets.

## What Changes

- **Hook router tests** (`hooks/server.rs`): table-driven tests for `process_event`'s event→action mapping and `resolve_active_plan`; extract a testable seam if the router is entangled with socket/app state (design.md D2).
- **Migration fs tests** (`migrate_legacy.rs`): tempdir-based tests for `migrate_config_dir` and `remove_legacy_cache_dir` — fresh→migrated, second-run idempotence, partial/corrupt source → no panic + non-destructive outcome.
- **Frontend runner + first tests**: add `vitest` (devDependency + `"test"` script), first suites for `shortcuts.ts` conflict detection, `deepLinkRouter.ts` parsing (valid/malformed/hostile inputs), `keymap.ts` normalization.
- **Prioritized as a verification baseline** (design.md D1): hook router first (highest blast radius), migration second (destructive, one-shot), frontend third (cheap, high-recurrence bug class).

## Capabilities

### New Capabilities

- `critical-path-testing`: the hook event router, the legacy one-shot migration, and shortcut/deep-link parsing logic carry regression tests, runnable via the project verify commands.

### Modified Capabilities

_None._

## Impact

- **`src-tauri/src/hooks/server.rs`**: +tests module (and possibly a small pure-router seam extraction); **`src-tauri/src/migrate_legacy.rs`**: +tempdir tests (and path-injection seams for the two fs fns if they hardcode `dirs::*` lookups).
- **Frontend**: `vitest` devDependency, `vitest.config.ts` (or vite config merge), `src/stores/shortcuts.test.ts`, `src/lib/deepLinkRouter.test.ts`, `src/lib/keymap.test.ts`.
- **Pairs with the pending `ci-quality-gates` change** (which wires `cargo test` and the frontend runner into CI) — authored in parallel; neither depends on the other to land, together they close the loop.
- **Risk**: LOW — additive tests; the only production-code edits are testability seams (path injection, router extraction), each behavior-preserving and compiler/test-verified.
- **Out of scope**: broad coverage goals or thresholds; E2E/webdriver testing; testing the PTY/terminal stack.
