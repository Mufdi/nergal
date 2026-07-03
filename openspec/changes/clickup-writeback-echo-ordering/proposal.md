## Why

ClickUp's own-echo suppression records a write only **after** the API call succeeds, leaving a window where the 45s poll lands between the server applying the write and the command recording it — the poll then sees the new server value with no matching registry entry and misclassifies the user's own write as a remote change (spurious self-notification, observed on the "assign to me" flow). Linear already solved this: it records **provisionally before** the API call (`src-tauri/src/linear/mod.rs:984-991` — "Provisional record BEFORE the API call") and its registry doc mandates it (`src-tauri/src/linear/writeback.rs:87-89`: record before, `clear_entry` on failure). ClickUp's four write paths all record after: `clickup_set_task_status` (`src-tauri/src/clickup/mod.rs:538`, after the await at 535-537), `clickup_set_checklist_item` (560), `clickup_update_task` (602/620/628 — description/assignees/due), `clickup_set_custom_field` (679), and the closure flow (`src-tauri/src/clickup/closure.rs:313`).

## What Changes

- **Move each `registry.record(...)` before its `client.*(...).await`** across the ClickUp write commands and the closure status write, mirroring Linear's shape. For `clickup_update_task`, the conditional records (only for fields present in the update) move above the single `cl.update_task` await, still conditional.
- **Clear the provisional entry on API failure** via the existing `WritebackRegistry::clear_entry` (`src-tauri/src/clickup/writeback.rs:144`), exactly as Linear's contract prescribes — a failed write must not suppress a real remote change that happens to land the same value.
- No registry/schema changes: `record` (`writeback.rs:105`) and `clear_entry` already exist.

## Capabilities

### New Capabilities

_None._

### Modified Capabilities

- `clickup-writeback`: the echo-dedup requirement's recording rule changes from "after a successful write" to "provisionally before the API call, cleared on failure".

## Impact

- **`src-tauri/src/clickup/mod.rs`**: 5 record sites reordered (538, 560, 602, 620, 628, 679 — the update_task trio counts as one site block) + failure-path `clear_entry` calls.
- **`src-tauri/src/clickup/closure.rs`**: the status record at 313 moves before its `client.set_task_status` await (the closure already computes `pre` beforehand).
- **Risk**: MEDIUM→LOW — mechanical reorder copying a proven sibling pattern; the one semantic addition is clear-on-failure, which Linear's doc already specifies and tests.
- **Out of scope**: comment posting (has its own uncertain-outcome protocol — `clickup-writeback` spec "Comments are posted once" requirement); Linear (already correct); the poller's echo-check logic itself (unchanged).
