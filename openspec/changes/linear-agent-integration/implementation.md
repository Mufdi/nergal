# Implementation — linear-agent-integration

> Read before implementing tasks. This is the plan mapped to the real codebase.
> The whole change mirrors the archived `clickup-task-integration`; every claim
> below is anchored against files on disk (verified 2026-06-17).

## Verified codebase facts (do not re-assume)

### Assembler + delivery primitives (already extracted by the ClickUp change)
- `pty.rs:605` `pub(crate) fn assemble_injected_context(g: &Database, session_id) -> Option<String>` — builds `vault_block` (`pty.rs:610`) then `clickup_block` (`pty.rs:622` `clickup::integration::assemble_clickup_context(g.conn(), &session)`), merges via `concat_context_blocks` (`pty.rs:628`).
- `pty.rs:628` `fn concat_context_blocks(vault: Option<String>, clickup: Option<String>) -> Option<String>` — 4-arm match; joins with `"{v}\n{c}"`. **Extend to a third source.**
- `pty.rs` `pub(crate) fn paste_to_session(state: &PtyManager, session_id, text, submit: bool) -> Result<...>` — bracketed paste + `\r` after the closing marker when `submit`. Used by `clickup_send_task_as_prompt` (`clickup/mod.rs:773`) and `clickup_reinject_task` (`clickup/mod.rs:792`).
- `pty.rs` `pub fn sanitize_for_pty(s: &str) -> String` — used by `compose_for_delivery` (`clickup/mod.rs:745`) + `clickup_spawn_worktree_with_task` (`clickup/mod.rs:831`).
- `pty.rs` `queue_session_prompt(pty, session_id, text)` — stashes the initial prompt consumed at PTY spawn; called at `clickup/mod.rs:897`.
- `worktree.rs` `create_worktree(repo_path, slug)` + `is_git_repo(repo_path)`; `commands.rs` `derive_worktree_slug(base, ts)`; `commands.rs` `extend_plan_watcher_for_session(...)`.

### Session model + db mapping
- `models.rs:44` `struct Session` — fields end with `active_clickup_task_id: Option<String>` (`:83`) + `pinned_clickup_task_ids: Vec<String>` (`:88`), both `#[serde(default)]`. **Add `active_linear_issue_id` + `pinned_linear_issue_ids` after them.**
- `db.rs:119` `parse_pinned_clickup_task_ids(raw: Option<String>) -> Vec<String>` (warns on malformed). **Add `parse_pinned_linear_issue_ids` mirror.**
- `db.rs:316` `find_sessions` SELECT lists columns; `db.rs:351-352` maps `active_clickup_task_id = row.get(14)`, `pinned = parse_..(row.get(15))`. `db.rs:430` `find_session` SELECT; `db.rs:448-449` same mapping. **Append the two linear columns → indices 16, 17 in both SELECTs/maps.**
- `db.rs:395` `create_session` INSERT; `db.rs:417-418` binds `active_clickup_task_id` + the pinned JSON. **Add the two linear binds.**
- `db.rs:773` `set_active_clickup_task(&self, session_id, task_id: Option<&str>) -> Result<()>` (`UPDATE ... active_clickup_task_id = ?1, updated_at = ?2`). `db.rs:781` `get_pinned_clickup_tasks`. `db.rs:797` `add_pinned_clickup_task`. `db.rs:806` `remove_pinned_clickup_task` (writes JSON at `:815`). **Add the four linear mirrors.**

### ClickUp composer (the structural template for `linear/integration.rs`)
- `clickup/integration.rs` — `CONTEXT_BUDGET_BYTES = 32*1024`, `MAX_COMMENTS = 20`, `FENCE_OPEN`/`FENCE_CLOSE`, `neutralize_fence_sentinels`, `compose_task_markdown(conn, task_id) -> Result<Option<String>>`, `assemble_clickup_context(conn, &Session) -> Option<String>` (active ∪ pinned dedup), `assemble_ids`, `compose_sections` (direct `query_row`/`query_map`), `render`, `fit_to_budget` (attrition), `head_tail_truncate` (char-boundary-safe). The `ComposedTask` struct + budget loop are reusable nearly verbatim minus checklist/custom-field fields.

### ClickUp Tauri commands (template for `linear/mod.rs`)
- `clickup/mod.rs:677` `clickup_bind_task`, `:691` `clickup_unbind_task`, `:704` `clickup_pin_task`, `:719` `clickup_unpin_task`, `:737` `compose_for_delivery` (private), `:751` `clickup_compose_task_prompt`, `:765` `clickup_send_task_as_prompt`, `:783` `clickup_reinject_task` (`submit: Option<bool>`), `:804` `clickup_spawn_worktree_with_task`. All registered in `lib.rs` invoke handler.

### Linear mirror (the data source)
- `023_linear_mirror.sql`: `linear_issues` (id, identifier, team_id, title, description, state_id, priority INTEGER, estimate REAL, assignee_id, project_id, cycle_id, parent_id, due_date, created_at, updated_at, completed_at, url, stale, stale_since); `linear_workflow_states` (id, name, type, color, position); `linear_users` (id, name, display_name, avatar_url); `linear_labels` (id, name, color); `linear_issue_labels` (issue_id, label_id); `linear_comments` (id, issue_id, user_json, body, created_at) — **table exists, poller does NOT populate it in change #1** (no `upsert_comment` in `linear/mirror.rs`).
- `linear/mirror.rs:485` `read_issues(conn, &IssueFilter) -> Vec<IssueView>` and `:582` `read_teams` are the ONLY read helpers — compose uses direct SQL (Delta 4).
- Priority int→label — reuse the panel's exact words from `linearPriorityStr` (`components/linear/LinearPanel.tsx:52`): 0=No priority, 1=urgent, 2=high, **3=normal** (NOT "Medium"), 4=low. The composer must agree with the panel.
- `linear_comments.user_json` (`023_linear_mirror.sql`) is a JSON blob of the Linear user object — parse `display_name` ?? `name`, fallback "Unknown". The poller never writes it in #1 (no `upsert_comment` in `linear/mirror.rs`; the only insert is the cascade test at `mirror.rs:660`, which omits `user_json`).

### Linear frontend (the action surface)
- `src/stores/linear.ts` (190 lines) — atoms: `linearTeamFilterAtom`, `linearGroupByAtom`, `linearAssignedToMeAtom`, `linearDetailIssueIdAtom`, `linearLabelFilterAtom`, `linearSortAtom`, `copyLinearIssueAction`. **Add binding/pins atoms + verb action atoms + send-confirm atom, mirroring `stores/clickup.ts:285-600`.**
- `src/stores/clickup.ts` integration atoms (template): `clickupBindingMapAtom` (`:286`), `clickupPinsMapAtom` (`:290`), `clickupSendConfirmAtom` (`:294`), `resolveActiveClickUpTask` (`:298`), `activeSessionClickUpTaskAtom` (`:328`), `activeSessionClickUpPinsAtom` (`:336`), `FUTURE_SPAWNS_HINT` (`:346`), `requestSendTaskAction` (`:417`), `togglePinTaskAction` (`:430`), `performBindTaskAction` (`:473`), `unbindTaskAction` (`:496`), `requestBindTaskAction` (`:512`, swalConfirm rebind), `spawnWorktreeWithTaskAction` (`:551`), `reinjectTaskAction` (`:595`), `escapeHtml` (`:542`).
- `src/components/clickup/ClickUpConfirmDialogs.tsx` — the send-as-prompt confirm dialog (template for a Linear equivalent).
- `src/components/linear/`: `LinearPanel.tsx` (rows + label filter + header nav), `LinearTaskView.tsx` (floating detail + `data-nav-key` cursor + `useLinearIssueController`), `LinearTaskDetail.tsx`.
- Session-tab indicator: locate the ClickUp tab chip (grep `resolveActiveClickUpTask` usage in the tab/sidebar component) and add a Linear sibling.
- Verb keybindings: `components/clickup/ClickUpTaskDetail.tsx:79-99` defines `VERB_KEYS = ["KeyS","KeyW","KeyP","KeyB","KeyR","KeyC","KeyO","KeyT"]` + a keydown handler (NOT in ClickUpTaskView/Panel, NOT in shortcuts.ts). Mirror S/W/P/B/R into `components/linear/LinearTaskDetail.tsx` (it already hosts `useLinearIssueController` + the `data-focus-zone='linear'` zone). `ToolbarAction` (`ClickUpTaskView.tsx:204`) is the toolbar-button component; the detail toolbar renders one per verb (`:1356-1375`).
- `Session {…}` literals needing the two new fields (Delta 7): `commands.rs:914`, `mcp/directory.rs:294`, `clickup/mod.rs:864`, `pty.rs:1521` (test), `db.rs:1305/1501/1584/1712` (tests), `clickup/integration.rs:464` (test_session) + the new linear-spawn literal.

## Execution order (per phase)

1. **Schema** (migration 024 + model + db mapping + binding helpers + db tests). Compile gate.
2. **Composer** (`linear/integration.rs` + unit tests: all-sections, missing-issue None, dedupe, fence-sentinel, attrition stages). Compile + test gate.
3. **Assembler** (extend `concat_context_blocks` to 3 sources + `assemble_injected_context` linear_block; preserve byte-identical empty case + existing assembler tests). Test gate.
4. **Commands** (`linear/mod.rs` 8 commands + register in `lib.rs`). Compile gate.
5. **Frontend** (`stores/linear.ts` atoms + verbs; panel + detail actions; send-confirm dialog; session-tab chip; shortcuts collision check). `tsc` gate.
6. **Verification** (full check + dev walk).

## Edge cases + mitigations

- **Migration index shift**: the two new columns append after `pinned_clickup_task_ids` (index 15) → linear active=16, pinned=17 in BOTH `find_session` and `find_sessions` SELECTs/maps. Miss one → wrong-column reads. Mitigation: db round-trip test asserting both new fields persist + load (mirror `db.rs:1663+` clickup test).
- **Dangling binding**: a bound issue id absent from the mirror (evicted/tombstoned) → `compose_issue_markdown` returns `None`; `assemble_linear_context` skips it with a log (mirror ClickUp `assemble_ids`). A send/reinject of a dangling id errors cleanly ("issue not found in the local mirror").
- **Comments table empty**: change-#1 poller does not populate `linear_comments`; compose reads it and renders no Comments section. Forward-compatible (Delta 3). Test seeds a comment directly to exercise the rendering + attrition path.
- **Unsupported agent**: attach records but does not inject; the tab chip reflects it (inherit the ClickUp/Obsidian handling — no PTY fallback).
- **shortcuts.ts collisions**: Linear detail already uses a `data-nav-key` cursor + contextual keys; the new S/W/P/B/R verb keys must not collide with existing Linear-zone bindings. Verify `src/stores/shortcuts.ts` and the Linear zone handlers before wiring (event.code).
- **Worktree spawn on non-git workspace**: guarded by `is_git_repo` (mirror `clickup/mod.rs:824`).

## Per-phase risk

- Schema (low): pure mirror of 018; the only trap is the SELECT index shift (covered by a round-trip test).
- Composer (medium): attrition + fence-sentinel are security-relevant; covered by unit tests ported from ClickUp.
- Assembler (medium): must keep the empty case byte-identical (existing tests at `pty.rs:1568+` assert `None` when empty — they must still pass).
- Commands (low): thin wrappers over db + the shared pty/worktree primitives.
- Frontend (medium): keyboard collisions + the rebind confirm (swal) + send-confirm dialog; `tsc` + walk.
