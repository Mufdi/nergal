## Context

Two tracker stacks, mirror-image module lists, no shared code beyond incidental util reuse. The duplication is disciplined (Linear was consciously built "Linear-style" on ClickUp's patterns, then improved on them — e.g. provisional writeback recording that ClickUp still lacks), which means the shapes are *convergent by intent* — favorable conditions for extraction. The precedent is `AgentAdapter` (`agent-adapter` spec): the codebase already did this successfully for agent CLIs (4 adapters behind one trait).

## Goals / Non-Goals

**Goals:**
- A concrete `IssueTrackerAdapter` contract proposal grounded in the two real stacks (not speculative generality).
- A migration path for one tracker + go/no-go recommendation, cheap enough to decide without betting the codebase.

**Non-Goals:**
- Building the adapter now; unifying frontend panels now; committing to a third tracker.

## Decisions

### D1: contract shape — trait over the backend seam, not the UI

Extract where the duplication is mechanical: `poller` (cadence, tombstoning, echo-check loop), `writeback` (registry semantics — provisional record, clear-on-failure, TTL), `mirror` (reconcile lifecycle, stale marking), `closure` (token flow), `auth` (keyring storage shape). Leave per-tracker: `client` (REST vs GraphQL), `model` (field vocabularies), state semantics (cycles/estimates vs lists/priorities). Frontend stays out of scope v1 — component duplication is real but not mechanical (different fields, different pickers).

**Alternatives considered:**
- *Full-stack adapter including UI*: highest payoff, highest risk; UI genericity historically produces lowest-common-denominator panels. Deferred to its own decision after the backend proves out.
- *Do nothing until a third tracker is real*: cheapest now, but the third copy locks in triplication and the extraction cost grows superlinearly (three call-site sets). The spike prices this trade instead of assuming it.
- *Config-driven generic tracker (no per-tracker code)*: trackers differ in auth flows and pagination/rate-limit semantics enough that config-only is fantasy. Rejected.

### D2: spike protocol (time-boxed, ~1 day)

1. **Duplication inventory**: diff the nine module pairs; classify each fn as shared-shape / parametrizable / tracker-specific (table).
2. **Draft the trait** (Rust sketch, no compile requirement): associated types for Task/State/Field, methods for the poller/writeback/mirror hooks.
3. **Paper-migrate ClickUp**: map each ClickUp module onto the trait; list every place the abstraction leaks (e.g. ClickUp checklists have no Linear analog — extension-point or tracker-specific surface?).
4. **Go/no-go**: extraction cost estimate vs. measured duplication + expected third-tracker cost. Append findings + recommendation to this design.md (Revision section per project openspec rules).

### D3: Linear is the semantic canon

Where the two stacks disagree on a pattern (echo recording order, overlay reconcile), the adapter contract adopts Linear's version — it is the later, deliberately-improved implementation (project memory + the pending ClickUp echo-ordering fix confirm the direction of improvement).

## Risks / Trade-offs

- [Design concludes no-go and the work feels wasted] → the duplication inventory itself becomes the checklist for keeping the stacks manually in sync (directly useful — the echo-ordering bug would have been caught by it).
- [Trait designed against 2 samples over-fits] → require the paper-migration step to name, for each trait method, what GitHub Issues would plausibly do (a cheap third sample on paper, no code).
- [Spike scope creeps into refactor] → hard rule: no source edits under this change; anything compiling is out of scope.

## Open Questions

- Which tracker migrates first if "go" (D2 assumes ClickUp; the spike validates).
- Whether the four openspec capabilities per tracker collapse into shared + per-tracker delta specs post-extraction (spec architecture question for the follow-up).
