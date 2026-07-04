## Why

The `issue-tracker-adapter` spike (archived `2026-07-04-issue-tracker-adapter`) measured the ClickUp vs Linear duplication byte-for-byte and returned **GO-BUT-NARROWER**: ~17% of the 15,759 combined LOC is mechanically shared with zero paper-migration leaks, concentrated in 4 slices, while the poller/mirror/closure layers carry 3 real leaks that a shared contract would either paper over or freeze the in-flight `clickup-writeback-echo-ordering` fix into. This change extracts the profitable 4 slices so a fix on genuinely shared mechanics (an echo-suppression bug, a keyring-fallback bug) lands once for both trackers instead of twice — the exact class of drift that made `clickup-writeback-echo-ordering` a separate change from its Linear counterpart.

## What Changes

- **New shared module `src-tauri/src/tracker_shared/`** absorbing 3 mechanically-shared slices (built against Linear's shape first per spike D3, then ClickUp migrated onto it):
  - `WritebackRegistry<F>` — generic own-echo registry (record/entries_for/clear_entry/purge_expired/tracked_ids + `check_echo`), parametrized over each tracker's `WriteField` via a `WriteFieldClass` trait that preserves ClickUp's real Scalar/Additive split (not dropped to Linear's all-scalar). `WriteConflict` stays per-tracker (its serde id key is on the event wire) — `check_echo` returns neutral data mapped to each tracker's conflict struct at the emit site. `WRITE_TTL` is a construction param, not a cross-module `use` (avoids an import cycle).
  - `CredentialStore` — keyring + atomic 0600 fallback-file store/load/clear, parametrized over service/account/filename. **The on-disk fallback field carries `#[serde(alias = "token", alias = "key")]`** so both trackers' existing `~/.config/nergal/{clickup,linear}.toml` files still deserialize after upgrade (they use different legacy key names — a verbatim merge would silently drop the losing tracker's stored credential). `AuthMode`/`authorization_header_value` and Linear's multi-workspace namespacing stay OUTSIDE it as tracker-specific wrappers.
  - closed-out marker trio (`mark_closed_out`/`read_closed_out`/`unmark_closed_out`) as SQL-templated free functions parametrized by table name (adding a deterministic `ORDER BY` to both — verified order-insensitive at the consumers).
- **Migrate ClickUp + Linear onto the shared module** at their existing call sites; delete the now-duplicated per-tracker copies. Net ~500-700 LOC reduction.
- **Behavior-preserving**: no tracker behavior, DB schema, on-disk credential format, or public command surface changes — this is an internal deduplication. Not **BREAKING**.
- **Explicit non-goals** (deferred): the compose/budget framework (`ComposableItem`/`fit_to_budget`) — its `fit_to_budget` is control-flow-divergent (ClickUp has a checklist-collapse stage Linear lacks), so a shared trait would be leaky (iprev round 1 cut it up front); the full 5-layer `IssueTrackerAdapter` trait spanning poller/mirror/closure; frontend panel sharing; a third tracker.

## Capabilities

### New Capabilities

_None._

### Modified Capabilities

- `issue-tracker-adapter`: narrow the contract from the full mirror+poller+closure+auth adapter (spike-rejected) to the 4 mechanically-shared slices actually extracted, and record that the go decision landed (the extraction gate is satisfied). The "cross-tracker fix lands once" guarantee now binds on the shared slices; the "third tracker is a fill-in job" scope is corrected to "the shared slices are reused; a third tracker still writes its own poller/mirror/client/model."

## Impact

- **New**: `src-tauri/src/tracker_shared/` (mod + the 3 slices, ~500-600 LOC) + its tests (consolidating the parallel registry/credential-store/closed-out test modules that both trackers duplicate today).
- **Modified**: `src-tauri/src/clickup/{auth,writeback,mirror}.rs` and `src-tauri/src/linear/{auth,writeback,mirror}.rs` — call sites rewired to the shared types; duplicated bodies removed. (`integration.rs` NOT touched — compose slice deferred.)
- **Security-touching**: `CredentialStore` consolidates keyring + 0600 fallback-file code that was security-reviewed per-tracker (`clickup/auth.rs`, `linear/auth.rs`) → this change carries a security review (the redaction `Debug` impls, atomic temp+rename, and 0600 perms must survive the extraction unchanged).
- **Sequencing**: migration order WritebackRegistry → closed-out trio → CredentialStore → ComposableItem (spike migration plan). ClickUp's writeback migration lands only after `clickup-writeback-echo-ordering` (already archived Wave 2.10) is settled, so no in-flight ordering fix is frozen mid-extraction.
- **No DB migration**: closed-out tables already shipped; the trio operates on existing per-tracker tables via a table-name parameter.
