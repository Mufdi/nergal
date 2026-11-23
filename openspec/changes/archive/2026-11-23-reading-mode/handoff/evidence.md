# Evidence: reading-mode (FRI-494)

## Baseline (2026-11-23, base main 88a4392)

- `pnpm test`: 46 files, 472 tests passed (3.25 s).
- Rust: the FRI-602 run on the same tree passed clippy `--all-targets -D warnings`, 1236 tests and `fmt --check` (see `openspec/changes/archive/2026-11-23-launch-prompt-via-file/handoff/evidence.md`). CI on main earlier runs green.
- No pre-existing failures.

## Planning

- iprev R1 (opus): APPROVED, 3 medium + 4 low. All medium and 3 low folded into spec, implementation and tasks (see the `validated` event in `.work-modules.json`). Low 8 (rollback) needed no change.

## Red (test-writer, sonnet)

- Files: `src/stores/readingMode.test.ts` (1.1 frontend, 2.1), `src/components/terminal/readingCols.test.ts` (3.1, incl. `nextResize` 180 → 100), `src/components/plan/readingWidth.test.ts` (4.1), `src-tauri/src/config_reading_mode_tests.rs` (1.1 Rust). Locked in `.red-tests`.
- Production edit by the test-writer: `#[cfg(test)] #[path = "config_reading_mode_tests.rs"] mod reading_mode_tests;` at the end of `src-tauri/src/config.rs`.
- `toastsAtom` is mocked with a write-only recorder atom (the real one calls sileo and the notifications store).
- `pnpm test`: 3 files fail on missing modules (`./readingWidth`, `./readingCols`, `@/stores/readingMode`), 472 tests pass. `cargo test --lib reading_mode_tests`: E0609/E0560 on the missing `Config` fields.
- Not unit-testable in node (no DOM): canvas centering, the 1 s config wait on first spawn, the late-config refit, the component roots. Covered by walks 5.3–5.6.

## UI brief

- Existing look kept: no new components, colors or chrome; the only visual changes are a width class and a centered canvas.
- Markdown (Plan, Spec, Obsidian, Linear, ClickUp roots): `mx-auto max-w-[72ch]` replaces `max-w-none` when the flag is on; `px-4` stays, so the text column is about 67ch.
- Terminal (center region only): canvas capped at 100 columns and centered by `justify-content:center` on the existing flex container; the container background fills the side gutters, so no seam appears. Quake is untouched.
- Feedback: the existing toast (`type: "info"`, English per spec) names the surface and the new state; no persistent indicator, the layout change itself is the state.
- Interaction: `leader f` routed by focus zone (terminal, panel); sidebar and quake are a silent no-op. No new focus targets, no new motion.

## Green (Builder, sonnet)

- Tasks 1.2, 1.3, 2.2, 2.3, 3.2, 3.3, 3.4, 4.2 implemented. Locks unchanged after the build (`git hash-object` on all 4 paths matched).
- First verify: `pnpm test` 49 files / 496 tests passed; `tsc --noEmit` clean; clippy `--all-targets -D warnings` clean; `cargo test` 1239 passed, 1 ignored; `fmt --check` clean.
- Red conflict (ruled by the orchestrator): the Builder made the 80×24 fallback joint because the locked test expected `readingLayout(40, 800)` → `{cols: 80, rows: 24}`. implementation.md says the fallback stays as in the old `computeCols`, per dimension. Ruling: test wrong. The test-writer changed that case to `{cols: 80, rows: 50}` and added 1440×20 → `{cols: 100, rows: 24, centered: true}`; re-locked at `bda9b578`. The Builder restores the per-dimension fallback.
- Orchestrator review of the diff: `waitForConfig` landed between `waitForLayout` and its doc comment; the `config.rs` doc had a WHAT prefix. Both sent back to the Builder.

## Reviews (B5)

- Adversarial (opus): FAIL, 3 Important + 4 Nit. Frontend (sonnet): PASS_WITH_NOTES, 2 Important + 5 Nit, no 🔴. Consolidated in `REVIEW.md`.
- Ruled false positive: frontend "margin click does not focus" (`TerminalManager.tsx:121-127` refocuses).
- User confirmed (2026-11-23) as tasks group 5: pending-spawn refit, margins scroll and drag, 1 s real-time config wait. Tables: user rejected horizontal scroll and asked for tables that fit the column → `[overflow-wrap:anywhere]` on `th`/`td`.
- Not acted on (warnings): adversarial Nit 2 (`migrateKeymapOverrides` drops a user override on `leader f`, as for any new default), Nit 3 (proposal says "right panel"; the flag also reaches the floating Linear/ClickUp detail, which is markdown too), frontend nits on keywords, toast phrasing and the scroll indicator position. Late-config scenario: covered by the refit subscription and by `refitNeeded` after registration; no walk can force a config slower than 1 s.

## Group 5 (Red + Green)

- Red: `cellMapping.test.ts`, `waitUntil.test.ts` (fake clock), missing modules; locked.
- Green: `cellMapping.ts`, `waitUntil.ts`, `terminalService.ts` (wheel + drag mousemove on the container via `pointerCell`; `refitNeeded` after `entries.set`; `waitForConfig` on `waitUntil` 1000 ms), `[overflow-wrap:anywhere]` on table cells. No deviations.

## Gates (after group 5)

| Gate | Result | Detail |
|------|--------|--------|
| 5. Deps | SKIP | no manifest or lockfile changed |
| 6a. Placeholders | PASS | none |
| 6b. Bar weakening | PASS | none |
| 6c. Secrets | PASS | gitleaks clean |
| 2. Red locks | PASS | 6 locked file(s) unchanged |

- `pnpm test`: 51 files, 509 tests passed. `tsc --noEmit`: no errors. `cargo clippy --all-targets -D warnings`: no issues (Builder run). `cargo test`: 1239 passed, 0 failed. `cargo fmt --check`: clean.

## Walk 1 (user, 2026-11-23)

- Passed: A1-A3 (terminal cap, margins, drag), B2 (annotations), C1-C4 (restart, quake, hidden session, sidebar). Tasks 6.3, 6.5, 6.6 marked.
- Revised by the user: the markdown cap of 72ch reads too narrow at 12 px; raised to 100ch, the terminal's column count. Tables span the full view in both states (on top of the cell wrap).
- Out of scope, filed: FRI-604 (plan panel `A` key does not approve), FRI-605 (open a Linear issue as a tab, parity with ClickUp; not a regression: the `linear-issue` tab was a placeholder since `3a4189c`).
- Red: test-writer rewrote `readingWidth.test.ts` (100ch, `MARKDOWN_CONTAINER_CLASS`, `MARKDOWN_TABLE_CLASS`); 3 failures for the expected reasons; relocked at `5abfd3f7`.
- Green: `readingWidth.ts` exports both classes; both views wrap the root in an `@container` div; tables use the breakout class. The annotation toolbar moved outside the container, so a query container can never become the containing block of the fixed toolbar. `pnpm test`: 511 passed. `tsc --noEmit`: clean.
- Re-verify: `cargo clippy --all-targets -D warnings` no issues; `cargo test` 1239 passed, 0 failed; `cargo fmt --check` clean; work-gates: 6 locks unchanged, gitleaks clean, no placeholders.

## Full-width tables setting (user request, 2026-11-23)

- The user asked for a switch for terminal and panel. The terminal half is not possible (one PTY column count; Claude Code picks grid or vertical list per table). The user chose a panel-only switch, default on.
- Red: test-writer added the Rust default/round-trip test, the `readingFullWidthTablesAtom` tests and the `markdownTableClass` tests; all failed for the expected reasons. A cast in `readingMode.test.ts` broke `tsc`; the test-writer fixed it. All three relocked.
- Green: `reading_mode_full_width_tables` (`default_true`), `readingFullWidthTablesAtom`, `markdownTableClass`, shared `ReadingTable`, switch in Appearance plus search keywords.
- Verify: `pnpm test` 514 passed; `tsc` clean; clippy clean; `cargo test` 1240 passed, 0 failed; fmt clean; work-gates: 6 locks unchanged, gitleaks clean, no placeholders. `.deb` built 19:20.
