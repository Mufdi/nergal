## 1. Hook router (priority 1)

- [x] 1.1 Assess `process_event`'s signature (hooks/server.rs:641): if constructible with test doubles, add `#[cfg(test)] mod tests` with table-driven cases per routed event type; if `AppHandle`-entangled, extract the `route_event(event, ctx) -> actions` seam first (design D2) and test that.
- [x] 1.2 Cover `resolve_active_plan` (601) and the plan-review resolution path with dedicated cases (active plan found / absent / ambiguous).

## 2. migrate_legacy (priority 2)

- [x] 2.1 Parameterize `migrate_config_dir` (69) and `remove_legacy_cache_dir` (47) with injectable roots (thin public wrappers keep current call sites).
- [x] 2.2 Tempdir tests: fresh no-op; legacy → migrated (incl. `cluihud.db`→`nergal.db` rename); re-run idempotent; partial/corrupt → no panic, source preserved.

## 3. Frontend runner + suites (priority 3)

- [x] 3.1 Add `vitest` (devDependency) + `"test": "vitest run"` script; config-minimal (no jsdom).
- [x] 3.2 `src/stores/shortcuts.test.ts`: conflict detection — no duplicate code+modifier combos across the registry (export a test seam if needed, design D5).
- [x] 3.3 `src/lib/deepLinkRouter.test.ts`: valid routes, malformed URLs, hostile inputs (oversized, wrong scheme, traversal-looking payloads) → safe rejection.
- [x] 3.4 `src/lib/keymap.test.ts`: normalization round-trips.

## 4. Verification

- [x] 4.1 `cd src-tauri && cargo clippy -- -D warnings && cargo test && cargo fmt --check`
- [x] 4.2 `npx tsc --noEmit && npx vitest run`
- [x] 4.3 Note in the PR: pending `ci-quality-gates` wires these into CI; if it landed first, confirm the new suites run there.
