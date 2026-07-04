# REVIEW — confirm-deep-link-session-spawn

## Escalated 3-parallel review (risk_tier critical) · sonnet · 2026-07-03/04

**Consolidated verdict: PASS** — after fixing one HIGH (code-quality) + one MEDIUM
(security) + three LOW/nits. All fixes verified: tsc clean, 71 vitest, clippy
`--all-targets` clean, cargo test green, fmt clean.

### code-quality-reviewer — FAIL → FIXED

**HIGH (real defect, was blocking)**: `handleOpenFile` gated only the workspace-creation
branch (`if (!workspace)`). A **known workspace with zero sessions** (normal state — "Add
workspace" creates no session) fell through to `create_session` + activate (spawns the
agent PTY) with NO confirmation — the exact silent-spawn class this change closes. The
builder's "brand-new workspace has no sessions → gate once above" reasoning was false for
a pre-existing session-less workspace.
→ **Fixed**: added a `confirmed` flag; the zero-session `create_session` branch now
probes + confirms (independent of the workspace-creation gate) when not already gated,
returning with zero backend calls on decline. New regression test
`gates session creation in a known workspace that has no session yet`.

Nits (both fixed): `is_dir` was probed but never consumed → now drives a "Path does not
exist" warning distinct from "Not a git repository"; dead `Clone` derive on
`WorkspaceProbe` removed (only Serialize needed).

### security-reviewer — PASS + 1 MEDIUM → FIXED

PASS on the vector: single entry point (`dispatchDeepLink`), all creation branches gated,
known-session focus doesn't spawn, Enter never proceeds (capture handler intercepts
before the button; only click / Tab+Space proceeds), probe side-effect-free +
fail-toward-warning, `resolve_repo_root` genuinely guarantees git in the open-file branch
(the skip is correct, not a lie), queue doesn't coalesce, decline creates nothing.
**MEDIUM (coverage gap)**: the `confirmBodyEscaping` scanner blind-spotted
`confirmDeepLinkSpawn` (body was object-shorthand + built in a separate `const` outside
the `confirm({...})` literal), so a future edit stripping `escapeHtml` here — the
highest-authority sink — would go uncaught. → **Fixed**: inlined the `body:` construction
directly into the `confirm({...})` call so the scanner covers it (verified: the scan
runs green over this site now).

### spec-reviewer — PASS

All requirements/scenarios hold; 3 routes gated, known-session ungated, D3/D4/D5
satisfied. Findings: #1 (LOW) orphaned `resolve_repo_root` doc-comment → **fixed** (moved
back to its fn); #2 (LOW/INFO) the zero-session ungated spawn — same as the cq HIGH,
**fixed**.

## Gates

- Gate 1-3: PASS (tsc clean, 71 vitest incl. the new zero-session gate test, clippy
  --all-targets clean, cargo test green, fmt clean).
- Gate 4 (security): the change IS the control; 3-parallel escalation ran; HIGH+MEDIUM
  closed + re-verified.
- Gate 6 (scope): the fixes stayed within the change's files.
