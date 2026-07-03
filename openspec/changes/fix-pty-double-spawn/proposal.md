## Why

`start_claude_session` has a check-then-act race that can spawn two PTYs for one session and orphan one of them unkillably. The idempotency check (`src-tauri/src/pty.rs:526-533`) reads `session_ptys` under a lock that is dropped at the end of its block; `spawn_pty` then runs unlocked (546-556); the `session_ptys.insert` happens only after (558-562). Two concurrent `start_claude_session` calls for the same `session_id` both pass the check, both spawn, and the second insert clobbers the first pty id — leaving a live PTY that `kill_session_pty` (`pty.rs:1195`) can never find. The codebase already knows this hazard: `spawn_aux_shell` (`pty.rs:861-867`) check-and-reserves under one lock hold, with a comment naming exactly this failure ("the second insert would orphan the first as an unkillable zombie"), and rolls the reservation back on spawn failure (882-885).

## What Changes

- **Reserve the `session_ptys[session_id]` slot under a single lock hold before `spawn_pty`**, mirroring `spawn_aux_shell`: compute `pty_id` first, then in one lock block — if the key exists return the existing id (idempotent early-return, unchanged semantics), else insert the new `pty_id` as a reservation.
- **Roll back the reservation if `spawn_pty` fails** (remove the key, return the error), exactly like `spawn_aux_shell:882-885`.
- **Delete the now-redundant post-spawn insert** (558-562).
- A concurrent second caller now gets the first caller's `pty_id` back while that PTY may still be booting — same contract as today's early-return for an established session (callers already treat `StartClaudeResult.pty_id` opaquely; the ready-wait at 565 is per-call and tolerates an already-ready PTY).

## Capabilities

### New Capabilities

- `pty-session-lifecycle`: at most one PTY exists per session id — creation is idempotent under concurrency (slot reserved before spawn, rolled back on spawn failure), so every spawned PTY remains reachable by `kill_session_pty`. (The audit handoff suggested MODIFIED `session-directory`, but that spec covers the MCP directory tools; `terminal-wezterm` covers VT emulation; no existing capability owns PTY lifecycle — new per the "New only when none fits" rule.)

### Modified Capabilities

_None._

## Impact

- **`src-tauri/src/pty.rs`**: `start_claude_session` (514-570) — restructure check/insert into one reserve block; add failure rollback; delete the trailing insert.
- **Pairs with the pending `reap-pty-child-zombies` Band-A change**: an orphaned PTY is also an unreaped child; this change removes the orphan-creation path, that one reaps whatever still dies.
- **Risk**: MEDIUM — concurrency-sensitive code on the app's most critical path (session start). Mitigated by copying a pattern already proven in the same file, and by the unchanged external contract.
- **Out of scope**: making the second caller wait for the first's ready signal (today's early-return doesn't either); aux-shell/quake paths (already correct).
