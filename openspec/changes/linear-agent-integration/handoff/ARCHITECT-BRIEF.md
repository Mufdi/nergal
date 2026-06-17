# Architect brief — linear-agent-integration

**Project mission**: Nergal — a Linux desktop wrapper for the Claude Code CLI (Tauri 2 + React 19). The agent CLI runs in a real PTY; React panels mirror state via Jotai atoms fed by the hook pipeline and transcript watchers. Nergal runs *around* the agent, augmenting the agent↔human loop — it never reimplements the agent's primitives.

## What this change is

Change #2 of 3 sequential Linear changes. `linear-mirror` (#1, feature-complete) made issues readable; this makes them **actionable** by the agent: three verbs (send-as-prompt, spawn-worktree-with-issue, attach-as-context), a 1:1 session↔issue binding, plus N pinned context issues. It rides the existing context-injection machinery (`assemble_injected_context` + the adapter contract) — no new injection plumbing.

## Control metadata (from .work-modules.json)

- tier **L**, ceremony **deep**, risk_tier **critical**
- files_estimate **12**, tags **[migration, security, feature]**, visibility **private**
- spec_target **linear-agent-integration**

## The one thing that makes this low-risk

It is a **near-1:1 mirror of `clickup-task-integration`** (archived, iterative-plan-review APPROVED across 5 rounds). Every Decision (1–7), the no-send-gate stance (ClickUp Revision 3), the fence framing (ClickUp Revision 2, "team brief" not "untrusted"), and the delivery mechanics are inherited. The primitives (`paste_to_session`, `sanitize_for_pty`, `queue_session_prompt`, the binding-column db pattern, the assembler) were all extracted by the ClickUp change and are reused as-is.

## Linear-specific deltas (the actual design work)

1. **Simpler composition** — Linear has no checklists / custom fields; attrition is 3-stage (comments → sub-issues → description).
2. **Attachments + relations excluded** — not mirrored (detail-only live fetch); mirror-only contract forbids a live call at compose time.
3. **Comments table present but poller-unpopulated in #1** — compose reads it (empty today, forward-compatible).
4. **Direct SQL composer** — `mirror.rs` has no per-section read helpers; compose queries the tables directly (matching ClickUp's core-row style).
5. **Slug/name from the issue title**; the identifier rides in the composed heading.

## Dependencies / blockers

- Depends on `linear-mirror` (mirror + panel) — done, on `main`.
- Independent of the context-bridge MCP changes.
- Stays in **dev** (no release/build/install this cycle — user decision 2026-06-17).

## Gating decision

iterative-plan-review **ON** (risk critical + migration tag), scoped to the Linear-specific deltas + a sanity pass on the inherited mechanics as instantiated for Linear. Build-phase: single-sequential reviewer with security escalation on the composer/PTY-delivery diff.

## Lazy-skill check

No frontend-redesign or new-domain skill applies — this rides existing panels and the established ClickUp UX patterns (canonized in `docs/patterns.md §9-13`). No SKILL.md to fold in.
