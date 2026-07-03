## 1. Fix the double-write

- [x] 1.1 `blocking_fifo_read_liveness_aware` (`hooks/cli.rs`) — remove the `output_deny(...)` call at the wall-clock backstop (`cli.rs:348`); move its user-facing message ("Plan review timed out — please resubmit") into the returned sentinel's `message` field. Keep the `tracing::warn!(ipc_event = "dead_peer_deny", ...)`.
- [x] 1.2 Same function — remove the `output_deny(...)` at the dead-GUI branch (`cli.rs:378`); carry its message ("Nergal GUI is no longer running — plan review cancelled") in the sentinel.
- [x] 1.3 Windows twin `blocking_pipe_read_liveness_aware` — apply the identical return-only treatment to its backstop/dead-GUI branches.
- [x] 1.4 Confirm `plan_review()` (`cli.rs:249-266`) remains the sole writer: sentinel with `approved:false` → single `output_deny(message)`; real decision → single `output_allow`/`output_deny`. No caller change expected.

## 2. Tests

- [x] 2.1 Extract the sentinel→stdout-decision mapping into a pure helper if not already; unit-test that a wall-clock sentinel and a dead-GUI sentinel each yield exactly one serialized `hookSpecificOutput` object (no concatenation).
- [x] 2.2 Assert the deny message text is preserved end-to-end from sentinel to the single write.

## 3. Verification

- [x] 3.1 `cd src-tauri && cargo clippy -- -D warnings && cargo test && cargo fmt --check`.
- [ ] 3.2 Manual (Linux): trigger a plan review, let the wall-clock backstop fire (or simulate) → confirm the agent receives a single well-formed deny and does not hang; kill the GUI mid-review → confirm one clean deny.
  - 3.2 pending: single-writer proven by construction + unit tests; live backstop/GUI-kill walk on a future dev session. Also fixed (same pattern): the Windows foreign_principal branch, see REVIEW.md.
