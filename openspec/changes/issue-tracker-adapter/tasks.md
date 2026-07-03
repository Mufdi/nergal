## 1. Spike (design deliverables only — no source edits)

- [ ] 1.1 Duplication inventory: diff the nine module pairs (`src-tauri/src/{clickup,linear}/{auth,client,closure,integration,mirror,model,poller,writeback,mod}.rs`); classify each public fn as shared-shape / parametrizable / tracker-specific; record the table in design.md.
- [ ] 1.2 Draft the `IssueTrackerAdapter` trait sketch (associated types + method set for poller/writeback/mirror/closure/auth hooks) in design.md — Rust sketch, no compile requirement.
- [ ] 1.3 Paper-migrate ClickUp onto the trait; list every abstraction leak (checklists, custom fields, spaces-vs-teams) and its resolution (extension point vs tracker-specific surface). Sanity-check each trait method against a hypothetical GitHub Issues adapter (paper only).
- [ ] 1.4 Write the go/no-go recommendation with cost estimate; append as a `## Revision 1: spike findings` section in design.md.

## 2. Decision gate

- [ ] 2.1 Present the recommendation to the user; record the decision. If GO: seed the follow-up build change(s) (backend adapter + first-tracker migration) via /work. If NO-GO: archive this change with the inventory as the manual-sync checklist.

## 3. Verification

- [ ] 3.1 `openspec validate issue-tracker-adapter --strict` passes with the revised design.md; no source files were modified (git status clean outside `openspec/changes/`).
