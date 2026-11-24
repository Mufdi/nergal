# Evidence: bracketed-paste-gate (FRI-507)

## Planning

- iprev (opus): R1 REVISE (1 high: PR viewer's Apply with Claude delivered through terminal_paste; 2 medium; 5 low), R2 APPROVED (3 low, folded).

## Baseline

- Branch `mufdidev/fri-507-bracketed-paste-gate`, stacked on `mufdidev/fri-506-mouse-tracking-clicks` (`1ff8a5c`), gates green there: cargo test 1273, pnpm test 561, clippy/fmt/tsc clean.

## Red (test-writer)

- New `src-tauri/src/pty_user_paste_tests.rs` (encode cases incl. CRLF, lone CR, CRCRLF, U+009B + rebuilt marker; user_paste_payload with/without 2004; accessor; tracker helper 3 cases; write_bracketed unchanged). Include appended to pty.rs. Locked.
- Red: 13 compile errors, all missing symbols (E0425 encode_user_paste / user_paste_payload / tracker_after_paste, E0599 bracketed_paste_enabled).

## Green (Builder)

- clippy --all-targets -D warnings: clean; rustfmt --edition 2024 --check: clean.
- cargo test: 1283 passed, 0 failed (user_paste_tests included).
- tsc --noEmit: clean; pnpm test: 52 files, 561 tests passed.
- Tasks 1.2 and 1.3 done: bracketed_paste_enabled(), encode_user_paste, user_paste_payload, tracker_after_paste, terminal_paste reshaped, terminal_deliver registered and used by PrViewer.

## Revision 1 tests (test-writer)

- Adversarial review: 2 Important (no test that the user's bracketed paste strips markers; walks pending), 4 nits. All fixed (user preference).
- 4 regression guards for the user bracketed paste (embedded end marker, rebuilt marker, C1 forms, color sequences); `cargo test --lib user_paste`: 16 passed. Relocked.

## Final gates (orchestrator)

- clippy clean; `cargo test` 1289 passed, 0 failed; fmt clean; tsc clean; `pnpm test` 561 passed; work-gates: lock unchanged, gitleaks clean. Poisoned lock now recovered via `into_inner` and dropped before the writer lock.
- Walks 3.3-3.4 wait for the checkpoint-1 `.deb`.
