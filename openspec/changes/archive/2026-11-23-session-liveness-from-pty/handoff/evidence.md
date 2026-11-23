# Evidence — session-liveness-from-pty (FRI-601)

## Baseline

Base `ff9f39e` (main). The Rust tree is unchanged since `a74d823`, where the FRI-597 gates passed (`cargo clippy --all-targets -D warnings`, `cargo test`, `rustfmt --check`). FRI-588 (`e418695`) touched only frontend files; tsc clean and vitest 472 passed on it. No pre-existing failure recorded.

## Plan review

iprev with opus, 3 rounds (medium-risk cap). R1 REVISE (2 high, 3 medium, 4 low), R2 REVISE (2 medium, 2 low), R3 REVISE (2 medium on spec wording, 2 low). All findings applied; reviews in `handoff/iprev-r1.md`, `iprev-r2.md`, `iprev-r3.md`.

## Red (groups 1–3)

Test-writer (sonnet) wrote 22 tests in three new files, locked in `handoff/.red-tests`:
- `src-tauri/src/agents/liveness_tests.rs` (6): live set semantics, late hook registration.
- `src-tauri/src/pty_liveness_tests.rs` (10): reserve/release/kill/EOF liveness, older-PTY races, `has_session_pty`, `resume_block_state`.
- `src-tauri/src/mcp/liveness_tests.rs` (6): directory `is_live`, markerless killed session, not-live descriptor, `inactive_target`, resume `pending`, `resume_blocked_reason` matrix.

Edits to existing tests (not lockable, they live inline in production files): the 8 reservation-test call sites in `pty.rs` updated to the new signatures with no assertion change; `make_live` (`mcp/directory.rs`) and `add_live_session` (`mcp/messaging.rs`) switched from `register_session` to `mark_live`.

Red proof: `cargo test --no-run` fails with 54 errors, all missing items or changed signatures (E0599 `mark_live`/`mark_dead`/`mark_dead_if`/`has_session_pty`/`resume_block_state`; E0425 `kill_session_pty_inner`/`on_agent_pty_eof`; E0432 `resume_blocked_reason`; E0061 `reserve_session_pty`/`release_session_pty` arity).

Visibility the Builder must provide: `resume_blocked_reason` at least `pub(crate)`; `mark_live`/`mark_dead`/`mark_dead_if` `pub`.

## Green (groups 1–3)

Builder (sonnet) implemented tasks 1.3, 2.2–2.5, 3.2–3.4 with no deviation from `implementation.md`. Orchestrator edits after review: corrected the `resume_blocked_reason` doc (the revive would spawn nothing, not double-spawn) and trimmed a WHAT doc on `mark_live`. `mark_dead` has no production caller (kill uses `mark_dead_if` with the removed id per design D3); it stays because design D1 defines it and a locked test uses it.

Gates (run by the orchestrator, base `main`):
- `rustfmt --edition 2024 --check` on the 6 changed Rust files: OK.
- `cargo clippy --all-targets -- -D warnings`: no issues.
- `cargo test`: 1203 passed, 1 ignored (5 suites).
- `work-gates.sh main session-liveness-from-pty`:

| Gate | Result | Detail |
|------|--------|--------|
| 5. Deps | SKIP | no manifest or lockfile changed |
| 6a. Placeholders | PASS | none |
| 6b. Bar weakening | PASS | none |
| 6c. Secrets | PASS | gitleaks clean |
| 2. Red locks | PASS | 3 locked file(s) unchanged |

## Review follow-ups (group 4)

The adversarial review (opus) returned PASS with 3 Important Compliance findings and 5 nits (`handoff/REVIEW.md`). The user confirmed all 4 follow-up groups (identity, warmup spec, soft-close test, text nits).

- Artifacts: MODIFIED deltas for `agent-spawned-worktrees` (`request_session_resume` delivery queues from the PTY reservation) and `nergal-mcp-server` (identity validated against the live set); proposal and design D2/D6 corrected.
- Red (test-writer, sonnet): `agents/identity_liveness_tests.rs` (4 tests, 2 failing by assertion before the fix), `pty_softclose_tests.rs` (1, coverage-only, passed already), `mcp/warmup_liveness_tests.rs` (1, coverage-only, passed already). Locked in `.red-tests` (6 files). Inline identity tests in `agents/state.rs` gained `mark_live` with no assertion change.
- Green (Builder, haiku): `resolve_session_hint` resolves only live sessions; MCP instructions describe not-live descriptors; `inactive_target` hint and `send_to_session` description point to `request_session_resume`. The orchestrator fixed two more leftover "revive via create_worktree_session" strings (`mcp/mod.rs` `search_sessions` description, `mcp/messaging.rs` `messageable=false` note).
- Not acted on: the millisecond `pty_id` collision nit (pre-existing, out of scope); `has_session_pty` / `mark_dead` without a production caller (both kept: design D1/D5 define them and locked tests use them).

Gates:
- `cargo test`: 1209 passed, 1 ignored (5 suites).
- `cargo clippy --all-targets -- -D warnings`: no issues.
- `rustfmt --edition 2024 --check` on the 6 changed Rust files: OK.
- `tsc --noEmit`: no errors.
- `work-gates.sh`: 5 SKIP (no manifest), 6a/6b/6c PASS, Red locks PASS (6 files unchanged).

## Walk round 1 (user, 2026-11-23)

C1 OK, C2 OK, C4 OK, C5 OK. **C3 FAIL**: after exiting the agent and the shell in an open tab, `list_sessions` still showed `is_live: true`. Root cause not confirmed yet: the agent PTY end-of-file needs every holder of the PTY slave to close, so a leftover background process would keep it open. The MCP children of `claude` (`nergal`, `fff-mcp`) were checked and hold sockets, not the PTY. Waiting on a live repro to inspect the process tree.

The user's test agent also reported:
- A delivered but unprocessed message is lost when the target closes. Out of scope; added to the FRI-500 vault note.
- `last_activity` went back to the creation time and `last_assistant_message` turned null after the close. Caused by this change. User decision: a not-live descriptor keeps past facts (`last_activity`, `last_assistant_message`, `recently_touched_files`) and drops present claims (`mode`, `waiting_for`, `background_tasks`, `session_crons`).

**Red conflict ruling:** the locked test `a_not_live_descriptor_uses_the_persisted_mode_and_empty_live_only_fields` encoded the old decision. Test wrong by user decision: the test-writer rewrote it as `a_not_live_descriptor_keeps_past_facts_and_drops_present_claims` (failed by assertion, `last_activity` 0 vs recorded) and the file was re-locked (`b88eb55`).

Green (Builder, haiku): `descriptor_from` and the MCP instructions text follow the decision. `cargo test` 1209 passed, 1 ignored; clippy clean; rustfmt OK; 6 locks unchanged.

## Walk round 2 (user, 2026-11-23)

- C3 re-run with both steps (`/exit` in claude, then `exit` in the shell, tab left open): the agent shell is a zombie with no other holder of the PTY, `list_sessions` reports `is_live: false`, and approving `request_session_resume` fails with the close-the-tab reason. PASS. The first C3 run only exited claude, so the shell kept the PTY alive: live by design.
- D1 PASS (`last_activity` keeps the last value after the close), D3 PASS (`is_live: false`, `waiting_for` null, empty background work).
- D2 PASS after the session ran a turn in this app run. The first attempt returned `last_assistant_message: null` because the `.deb` install restarted Nergal and the runtime maps were empty for that session, the degraded case the spec documents.
- Out-of-scope findings filed: FRI-602 (resume with a relayed message breaks the zsh launch command); FRI-500 vault note gained "mensaje entregado que nadie procesa" and "mensajes encolados en una sesión recién reanudada".
