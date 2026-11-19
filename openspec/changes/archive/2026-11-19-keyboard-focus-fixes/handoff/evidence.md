# Evidence: keyboard-focus-fixes

- iprev: 3 rounds, sonnet (opus weekly limit). R1 REVISE (11 findings, incl. global focusin risk to BUG-09), R2 REVISE (8), R3 REVISE (1 medium applied: preconditions moved into the helpers; lows accepted in design.md).
- Red: 6 files locked in `.red-tests`, written by separate test-writers. `signals.test.ts` from the first plan was deleted before locking, because the revised plan replaced the watermark helper.
- Green: `npx tsc --noEmit` clean; `pnpm test` 434 passed (41 files). `work-gates.sh main keyboard-focus-fixes`: 6 locks unchanged, gitleaks clean; 6b WARN lines are the two `eslint-disable react-hooks/exhaustive-deps` from the already-committed Branches/History chips (base `main`).
- Review: adversarial (sonnet) → see REVIEW.md resolution.
- Final gates (batch, 2026-11-18): clippy clean; `cargo test` 1154 passed, 1 ignored; rustfmt clean; `tsc` clean; `pnpm test` 458 passed (43 files); locks unchanged.
- Walks 4.2–4.5 passed on the 21:39 `.deb` (user, 2026-11-19). Known and accepted: with the sidebar collapsed, `Ctrl+digit` on a non-live session resumes it only after the sidebar expands.
