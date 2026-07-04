# critical-path-testing Specification

## Purpose
TBD - created by archiving change critical-path-test-coverage. Update Purpose after archive.
## Requirements
### Requirement: Hook event router is regression-tested

The hook event router (`process_event`, `resolve_active_plan`) SHALL have table-driven tests asserting the event→action mapping for every routed hook event type, runnable via `cargo test` without a live app or socket.

#### Scenario: routing table is covered

- **WHEN** `cargo test` runs
- **THEN** each hook event variant the router handles has at least one test asserting its observable effect (state written, event emitted, or control-flow result)

### Requirement: Legacy migration fs paths are tested against tempdirs

`migrate_config_dir` and `remove_legacy_cache_dir` SHALL be testable against injected root paths, with tests covering: fresh install (no-op), legacy present (migrated, content verified), re-run (idempotent), and partial/corrupt legacy state (no panic, non-destructive).

#### Scenario: idempotent re-run

- **GIVEN** a tempdir where the migration already ran
- **WHEN** it runs again
- **THEN** it is a no-op and no data is modified or lost

#### Scenario: corrupt legacy state does not destroy data

- **GIVEN** a partially-populated or unreadable legacy dir
- **WHEN** the migration runs
- **THEN** it does not panic and the legacy source is preserved on the failure path

### Requirement: Frontend has a test runner with first critical suites

The frontend SHALL have a test runner (`vitest`, `pnpm test` / `npx vitest run`) with suites for `shortcuts.ts` conflict detection (no duplicate code+modifier bindings), `deepLinkRouter.ts` URL parsing (valid, malformed, and hostile inputs), and `keymap.ts` normalization.

#### Scenario: shortcut collision is caught by a test

- **WHEN** a developer adds a binding whose `code`+modifiers collide with an existing one
- **THEN** the conflict-detection suite fails before the collision ships

#### Scenario: malformed deep link does not throw

- **WHEN** the deep-link parser receives a malformed or hostile `nergal://` URL
- **THEN** it returns a safe rejection (no exception, no partial action)

