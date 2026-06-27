# Review — platform-ipc

_Reviewers write here during Mode B. Empty until execution._

## iprev RUN 2026-06-26 (platform-ipc, skeptic)

- **Evaluator:** `claude` (CLI `claude -p`) · **persona:** skeptic (adversarial, hostile-local-user + production-scale; severities escalated) · **mode:** Mode A plan-artifact review (no source touched) · **rounds:** 5 (cap) · **final verdict:** APPROVED (round 5).
- **Validation:** `openspec validate platform-ipc --strict` passed after every revision round.

### Issue counts per round
| Round | critical | high | medium | low | verdict |
|---|---|---|---|---|---|
| 1 | 1 | 5 | 5 | 2 | REVISE |
| 2 | 0 | 2 | 3 | 1 | REVISE |
| 3 | 0 | 2 | 2 | 2 | REVISE |
| 4 | 0 | 1 | 2 | 3 | REVISE |
| 5 | 0 | 0 | 0 | 3 (residual) | APPROVED |

### Critical / high callouts (and resolutions)

**R1-#1 (CRITICAL) — unified blocking primitive had ZERO peer-auth.** The plan-review/ask-user FIFOs (which gate the agent-spawned-worktree human approval) lived in world-writable `/tmp` under a guessable PID-derived name with no peer check — a different-uid local process could write a forged `allow`. Fixed: owner-only boundary on the blocking primitive via the per-user IPC dir + documented the FIFO gap.

**R1 highs** — no automated cross-uid test (the only MCP access control could be silently dropped by the refactor); the false "SO_PEERCRED works on macOS" claim (macOS is `LOCAL_PEERCRED`/`getpeereid`, no peer PID); hook socket authenticated by file mode alone with a create→chmod TOCTOU; stale-socket squat DoS in sticky `/tmp`; deferred Windows loopback fallback unconstrained. All addressed (cross-uid test, corrected peer-cred semantics, hook-socket peer-uid backstop, per-user dir, constrained-loopback requirement).

**R2-#1 (HIGH) — the per-user `0700` dir RELOCATED the squat DoS** rather than killing it (a hostile `mkdir` of the predictable `nergal-<uid>` leaf under sticky `/tmp` forces a permanent refuse-to-bind). Fixed: root the dir at an un-squattable base — `$XDG_RUNTIME_DIR` → later `/run/user/<getuid()>` — not `temp_dir()`.
**R2-#2 (HIGH) — mock test couldn't verify the macOS `LOCAL_PEERCRED` extraction** yet a task claimed it did. Fixed: split into a CI comparison-branch mock test vs a privileged real-foreign-uid extraction harness.

**R3-#1 (HIGH) — liveness-aware deny specified a connection-close signal that does not exist on the connectionless FIFO** the iteration ships. Fixed: `gui.pid` + non-blocking open + `kill(pid,0)` poll.
**R3-#2 (HIGH) — single resolver keyed off `$XDG_RUNTIME_DIR` env var, which Codex strips from the MCP shim** → bind/connect path divergence breaks MCP. Fixed: key the resolver off `getuid()`, `getpwuid()` home fallback (not `$HOME`).

**R4-#1 (HIGH) — the `0700` dir blocks the `nobody` connector at the filesystem before `peer_cred()` runs**, so the one load-bearing macOS security test would FALSE-GREEN (an `EACCES` at connect read as "boundary rejected"). Fixed: a2 binds a dedicated test socket in a deliberately-traversable parent, forces the peer to reach `accept()`, and distinguishes `EACCES`-at-connect/`EACCES`-at-exec/missing-sudo from a genuine post-accept rejection (→ UNVERIFIED-pending, never a pass). Also: PID-reuse hole in `gui.pid` (added start-time token); retained XDG override dropped entirely; FIFO poll cadence pinned (~1s timeout, ignore POLLHUP).

### R5 residuals (LOW, non-blocking — captured as contract constraints)
1. `gui.pid` written only by the `flock`-holding instance (else a launch-race sibling causes a spurious-but-safe deny). **Landed** in design + spec.
2. CI assertion that endpoints live inside the per-user dir (parent not group/world-writable) so a silent revert to `/tmp` fails CI. **Landed** as task 5.1b.
3. start-time-token tick-granularity residual — one-sentence note that the human-scale backstop is the final bound. **Landed** in design.

### Substantive security revisions (net effect on the plan)
- **Root-cause hardening, not a patch:** introduced a per-user IPC runtime directory rooted at an un-squattable base (`/run/user/<getuid()>` on Linux, `getpwuid` home fallback, `temp_dir()` on macOS), keyed off `getuid()` so the env-stripped Codex shim still resolves the same path. This single decision closes the squat-DoS, the hook-socket TOCTOU, the forgeable-FIFO approval gate, and the `TMPDIR`/payload-tear findings together.
- **Boundary now verified, not asserted:** a CI comparison-branch test (mock) guards the uid `!=` check from a silent refactor drop; a committed privileged `sudo -u nobody` extraction harness (against a relaxed-perm test socket, staged binary) validates the real macOS `LOCAL_PEERCRED` semantic, with explicit UNVERIFIED-pending bucketing for un-runnable harnesses. Hook socket gained a peer-uid backstop.
- **Blocking approval gate hardened on the shipping FIFO path:** owner-only via the dir, liveness-aware deny via `gui.pid` (pid + start-time token, `flock`-owned) with a timer-driven non-blocking poll, RAII/pre-seed guard, human-scale backstop — fails toward deny, never hangs, never auto-denies live deliberation.
- **Deferred Windows contract constrained** so the future body cannot silently weaken the boundary: named-pipe recommended; loopback fallback requires ≥128-bit CSPRNG token, non-world-readable/non-inspectable delivery, constant-time comparison, reject-before-dispatch.
