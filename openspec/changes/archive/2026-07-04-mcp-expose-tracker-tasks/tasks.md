## 1. Design review (this change's deliverable)

- [x] 1.1 User sign-off on design D3 scoping (workspace-scoped v1; cross-workspace deferred) and D1 unified-tool naming.
- [x] 1.2 Settle the two open questions (PR rollup-only v1; comment budget in `get_tracker_task`) and record answers in design.md.

## 2. Seed the build change (if accepted)

- [ ] 2.1 Create the follow-up build change via /work: tool defs + dispatch in `src-tauri/src/mcp/mod.rs`, mirror read mappers (reusing `clickup::mirror::read_tasks`-shape queries and the Linear equivalents), `mirror_updated_at` metadata, payload caps per design D4; sequence after `clickup-subdata-indexes` (index-backed reads).

## 3. Verification

- [x] 3.1 `openspec validate mcp-expose-tracker-tasks --strict` passes; no source files modified by this change.
  - 2.1 build change seeded + implemented directly this session (user GO) — see the mcp-tracker-tools build.
