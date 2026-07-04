## Why

Follow-up 3 (the deferred "compose slice-4" from `extract-tracker-shared`) asked whether the ClickUp/Linear `fit_to_budget` context-attrition framework can be shared. Re-evaluation of the actual code (`clickup/integration.rs` vs `linear/integration.rs`):

- **`fit_to_budget` orchestration — deliberately NOT shared (confirmed leaky).** The two attrition loops are structurally similar but control-flow-divergent: ClickUp runs 4 stages (comments → checklists → subtasks → description), Linear runs 3 (comments → subissues → description); they operate on different item types (`ComposedTask`/`ComposedIssue`) with different field names (`date`/`created_at`, `subtasks`/`subissues`). A shared abstraction would need 4-5 injected closures re-exposing each struct's internals plus a per-tracker `render` — net-negative readability, the exact anti-pattern the spike rejected. The `issue-tracker-adapter` spec already records this; this change refines it.
- **`render` / `neutralize_fence_sentinels` — NOT shared.** `render` is fully divergent markdown; `neutralize_fence_sentinels` hardcodes per-tracker fence sentinels (parametrizing a 2-line fn is not worth it).
- **`head_tail_truncate` + `DESCRIPTION_TRUNC_MARKER` — cleanly shareable (the sub-win the coarse slice-4 cut missed).** Verified **byte-identical** (`cmp`, 864 bytes) across both files, plus an identical `head_tail_truncate_is_char_boundary_safe` test. Pure, type-agnostic, references only `std` + the shared marker const. Zero leak.

## What Changes

- **Extract** `head_tail_truncate` + `DESCRIPTION_TRUNC_MARKER` (and the shared char-boundary test) into a new `tracker_shared::context_budget` module; ClickUp and Linear call the shared fn and import the const. Removes ~40 lines of byte-identical duplication.
- **Record the non-extraction decision** for the `fit_to_budget` orchestration + `render` + `neutralize_fence_sentinels` as a spec refinement, closing the slice-4 question permanently so it is not re-opened.

## Capabilities

### New Capabilities

_None._

### Modified Capabilities

- `issue-tracker-adapter`: add the pure description-truncation helper to the shared-slices list, and refine the `fit_to_budget` note to distinguish the per-tracker *attrition orchestration* (not shared) from the *pure truncation helper it calls* (now shared).

## Impact

- **`src-tauri/src/tracker_shared/context_budget.rs`** (new): the shared fn + const + test.
- **`src-tauri/src/tracker_shared/mod.rs`**: declare the module.
- **`src-tauri/src/clickup/integration.rs`** + **`src-tauri/src/linear/integration.rs`**: delete the local copies, import from `tracker_shared`.
- No behavior change (byte-identical helper). **Risk**: LOW (pure-fn move). Verified by the existing per-tracker `fit_to_budget` tests + the consolidated char-boundary test.
- **Out of scope**: sharing `fit_to_budget`/`render`/`neutralize_fence_sentinels` — deliberately per-tracker (leaky), documented as the decision.
