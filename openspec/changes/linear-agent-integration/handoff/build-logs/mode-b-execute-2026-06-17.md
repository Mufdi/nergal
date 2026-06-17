## Session mode-b-execute · 2026-06-17

Autonomous `/work` build (single builder = main session, per the user's `/loop` request). iprev APPROVED (2 rounds) before build.

### File changes
- `migrations/024_linear_session_binding.sql`: +6 (new) — two nullable session columns.
- `src/db.rs`: +~95 — `parse_pinned_linear_issue_ids`, 2 SELECT/map sites, INSERT, 4 binding helpers, 2 new tests + extended roundtrip.
- `src/models.rs`: +10 — two `Session` fields.
- `src/linear/integration.rs`: +~560 (new) — composer (direct SQL), priority/user_json parse, fence + sentinel neutralization, 3-stage attrition, 13 unit tests.
- `src/linear/mod.rs`: +~210 — 8 Tauri commands + `pub mod integration`.
- `src/pty.rs`: +~10 — `concat_context_blocks` → 3 sources, linear block in assembler, updated concat tests.
- `src/lib.rs`: +9 — register 8 commands.
- `src/{clickup/mod.rs, commands.rs, mcp/directory.rs, clickup/integration.rs}`: +2 each — Session-literal field additions (Delta 7).
- `src/stores/linear.ts`: +~250 — binding/pins atoms, resolvers, 7 verb action atoms, send-confirm atom, labels.
- `src/stores/workspace.ts`: +6 — two `Session` TS fields.
- `src/components/linear/LinearTaskView.tsx`: +~95 — `ToolbarAction` + `LinearVerbToolbar` + imports.
- `src/components/linear/LinearTaskDetail.tsx`: +~60 — VERB_KEYS handler (S/W/P/B/R) + toolbar wiring.
- `src/components/linear/LinearConfirmDialogs.tsx`: +~110 (new) — send-confirm dialog.
- `src/components/layout/{Workspace.tsx, TopBar.tsx}`: mount the dialog + active-issue tab chip.

### Decisiones + tradeoffs
- **Composer reads via direct SQL** (Delta 4) — no new `mirror.rs` view added for a single consumer.
- **Comments read from `linear_comments`** even though the #1 poller never populates it (Delta 3) — forward-compatible; exercised by a seeded test fixture.
- **Attachments/relations excluded** from compose (Delta 2) — not mirrored; mirror-only contract forbids a live call.
- **Single `data-focus-zone='linear'`** for both panel + detail (Delta 6) — VERB_KEYS resolves the open detail issue first, else the `data-nav-selected` row's `data-issue-id`.
- **Toolbar drops C (closeout=writeback) + T (tab) + O (open=body nav key)** vs ClickUp — read-only-outward scope.

### Divergencias vs proposal
- None material. The proposal's "panel rows + floating detail" verbs are delivered via the detail toolbar (buttons) + the zone-wide VERB_KEYS keyboard handler (which covers the panel's selected row). Per-row verb buttons in `LinearPanel.tsx` were not added — keyboard-first covers the panel surface, matching the spec's "keyboard-first actions".

### Gates
- `cargo clippy -- -D warnings` ✅ · `cargo test` ✅ 580 passed · `cargo fmt --check` ✅ · `npx tsc --noEmit` ✅ · `vite build` ✅
- Manual dev walk (6.3): PENDING — needs the user's Linear key + GUI (same blocker as #1).
