# Evidence: terminal-pixel-geometry (FRI-597)

Branch `fri-597-terminal-pixel-geometry` from `main` `fe2872b`.

## Baseline

Skipped: CI is green on the base commit (`gh run list --branch main`: last 3 runs `ok`).

## Root cause reproduction (2026-11-22, before planning)

Temporary spike test (deleted afterwards): 64 × 48 sixel generated with ImageMagick fed into `TerminalSession::new(80, 24, sink)`:

```
thread 'terminal::spike_sixel::spike_sixel_with_zero_pixel_size' panicked at
~/.cargo/git/checkouts/wezterm-26cdb9e734a97642/5046fc2/term/src/terminalstate/image.rs:103:21:
attempt to divide by zero
test result: FAILED. 0 passed; 1 failed
```

## Plan review

iprev (opus): R1 REVISE (2 high, 4 medium, 6 low) → R2 REVISE (1 high, 2 medium, 3 low) → R3 APPROVED (4 low, applied).

## Red (test-writer, sonnet)

Locked in `handoff/.red-tests` (5 files, `git hash-object`):
- `src-tauri/src/terminal/sixel_zero_pixel_tests.rs`: runtime Red, `panicked at .../term/src/terminalstate/image.rs:103:21: attempt to divide by zero`.
- `src-tauri/src/terminal/pixel_geometry_tests.rs`, `src-tauri/src/pty_pixel_tests.rs`, `src-tauri/src/log_owner_tests.rs`: compile Red (`E0425 pty_size_for`, `E0061 resize_pty` arity, `E0432 clamp_pixel_geometry`, `E0432 try_lock_log/retry_lock_log/rotate_log`, `E0599 new_with_pixels/resize_with_pixels`; 13 errors).
- `src/components/terminal/pixelGeometry.test.ts`: Red on the unresolved `./pixelGeometry` import; 458 other vitest tests pass.

Assumed API (Builder contract): `clamp_pixel_geometry`, `TerminalSession::new_with_pixels`, `TerminalSession::resize_with_pixels -> bool` (session.rs); `pty_size_for`, `resize_pty(state, id, cols, rows, pw, ph)` (pty.rs); `try_lock_log`, `retry_lock_log`, `rotate_log` taking the log dir (lib.rs, Linux); `pixelGeometry`, `nextResize`, types `Sent`, `ResizePayload` (pixelGeometry.ts).

Test-writer caveat: behavioral Rust expectations (scroll-region rows, `CSI 16 t` = `6;20;10t`, same-process flock conflict) were derived from the plan and wezterm source; first run happens in Green.

## Green (Builder, sonnet) — groups 1, 2, 3

Files: `src-tauri/src/terminal/session.rs`, `src-tauri/src/terminal/mod.rs`, `src-tauri/src/pty.rs`, `src-tauri/src/lib.rs`, `src/components/terminal/pixelGeometry.ts` (new), `src/components/terminal/terminalService.ts`. `src/stores/quake.ts` unchanged (headless spawn gets the fallback cell).

Deviations / rulings:
- `#[allow(clippy::type_complexity)]` on the `pixel_geometry_tests` mod line (`terminal/mod.rs`): the locked test's table type at `pixel_geometry_tests.rs:64` trips the lint under `--all-targets`. Ruling: accepted; test-only scope, no production lint weakened, the lock forbids editing the test. Justifies gate 6b WARN.
- `terminal::session` became `pub(crate)` so `pty.rs` reaches the clamp.
- `sendResize` may send duplicate identical invokes while one is in flight; the backend no-op absorbs them.

Gates (orchestrator re-run, 2026-11-22):

| Gate | Result | Detail |
|------|--------|--------|
| 1. Compile/Type | PASS | `cargo clippy --all-targets -- -D warnings`: No issues found; `npx tsc --noEmit`: No errors found |
| 2. Tests | PASS | `cargo test`: 1176 passed, 1 ignored (5 suites, 27.64s); `pnpm test`: 464 passed (44 files) |
| 2. Red locks | PASS | 5 locked file(s) unchanged |
| 3. Lint/format | PASS | `cargo fmt --check` exit 0 |
| 5. Deps | SKIP | no manifest or lockfile changed |
| 6a. Placeholders | PASS | none |
| 6b. Bar weakening | WARN | `#[allow(clippy::type_complexity)]` (justified above) |
| 6c. Secrets | PASS | gitleaks clean |

Comment audit: added comments are WHY-only (lock close-on-exec, dup2 SAFETY after threads, no-op resize reason, lastSent on resolve, hidden container fallback).

## Adversarial review (opus) → group 5

Verdict FAIL with 2 Important (log guard dropped when `setup` returns; resolve-order `lastSent` race) + compliance gap (lock lifetime). Details in `REVIEW.md`. Nits taken: 0 cols/rows via IPC, duplicate predicate, leftover `resize(cols, rows)`. Nits accepted: post-spawn test only covers the pure function; no direct test of the master-resize skip (predicate + scroll-region test cover it); API header blocks in locked tests.

Red (group 5, locked): `src/components/terminal/resizeTracker.test.ts` (5 tests fail: missing `recordRequest`, `afterResizeFailure`), `src-tauri/src/pty_zero_size_tests.rs` (compile Red: `E0425 validate_grid_size`). Lock lifetime (5.2) has no unit seam (needs a running Tauri app); verified by review + manual 4.4.

## Green (group 5, Builder sonnet) + re-review

- `lib.rs`: `LogOwner { _guard }` kept in Tauri managed state (`app.manage`) for both the startup and the retry path; the orchestrator replaced the Builder's `#[allow(dead_code)]` with a `_guard` field.
- `pixelGeometry.ts`: `recordRequest`, `afterResizeFailure`; `terminalService.ts`: `lastRequested` set before the invoke, cleared on failure only when it is still the latest request.
- `pty.rs`: `validate_grid_size` first in `resize_pty`, `resize_session_terminal`, `start_claude_session`, `spawn_aux_shell`.
- `session.rs`: `TerminalSession::resize(cols, rows)` removed (only an in-file test used it; switched to `resize_with_pixels`). `resize_pty` keeps one pre-check (`size_changed` must run before `master.resize` so a no-op never issues `TIOCSWINSZ`); the redundant second gate was removed by the orchestrator after the re-review nit.

Re-review (adversarial, opus): PASS WITH FINDINGS, no new Important. Remaining nit accepted: API header blocks in locked tests.

Final gates (orchestrator, 2026-11-22):

| Gate | Result | Detail |
|------|--------|--------|
| 1. Compile/Type | PASS | `cargo clippy --all-targets -- -D warnings`: No issues found; `npx tsc --noEmit`: No errors found |
| 2. Tests | PASS | `cargo test`: 1181 passed, 1 ignored (5 suites, 29.70s); `pnpm test`: 469 passed (45 files) |
| 2. Red locks | PASS | 7 locked file(s) unchanged |
| 3. Format | PASS | `cargo fmt --check` exit 0 |
| 6a/6c | PASS | no placeholders; gitleaks clean |
| 6b | WARN | `#[allow(clippy::type_complexity)]` on the locked test module line (justified in Green section) |

Pending: manual walk 4.2–4.4 by the user on the installed `.deb`.

## Manual walk (user, installed .deb, 2026-11-22)

- 4.2 PASS: yazi in the quake, hovering images: Nergal stays open; the preview area stays empty (drawing is FRI-600).
- 4.3 PASS: `printf '\e[14t'` answers real pixel values, not `0;0`.
- 4.4 PASS: a second launch from the launcher left `nergal.log` with the first instance's run (starts 23:45:15Z, 110 lines). The first start of the new build rotated the previous run (starts 19:54:27Z) into `nergal.log.1`; `nergal.log.lock` exists.
