# REVIEW — untrusted-context-boundary

## Escalated review (tag security, risk_tier medium): security + spec · sonnet · 2026-07-03

**Consolidated verdict: PASS** (after closing one HIGH found by security).

### security-reviewer (adversarial) — PASS after fix

Round 1: the `concat_context_blocks` fence mechanism itself is sound — crux collision
attack closed (both markers escaped as prefix matches, order-independent; verified by
hand, not just test names). But found **Finding 1 (HIGH)**: a SECOND injection surface,
`reinject_pinned_note` (the N2 hot-reload path), wrote the vault note body straight
into the LIVE PTY with only a `>` blockquote — no fence, no preamble, no neutralization.
Same untrusted source this change exists to protect; not enumerated in the proposal.
Zero collision engineering needed (no fence to escape). Findings 2 (case-variant markers)
and 3 (whitespace-split markers) = accepted defense-in-depth LOWs (design frames the
mitigation as non-hard-guarantee).

Fix (builder round 2): extracted `build_reinject_block(name, body)` =
`fence_external_block("vault", sanitize→(name + body))`; the live re-inject now gets the
identical fenced+neutralized boundary. Round 2 re-verify: **PASS** — same boundary as
spawn path, sanitize→fence order, name AND body inside the neutralized content, no new
bypass, crux test genuine. Orchestrator added `reinject_block_neutralizes_marker_in_note_name`
to close the one coverage gap the reviewer flagged (marker in the filename).

### spec-reviewer — PASS

All scenarios trace; central application confirmed (builders untouched — fence 100% in
`concat_context_blocks`, a 4th source fenced by construction); labels correct;
empty-source → None preserved; task 3.4 collision test is a strong assertion (attacker
trailer proven enclosed); the 2 rewritten passthrough tests assert the new fenced
invariant, not weakened; single MODIFIED delta on `obsidian-context-injection` (Decision 4).

## Scope expansion (HIGH-driven)

The reinject path was not in the proposal's Impact list — the HIGH brought it into the
change's real scope (same untrusted source, same capability `obsidian-context-injection`
which governs the re-inject per its N2 requirement). The spec delta was extended with a
"live re-inject is also fenced" requirement clause + scenario to match implemented reality.

## Gates

- Gate 1-3: PASS (clippy -D warnings clean, 760 tests incl. 6 new fence/reinject, fmt
  clean, tsc clean).
- Gate 4 (security): the change IS the control; security-reviewer escalation ran, HIGH
  closed + re-verified.
- Gate 6 (scope): 1 file (pty.rs) — the reinject expansion stayed in-file.
