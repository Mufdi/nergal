## Context

Rust-side testing exists and is healthy in leaf modules (migrations, hooks/cli, hooks/events, adapters — 136+ tests green as of the agent-agnostic refactor), but the *integration spine* — the hook router in `hooks/server.rs` — grew to 1471 lines with none, and the destructive one-shot `migrate_legacy.rs` fs paths are untested beyond string helpers. The frontend never acquired a runner. This change frames the work as a **verification baseline**: the minimum set of tests that make the scariest regressions loud, not a coverage program.

## Goals / Non-Goals

**Goals:**
- Regression tests where failure = corrupted user state or broken core loop (hook routing, legacy migration, shortcut collisions, deep-link parsing).
- A frontend test runner that future changes (e.g. `useStaleGuard`, `useConflictResolution` from sibling pending changes) can immediately use.

**Non-Goals:**
- Coverage targets; UI component/render tests; E2E; testing wezterm/PTY internals.

## Decisions

### D1: priority order = blast radius × change frequency

1. **Hook router** — every agent event flows through `process_event` (server.rs:641); a routing regression breaks plan review, tasks, activities, cross-session delivery silently.
2. **`migrate_legacy`** — destructive fs ops that run exactly once per user upgrade; a bug destroys config with no retry (and it cannot be re-tested in production by definition).
3. **Frontend trio** — `shortcuts.ts` has a documented collision-bug history (memory + CLAUDE.md warning); `deepLinkRouter.ts` parses untrusted `nergal://` input; `keymap.ts` is pure and trivial to test.

### D2: hook router testability — test through `process_event` directly, extract only if forced

`process_event` (server.rs:641) takes the parsed event + state handles; the table-driven tests construct `HookEvent` values (constructors proven by `hooks/events.rs:258` tests) and assert on observable effects (DB rows, emitted events via a capturing test double, returned control flow). If `process_event`'s signature drags in un-mockable Tauri state (`AppHandle`), extract a pure `route_event(event, ctx) -> Vec<Action>` seam and test that — the smallest cut that decouples routing decisions from side-effect execution. **Alternative — full socket-level integration tests**: heavier, flakier (UDS lifecycle), and covered implicitly by dev usage. Deferred.

### D3: `migrate_legacy` — inject roots, tempdir tests

`migrate_config_dir` (69) / `remove_legacy_cache_dir` (47) resolve real user dirs internally; add `_at(base: &Path)` variants (or parameterize the existing fns, keeping thin public wrappers) so tests drive them against tempdirs. Cases: fresh (no legacy) → no-op; legacy present → migrated + verified content (including the `cluihud.db`→`nergal.db` rename inside); re-run → idempotent no-op; partial/corrupt legacy (missing subdirs, unreadable file) → no panic, source preserved on failure paths. **Alternative — mock fs crate**: heavier dependency for what tempdirs do natively. Rejected.

### D4: vitest, config-minimal

`vitest` shares Vite's transform pipeline (the project is Vite 7) — zero-config for pure-TS tests; add `"test": "vitest run"` script. Target only pure logic modules now (no jsdom/component testing environment yet — that's a later decision when a component test is actually wanted). **Alternative — node:test**: no TS transform without extra tooling; vitest is the ecosystem default on Vite projects. **Alternative — defer runner entirely**: keeps the 0-test status quo the audit flagged; rejected.

### D5: what "shortcuts.ts conflict detection" means concretely

Test the pure collision surface: the registry's binding table invariants (no duplicate `code`+modifier combos within a tier; reserved combos respected) — the exact class of the historical BUG entries. Implementation may need to export the table or a `detectConflicts()` helper; keep the export `@internal`-commented if it's test-only.

## Risks / Trade-offs

- [Testability seams alter production behavior] → seams are parameter-injection only; wrappers preserve signatures; `cargo test` + manual smoke.
- [vitest pulls a dependency tree into a repo that ships a desktop binary] → devDependency only; no runtime surface (consistent with the `remove-unused-frontend-deps` hygiene rule).
- [Hook router tests calcify current behavior including bugs] → table rows cite the spec/docs expectation, not just observed behavior; divergences found while writing tests get filed, not enshrined.

## Open Questions

- None blocking. Whether `shortcuts.ts` needs an exported seam is resolved at implementation.
