## 1. Spike (design deliverables only — no source edits)

- [x] 1.1 Duplication inventory: diff the nine module pairs (`src-tauri/src/{clickup,linear}/{auth,client,closure,integration,mirror,model,poller,writeback,mod}.rs`); classify each public fn as shared-shape / parametrizable / tracker-specific; record the table in design.md.
- [x] 1.2 Draft the `IssueTrackerAdapter` trait sketch (associated types + method set for poller/writeback/mirror/closure/auth hooks) in design.md — Rust sketch, no compile requirement.
- [x] 1.3 Paper-migrate ClickUp onto the trait; list every abstraction leak (checklists, custom fields, spaces-vs-teams) and its resolution (extension point vs tracker-specific surface). Sanity-check each trait method against a hypothetical GitHub Issues adapter (paper only).
- [x] 1.4 Write the go/no-go recommendation with cost estimate; append as a `## Revision 1: spike findings` section in design.md.

## 2. Decision gate

- [x] 2.1 Present the recommendation to the user; record the decision. **Decision (orchestrator, delegated by user 2026-07-03): GO-BUT-NARROWER.** The full 5-layer `IssueTrackerAdapter` trait is rejected (3 leaks — closure 2-vs-3-halves, echo-check ordering, poller completeness model — would force per-tracker overrides on nearly every method or freeze the in-flight `clickup-writeback-echo-ordering` fix into a cross-tracker contract). Extract only the 4 mechanically-shared slices (~17% of 15,759 LOC, zero paper-migration leaks): `WritebackRegistry<F>` generic, `CredentialStore` struct (auth core, excluding `AuthMode`/multi-workspace), the closed-out marker trio, and the `ComposableItem`/compose-budget framework. Follow-up build change seeded: `extract-tracker-shared` (L, security-touching → iprev), migration order WritebackRegistry → closed-out trio → CredentialStore → ComposableItem, built against Linear's shape first (D3) then ClickUp migrated onto it after `clickup-writeback-echo-ordering` settles.

## 3. Verification

- [x] 3.1 `openspec validate issue-tracker-adapter --strict` passes with the revised design.md; no source files were modified (git status clean outside `openspec/changes/`).
