## 1. Trait extension and types

- [ ] 1.1 Define `PlanCapability` enum in `src-tauri/src/agents/mod.rs` with variants `FileBased { dir: PathBuf, label: String }` and `NotApplicable`. Derive `Debug`, `Clone`, `Serialize`.
- [ ] 1.2 Add method to the `AgentAdapter` trait: `fn plan_capability(&self, session: &Session, cwd: &Path) -> PlanCapability`. Default impl returns `NotApplicable`.
- [ ] 1.3 Add a serializable wire type `PlanCapabilityWire` so the frontend gets `{ kind: "FileBased", dir: string, label: string }` or `{ kind: "NotApplicable" }`. Convert from `PlanCapability` at the command boundary.
- [ ] 1.4 Add Tauri command `get_session_plan_capability(session_id: String) -> Result<PlanCapabilityWire, String>` that resolves the session's adapter via `AgentRegistry` and calls `plan_capability(session, cwd)`. Register it in `lib.rs::invoke_handler`.

## 2. CC adapter — resolver and impl

- [ ] 2.1 Implement helper `resolve_cc_plans_directory(cwd: &Path) -> PathBuf` in `agents/claude_code/adapter.rs` (or a new sibling `agents/claude_code/plans_path.rs` if it grows). Resolution rules: read `~/.claude/settings.json`, `<cwd>/.claude/settings.json`, `<cwd>/.claude/settings.local.json` and merge `plansDirectory` with project-local taking precedence. Expand `~`. If absolute → use as-is. If relative → join against `cwd`. If absent → fallback to `<cwd>/.claude/plans`.
- [ ] 2.2 Implement `plan_capability` on the CC adapter, returning `FileBased { dir: resolve_cc_plans_directory(cwd), label: "Claude Code".to_string() }`.
- [ ] 2.3 Refactor `agents/claude_code/plan.rs::PlanWatcher` setup in `lib.rs` to call `resolve_cc_plans_directory()` instead of using `config.plans_directory`. Watcher behavior (file events, debouncing) unchanged.
- [ ] 2.4 Unit tests for `resolve_cc_plans_directory`:
  - Relative path resolves against cwd
  - Absolute path passes through
  - `~/...` expansion works
  - Missing settings.json → fallback to `cwd/.claude/plans`
  - Project-local settings override home settings
  - `settings.local.json` overrides `settings.json` in cwd

## 3. OpenCode adapter

- [ ] 3.1 Implement `plan_capability` in `agents/opencode/adapter.rs` returning `NotApplicable`. OpenCode plan mode is read-only by design — verified against `anomalyco/opencode` `packages/opencode/src/agent/agent.ts` `dev` branch: the only `edit` permission exceptions are `.opencode/plans/*.md` and `Global.Path.data/plans/*.md`, but neither is auto-written; cross-referenced with upstream issue #11078 where the model refuses to write even with permission. Add a source comment pointing readers at that finding and at `Per-agent feature limitations.md`.

## 4. Codex and Pi adapters

- [ ] 4.1 Implement `plan_capability` in `agents/codex/adapter.rs` returning `NotApplicable`. Add a `// SAFETY/SCOPE:` source comment pointing to `Per-agent feature limitations.md` so future readers know this is a deliberate decision.
- [ ] 4.2 Same for `agents/pi/adapter.rs`.

## 5. Command refactor — list_session_plans

- [ ] 5.1 Rewrite `commands::list_session_plans` (`src-tauri/src/commands.rs`):
  - Load session via `db.find_session(&session_id)`.
  - Compute `cwd` from `session.worktree_path` or `repo_path` (same logic as today).
  - Resolve adapter via `agent_state.registry.get(&session.agent_id)`.
  - Call `adapter.plan_capability(&session, &cwd)`.
  - If `FileBased { dir }` → scan dir and return `{ capability: "FileBased", dir, plans }`.
  - If `NotApplicable` → return `{ capability: "NotApplicable", plans: [] }`.
- [ ] 5.2 Update the wire type of `list_session_plans` to the new shape. Update `PlanListView` consumption accordingly.
- [ ] 5.3 Grep for callers of the global `list_plans` command. If none remain in the frontend, remove the command. If callers exist, leave it as-is for now.
- [ ] 5.4 Remove `Config::plans_directory` field from `src-tauri/src/config.rs`. Use `#[serde(default)]` + `#[serde(skip)]` strategy if needed to tolerate legacy config files containing the field; document the back-compat approach in the impl.
- [ ] 5.5 Update `src-tauri/src/lib.rs` `setup` block to compute the watcher path via `resolve_cc_plans_directory(home_cwd_or_first_workspace)` instead of `config.plans_directory.clone()`. If no workspace is open at boot, the watcher startup SHALL log-and-skip rather than crash.

## 6. Frontend — capability awareness

- [ ] 6.1 Add to `src/stores/plan.ts`: an atom `activePlanCapabilityAtom` (derived from `activeSessionIdAtom`) that calls `get_session_plan_capability` and caches per session. Invalidate on session switch.
- [ ] 6.2 Update `src/components/panel/PlanListView.tsx`:
  - Read `activePlanCapabilityAtom`.
  - If `NotApplicable` → render nothing or a minimal placeholder (panel should not be reachable in this state, but defensive).
  - If `FileBased { dir }` and `plans.length === 0` → empty state shows user-friendly copy (no internal-jargon like `plansDirectory`) plus a muted-mono path so the user can verify where the panel is reading from.
  - Otherwise → existing list behavior.
- [ ] 6.3 Update the right-panel chrome (locate the panel switcher component — likely `src/components/panel/RightPanel.tsx` or `PanelChrome.tsx`):
  - Subscribe to `activePlanCapabilityAtom`.
  - When `NotApplicable`, omit the Plans entry from the rendered switcher.
  - If the currently-visible panel was Plans and the active session switches to a `NotApplicable` agent, transition to a fallback panel (Activities, configurable via existing default-panel logic).

## 7. Settings UI cleanup

- [ ] 7.1 Remove the `plans_directory` field from `src/components/settings/SettingsPanel.tsx` (around line 1754). Also remove the corresponding form binding and any state in `src/stores/config.ts`.
- [ ] 7.2 Update `src/lib/types.ts` to drop `plans_directory` from the `Config` type.

## 8. Tests and verification

- [ ] 8.1 Rust unit tests for `resolve_cc_plans_directory` covering all resolution paths (5+ scenarios per 2.4).
- [ ] 8.2 Rust integration test: register stub adapters with each `plan_capability` variant, simulate `list_session_plans` for each, assert the response shape.
- [ ] 8.3 Manual UX walks:
  - CC session with `plansDirectory = ".claude/plans"` (current dev setup) → panel works, no regression.
  - CC session with `plansDirectory = "/tmp/custom-plans"` → panel reads from `/tmp/custom-plans`.
  - CC session with no `plansDirectory` set → fallback to `<cwd>/.claude/plans`.
  - OpenCode session → Plans entry hidden from right-panel switcher (opencode plan mode does not auto-persist plans; see anomalyco/opencode#11078).
  - Codex session → Plans entry hidden from right-panel switcher.
  - Pi session → Plans entry hidden from right-panel switcher.
  - Switching between CC and Codex sessions → Plans entry appears/disappears correctly.
- [ ] 8.4 Full verification: `cd src-tauri && cargo clippy -- -D warnings && cargo test && cargo fmt --check && cd .. && npx tsc --noEmit`.
- [ ] 8.5 Bundled install verification: `pnpm tauri build && sudo dpkg -i src-tauri/target/release/bundle/deb/Nergal_*.deb`. Open the installed app and run the manual UX walk against the real binary (the dev environment can differ from the bundled hook CLI).
