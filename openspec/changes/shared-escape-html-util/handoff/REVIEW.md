# REVIEW — shared-escape-html-util

## Reviewer: security-reviewer (single, sonnet) · 2026-07-03

**Verdict: PASS** — zero blocking findings.

- **escapeHtml completeness**: 5 chars (`& < > " '`, `&` first) fully neutralize the
  `dangerouslySetInnerHTML` sink; `<img src=x onerror=…>` → inert escaped text (tested).
  `"`/`'` included defensively for future attribute-context reuse. Backtick/`=` not HTML
  metacharacters — not a gap.
- **All sites covered**: reviewer independently grepped ALL 9 confirm bodies in src/
  (Sidebar ×2, StatusBar ×1/2-branches, SettingsPanel ×2, clickup ×1, linear ×1,
  PlanPanel ×2, SpecPanel ×2, KeymapSection ×1, TasksIsland ×1) — every caller-name
  interpolation escaped. Each allowlist entry verified: `name(current)`/`name(taskId)`
  = clickup's local helper that wraps escapeHtml internally (confirmed by reading it);
  `sessionsLine`/`info.pid`/`annotations.length`/pluralization = numeric/pre-built safe.
- **5th site (StatusBar `info.label`, Docker container name from local `docker ps`)**:
  same trust class + sink, correctly escaped in-change (found by the scan during dev).
- **Dedup**: `git show HEAD:` confirms both deleted local copies were byte-identical to
  the shared util — zero behavior change.
- **Honest scoping**: `ConfirmHost.tsx` is the ONLY raw-HTML sink (grepped
  dangerouslySetInnerHTML/innerHTML/srcDoc); `TranscriptViewer` uses ReactMarkdown with
  NO rehypeRaw → safe-by-default, correctly untouched.

## Scan blind spots (Low, defense-in-depth) — one closed, two documented

Reviewer flagged 3 scan evasions (none present today). Orchestrator response:
1. **Partial escape** `${escapeHtml(a) + rawVar}` — CLOSED: the check is now
   `isSingleEscapeHtmlCall` (whole expr must be one top-level escapeHtml call, paren
   closes at expr end), + a regression test.
2. Non-template-literal body (string concat / plain var) — documented as a known limit.
3. Third `confirm` import alias — documented as a known limit.
Both remaining require an AST pass; noted inline in the test as a follow-up if a site
ever needs it.

## Gates

- Gate 1-3: PASS (tsc --noEmit clean, 57 vitest tests incl. escapeHtml units + the
  enforceable scan + partial-escape regression).
- Gate 4 (security): the change IS the control; security-reviewer ran, no bypass.
- Gate 6 (scope): 7 edited + 3 new files; the StatusBar 5th site is an in-scope
  expansion of the same threat.
