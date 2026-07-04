# Implementation Plan: issue-tracker-adapter (spike)

> Grounded in current codebase, symbols verified 2026-07-02. This change is documentation-only: the "implementation" below is the spike's execution plan — its deliverables are design.md artifacts, never source edits.

## Verified codebase facts (do not re-assume)

- Both stacks have **identical module lists**: `src-tauri/src/{clickup,linear}/` each contain `auth.rs`, `client.rs`, `closure.rs`, `integration.rs`, `mirror.rs`, `mod.rs`, `model.rs`, `poller.rs`, `writeback.rs` (clickup additionally `fixtures/`). Sizes are same-order (e.g. mirror 42.3K vs 48.5K; writeback 28.9K vs 19.5K; poller 76.4K vs 26.1K — the poller asymmetry is the first thing the inventory must explain).
- **Writeback registries are parallel but not identical**: ClickUp `WritebackRegistry::record` (`clickup/writeback.rs:105`) + `clear_entry` (`clickup/writeback.rs:144`); Linear's contract doc (`linear/writeback.rs:87-89`) mandates provisional record BEFORE the API call — ClickUp records after (being fixed by the pending `clickup-writeback-echo-ordering` change). Divergence direction: Linear is the later, corrected pattern (design D3).
- **Frontend duplication is real but non-mechanical**: `src/components/clickup/` (10 files incl. `ClickUpPanel.tsx` 53.5K, `ClickUpTaskView.tsx` 58.3K) vs `src/components/linear/` (8 files incl. `LinearPanel.tsx` 61.4K, `LinearTaskView.tsx` 60.1K) — same shapes, different field vocabularies. Out of v1 scope (design D1).
- **Adapter precedent exists in-repo**: the `AgentAdapter` trait (`src-tauri/src/agents/mod.rs`, spec `openspec/specs/agent-adapter/`) already abstracts 4 agent CLIs behind one trait with capability bitflags + default methods — the pattern (trait + registry + capability gating) is proven in this codebase and is the template for the sketch.
- **Four openspec capabilities per tracker**: `clickup-{mirror,task-panel,writeback,agent-integration}` and `linear-{mirror,task-panel,writeback,agent-integration}` — the spec-architecture question (shared capability + per-tracker deltas?) is an explicit open question, not assumed.
- **DB seams**: mirror tables live in per-tracker migrations (`015_clickup_mirror.sql`, `023_linear_mirror.sql`); mirror read paths are per-tracker (`clickup/mirror.rs:390 read_tasks`). Any shared-adapter data layer must respect that the schemas already shipped and cannot be unified retroactively without rebuilds — the paper-migration must treat table names/shapes as tracker-specific inputs, not unifiable.

## Execution order

1. **Duplication inventory** (tasks 1.1): side-by-side diff of the nine module pairs; output = one table per module classifying each public fn: `shared-shape` / `parametrizable` / `tracker-specific`. Explain the poller size asymmetry (76K vs 26K) explicitly — if ClickUp's poller carries logic Linear solved elsewhere, that's prime extraction evidence.
2. **Trait sketch** (tasks 1.2): Rust sketch in design.md modeled on the `AgentAdapter` shape — associated types (`Task`, `State`, `FieldValue`), method groups (mirror reconcile hooks, poll cadence + echo registry, closure token flow, keyring auth), capability flags for optional surfaces (checklists, cycles, estimates). No compile requirement.
3. **Paper-migration of ClickUp** (tasks 1.3): map every ClickUp module fn onto the sketch; list each abstraction leak + its resolution (extension point vs tracker-specific). Sanity-check each trait method against a hypothetical GitHub Issues adapter on paper (cheap third sample).
4. **Go/no-go** (tasks 1.4): cost estimate (files touched, call-site churn, test surface) vs measured duplication + projected third-tracker cost; recommendation appended as `## Revision 1: spike findings` in design.md.
5. **Decision gate** (tasks 2.1): user decides; GO → seed follow-up build changes via /work; NO-GO → the inventory table becomes the manual-sync checklist and the change archives as a recorded decision.

## Per-phase risk

- [Inventory scope explodes (nine pairs × dozens of fns)] → classify at fn granularity but summarize at responsibility granularity; time-box ~1 day total for phases 1-4.
- [Trait sketch over-fits the two samples] → the GitHub-Issues paper check in phase 3 is mandatory per method, not optional.
- [Spike drifts into refactor] → hard rule restated: any change outside `openspec/changes/issue-tracker-adapter/` fails the change's own verification task (3.1).
- [Pending sibling changes shift the ground truth (`clickup-writeback-echo-ordering` reorders records)] → run the inventory AFTER that change lands, or mark affected rows "post-fix" explicitly.

## Verification

- `openspec validate issue-tracker-adapter --strict` green with the revised design.md.
- `git status` clean outside `openspec/changes/` (documentation-only invariant).
- No build/test commands apply (no source edits by design).
