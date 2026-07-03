## 1. Reserve-before-spawn

- [ ] 1.1 `src-tauri/src/pty.rs` `start_claude_session`: move the `pty_id` computation (536-543) above the idempotency block; convert the read-only check (526-533) into check-and-reserve under one lock hold (early-return existing id; else insert `pty_id`), copying `spawn_aux_shell`'s block shape and hazard comment (861-867).
- [ ] 1.2 On `spawn_pty` error, remove the reservation and propagate (mirror `spawn_aux_shell:882-885`).
- [ ] 1.3 Delete the post-spawn `session_ptys.insert` (558-562).

## 2. Tests

- [ ] 2.1 Concurrency test (or loom-free deterministic approximation): two tasks race `start_claude_session` for one session id against a stubbed/fast spawn — assert single map entry and equal returned ids. If PTY spawning is too heavy for CI, test the extracted reserve/rollback logic behind a small helper.

## 3. Verification

- [ ] 3.1 `cd src-tauri && cargo clippy -- -D warnings && cargo test && cargo fmt --check`
- [ ] 3.2 Cross-platform invariant: no new `cfg` seam introduced (pure std sync primitives); confirm the `macos-cross-check` + `windows-check` CI jobs pass on the PR (CLAUDE.md cross-platform invariant).
- [ ] 3.3 Manual: open a session, spam-switch tabs / double-trigger session open rapidly; confirm one PTY per session (`ps` tree) and that closing the session kills it.
