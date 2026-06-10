# BUILD-LOG — clickup-sync · Mode B execute · 2026-06-09

## Session summary

Full implementation of groups 1-5 + verification (6.1/6.2). Manual walk (6.3) pending — needs a real token + live workspace.

### Tool activity
- Agent spawns: 3 canary isolation checks (haiku) + 3 builders (inherit main model) + 2 mid-build reviewers (security, deps). Final 4-parallel reviewer pass skipped by user decision; replaced with orchestrator inline spot-check.
- Verify runs: cargo clippy/test/fmt + tsc after each builder, independently re-run by the orchestrator.

### File changes (working tree vs HEAD)
- NEW `src-tauri/src/clickup/` — mod.rs, auth.rs (~290), model.rs (~570), client.rs (~410), mirror.rs (~810), poller.rs (~1770 incl. 19 tests), fixtures/*.json (7)
- NEW `src-tauri/migrations/015_clickup_mirror.sql` (12 tables), `016_clickup_stale_since.sql`
- NEW `src/stores/clickup.ts` (+193), `src/components/clickup/ClickUpPanel.tsx` (+459), `ClickUpTaskDetail.tsx` (~400)
- Modified: Cargo.toml (+keyring 3), Cargo.lock, db.rs (conn accessor + migrations + full-chain test), lib.rs (9 commands + poller start + managed state), config.rs (poll interval), rightPanel.ts, shortcuts.ts (Ctrl+Shift+U), TopBar.tsx, RightPanel.tsx, TabBar.tsx, App.tsx, SettingsPanel.tsx (+200)
- Totals: tracked diff 702+/75−, plus ~4600 lines in new untracked files. ~28 files vs estimate 22 (ratio 1.27).

### Decisiones + tradeoffs
- **Canary isolation**: 3/3 runs, no leak of the conversation canary (system-context identifiers visible by design) → gating with subagents enabled. Cache key: cc-2.x-fable5-test-v1, 2026-06-09.
- **Builder model**: inherited main-loop model instead of the skill's static `claude-sonnet-4-6` table — `.work-modules.json` left it "TBD at build"; harness-entropy principle favors the resolved session model.
- **Migration numbers**: 015 (mirror) + 016 (`stale_since` for GC retention — Builder B addition; `stale` alone can't drive a retention window).
- **keyring backend**: `async-secret-service` + `crypto-rust` (pure-Rust, no libdbus). Costs a duplicate zbus 4.x stack next to the tree's zbus 5 (keyring 3 pins secret-service ^4); dedupe path = keyring v4. Documented in Cargo.toml.
- **Security fixes applied mid-build** (from PASS-with-findings review): redacted manual `Debug` impls for `StoredToken`/`FallbackFile`; FK added to `clickup_task_custom_values.field_id` (pre-ship migration edit, tasks.md DDL synced).
- **Orchestrator spot-check fixes**: `openExternalUrl()` http/https guard for `openShell` on untrusted ClickUp URLs (unscoped `shell:allow-open`); thumbnail `<img>` additionally scheme-gated.
- **show-closed semantics**: `clickup_fetch_closed_tasks` returns EPHEMERAL TaskViews (never written to the mirror — writing would un-tombstone and fight the next reconcile); panel merges client-side, open mirror rows win.
- **Changed-detection**: per-team fetch fingerprint (upsert row-counts can't distinguish idle cycles under ON CONFLICT DO UPDATE).
- **GC guards**: only childless stale rows deleted (CASCADE would take live children); chains drain across cycles.
- **Team switch**: tombstones the whole mirror; old team's rows decay via GC.
- **Detail refresh never un-tombstones** — only the authoritative poll fetch resurrects.
- **UI prefs**: module-level Jotai atoms (codebase's actual panel-pref tier); detail geometry via the generic `scratchpad_*_geometry` row keyed `clickup-task-detail` (no migration).

### Divergencias vs proposal
- implementation.md said "all tables new, no ALTER" — held for group 3; group 4 added migration 016 (ALTER for `stale_since`), justified above.
- tasks.md 3.1 DDL gained the `field_id` FK (review finding; synced into tasks.md).
- Handoff note drift found by Builder B: the existing `send_notification` command is a notify-send shell-out; poller uses `tauri_plugin_notification::NotificationExt` directly (plugin was initialized but unused) — matches implementation.md's "reuse tauri-plugin-notification".
- `token_on_disk` disclosure persists for the app run only (no query command; out of prescribed scope) — candidate follow-up for clickup-writeback or a later tweak.

### Verification (final)
- `cargo clippy --all-targets -- -D warnings` ✅ · `cargo test` 371 passed ✅ · `cargo fmt --check` ✅ · `npx tsc --noEmit` ✅
- openspec validate clickup-sync --strict ✅ (pre-existing unrelated fail: spec/annotation-persistence)
- Pending: tasks.md 6.3 manual walk.
