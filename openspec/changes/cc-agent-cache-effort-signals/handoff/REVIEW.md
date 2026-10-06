---
type: review
feature: cc-agent-cache-effort-signals
reviewed_at: 2026-10-05
verdict: PASS (after fixes)
---

# Review: cc-agent-cache-effort-signals (FRI-501)

## Plan review (iterative, Claude evaluator, opus)

- Round 1 REVISE: no clearing path for `agent_needs_input`; duplicate Agent + Subagent rows; missing `AgentStatus` literal sites; drop point vague; cause table undefined; effort mapping unstated; tooltip wording; null vs omitted keys. All fixed.
- Round 2 REVISE: `agent_completed` cleared every attention source. Fixed with source-scoped clearing (atom value + `background agent` waiting_for prefix). Agent rows closed by the any-name fallback: fixed.
- Round 3 APPROVED. Nit (last writer wins on the attention slot) recorded under design Risks.

## Spec reviewer (sonnet)

SPEC: PASS. No divergence from the four delta specs or tasks 1.x–4.x.

## Frontend reviewer (sonnet)

- 🟡 Cache chip carried meaning by icon only → fixed: `aria-label` on both chip triggers.
- 🟡 Chips could wrap at narrow widths → fixed: `shrink-0 whitespace-nowrap`.
- 🟡 Width jitter of the percentage → fixed: `inline-block min-w-[2.25rem] text-right tabular-nums` (copied from the context chip).
- 🟢 Tooltips via `Tooltip` (no native `title`), sky-400 encoded in design.md §1.2, no spinners.
- Nit: empty `notification` case → kept, with a WHY comment (it prevents the generic default entry).

## Adversarial reviewer (opus)

Pass 1 Bugs
- Important: Agent/Task rows stayed running forever once excluded from the fallback → fixed: Stop closes unclaimed Agent/Task rows (design Revision 2).
- Important: foreground PostToolUse/Stop/PreToolUse erased a background wait (tint + MCP) → fixed: background waits survive foreground turn events (design Revision 1, new spec scenarios).
- Nit: explicit `null` `last_miss_causes` failed the whole `AgentStatus` → fixed: null-tolerant deserializer + test.
- Nit: parallel Agent calls of different types can swap labels → accepted, recorded under Risks.
- Nit: Stop scan can land after a quick prompt submit → accepted, recorded under Risks.

Pass 2 Security
- None. Payload text renders as React text; no new paths from payloads.

Pass 3 Compliance
- Nit: background-wait detection differs by layer (prefix vs type), last writer wins with several agents → accepted, recorded under Risks.
- Nit: `assistant_effort_in_line` duplicated the scan parse → fixed: both use `assistant_effort`.

## Retro (repeat-mistake rule)

The five newest archived REVIEW.md files (2026-07-04 batch) carry no Important finding, so nothing repeats. No CLAUDE.md line proposed.
