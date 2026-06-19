# Tasks — linear-agent-integration

> Depends on `linear-mirror` (mirror + panel). Migration number = `024` (next
> free after `023_linear_mirror.sql`), registered in `db.rs`'s migration list in
> order. Read `implementation.md` first — it anchors every symbol to a line ref.

## 1. Schema — session binding

- [x] 1.1 New migration `024_linear_session_binding.sql`: `ALTER TABLE sessions ADD COLUMN active_linear_issue_id TEXT;` and `ALTER TABLE sessions ADD COLUMN pinned_linear_issue_ids TEXT;` (nullable JSON array, same pattern as `pinned_clickup_task_ids`). Register in `db.rs`'s migration list after `023`.
- [x] 1.2 Extend the `Session` model (`models.rs:44`) with `active_linear_issue_id: Option<String>` + `pinned_linear_issue_ids: Vec<String>` (both `#[serde(default)]`, after the clickup fields). **Update ALL `Session {…}` struct literals** (non-`Default` fields → compile error otherwise): `commands.rs:914`, `mcp/directory.rs:294`, `clickup/mod.rs:864`, the new linear-spawn literal (task 4.6), and the test literals `db.rs:1305/1501/1584/1712`, `pty.rs:1521`, `clickup/integration.rs:464` — seed `None` / `Vec::new()` except where a test exercises a linear binding (Delta 7).
- [x] 1.3 `db.rs`: add `parse_pinned_linear_issue_ids` (mirror `parse_pinned_clickup_task_ids` at `:119`); extend `find_session` + `find_sessions` SELECTs and row maps with the two new columns (indices 16, 17 — appended after the clickup pinned column at 15); extend `create_session` INSERT + binds.
- [x] 1.4 `db.rs`: add `set_active_linear_issue`, `get_pinned_linear_issues`, `add_pinned_linear_issue`, `remove_pinned_linear_issue` (mirror `set_active_clickup_task`/`get_pinned_clickup_tasks`/`add_pinned_clickup_task`/`remove_pinned_clickup_task` at `:773`–`:815`).
- [x] 1.5 Tests: `parse_pinned_linear_issue_ids` handles null/empty/garbage/valid; a session round-trip persists + loads both new fields (mirror the clickup test at `db.rs:1663+`).

## 2. Context composition

- [x] 2.1 `linear/integration.rs`: `compose_issue_markdown(conn, issue_id) -> Result<Option<String>>` and `assemble_linear_context(conn, &Session) -> Option<String>` (active ∪ pinned, deduped by id, active first; `None` when no bindings or all dangle, errors degrade to `None` + log). Fence labeled "# Linear issue brief" (team-authored, per Decision 6); `neutralize_fence_sentinels` mangles both sentinels. `CONTEXT_BUDGET_BYTES = 32*1024`, `MAX_COMMENTS = 20`.
- [x] 2.2 Section composition via direct SQL (Delta 4): heading `## {identifier} {title}` + `State: {state} · Priority: {label} · {url}`; description; priority label using the panel's exact words (0=No priority,1=urgent,2=high,3=normal,4=low — matches `LinearPanel.tsx:52` `linearPriorityStr`, NOT "Medium"); estimate + assignee display name when present; labels (`linear_issue_labels` join → comma list); sub-issues (`WHERE parent_id=?1 AND stale=0` → `- {identifier} {title} — {state}`); comments (`linear_comments` → `- **{author}**: {body}`, author parsed from the `user_json` JSON blob: `display_name` ?? `name`, fallback "Unknown" on null/unparseable). No checklists / custom fields / attachments / relations (Deltas 1, 2).
- [x] 2.3 Byte-budget attrition (`fit_to_budget`): drop oldest comments first → collapse sub-issue list to a count → head/tail-truncate description (char-boundary-safe `head_tail_truncate`). Each step a visible marker; heading never dropped. Log the attrition summary.
- [x] 2.4 Unit tests (port the ClickUp set, drop checklist/custom-field cases): all-sections compose over a fixture issue (seed `linear_issues` + labels + sub-issue + a directly-inserted comment carrying a realistic `user_json` so the author-parse + attrition paths run); missing-issue → `None`; dedupe active∈pinned (one fence, one issue); fence-sentinel **in both a comment AND the description** cannot close the fence early; comment author with null/garbage `user_json` falls back to "Unknown"; attrition stages isolated by budget (comments → sub-issues → description, heading kept). A small fixtures module seeding the minimal `023` hierarchy (team → state → issue) like `clickup/integration.rs`'s `seeded_conn`.

## 3. Assembler integration

- [x] 3.1 Extend `concat_context_blocks` (`pty.rs:628`) to take a third `Option<String>` (linear) and merge all present sources with `\n` separators, preserving `None`-when-all-empty.
- [x] 3.2 In `assemble_injected_context` (`pty.rs:605`): after the clickup block, call `linear::integration::assemble_linear_context(g.conn(), &session)` and pass it to the 3-arg `concat_context_blocks`. The existing empty-case tests (`pty.rs:1568+`) must still pass (a session with no notes/tasks/issues → `None`, byte-identical spawn).
- [x] 3.3 Test: a session with a bound linear issue (no notes, no clickup task) assembles a block containing the issue heading; resume path re-reads (assembler runs on fresh + resume — covered by the single function).

## 4. Verbs + binding commands

- [x] 4.1 `linear_bind_issue(session_id, issue_id)` — set `active_linear_issue_id` (replace if one exists; UI confirms upstream). `linear_unbind_issue(session_id)` — clear it. Future spawns/resumes only (Decision 7).
- [x] 4.2 `linear_pin_issue` / `linear_unpin_issue` — ordered, idempotent JSON-array edits; return the updated pin list (mirror `clickup_pin_task`/`clickup_unpin_task`).
- [x] 4.3 `compose_for_delivery(db, issue_id)` private helper: `compose_issue_markdown` → `sanitize_for_pty`; error "issue not found in the local mirror" on dangling. `linear_compose_issue_prompt(issue_id)` returns it for the confirm step.
- [x] 4.4 `linear_send_issue_as_prompt(session_id, issue_id)` — recompose, deliver via `paste_to_session(.., submit=true)`. One-shot, no binding. Mid-turn relies on the agent's native queueing (no gate).
- [x] 4.5 `linear_reinject_issue(session_id, issue_id, submit: Option<bool>)` — recompose + `paste_to_session(.., submit.unwrap_or(false))`. Pin/refresh paste WITHOUT submit; bind delivers WITH submit (called from the frontend bind action).
- [x] 4.6 `linear_spawn_worktree_with_issue(workspace_id, issue_id, slug?)` — resolve repo + `is_git_repo` guard; `compose_issue_markdown` → `sanitize_for_pty`; read the issue title from `linear_issues`; derive slug from title (`derive_worktree_slug`); slug-collision check; `create_worktree`; build the `Session` (resolve agent like `clickup_spawn_worktree_with_task`, `active_linear_issue_id = Some(issue_id)`); `create_session` + `register_session` + `extend_plan_watcher_for_session`; `queue_session_prompt`. Return the `Session`.
- [x] 4.7 Register all 8 commands in the `lib.rs` invoke handler.

## 5. Frontend

- [x] 5.1 `src/stores/linear.ts`: binding/pins atoms (`linearBindingMapAtom`, `linearPinsMapAtom`, `linearSendConfirmAtom`), resolvers (`resolveActiveLinearIssue`, `activeSessionLinearIssueAtom`, `activeSessionLinearPinsAtom`), `FUTURE_SPAWNS_HINT`, action atoms (`requestSendIssueAction`, `togglePinIssueAction`, `performBindIssueAction`, `unbindIssueAction`, `requestBindIssueAction` with swal rebind confirm + `escapeHtml`, `spawnWorktreeWithIssueAction`, `reinjectIssueAction`). Mirror `stores/clickup.ts:285-600`.
- [x] 5.2 Issue actions in `src/components/linear/`: `ToolbarAction` buttons in the detail toolbar (mirror `ClickUpTaskView`'s toolbar) + a local `VERB_KEYS` keydown handler in `LinearTaskDetail.tsx` (mirror `ClickUpTaskDetail.tsx:79-99`: S send, W spawn, P pin, B bind, R reinject — `event.code`, gated by the `data-focus-zone='linear'` zone). Collision check is the Linear detail zone's own keys (its existing `copyid`/`open` nav keys), NOT global `shortcuts.ts` (Delta 6). "Open as tab" is out of scope. **id-resolution precision** (do NOT copy ClickUp's verbatim): the panel row exposes `data-issue-id` (`LinearPanel.tsx:1102`), NOT `data-task-id`; and Linear uses a single `data-focus-zone='linear'` for both the detail (`LinearTaskDetail.tsx:102`) and the panel (`LinearPanel.tsx:476`) — so the ClickUp two-zone branch (`clickup` detail vs `panel` rows) collapses to one zone. Resolve the target id as: open detail issue (`linearDetailIssueIdAtom`) wins, else the `[data-nav-selected='true'][data-issue-id]` row. Send-as-prompt shows a confirm dialog with the composed (team-authored) block before submitting. Unbind/unpin UI states it affects future spawns.
- [x] 5.3 Session-tab indicator of the active issue (a chip naming the bound issue; click focuses it in the panel) — sibling to the ClickUp tab chip.
- [x] 5.4 Rebind confirmation UI when binding over an existing active issue (swal, matching the ClickUp `requestBindTaskAction`).

## 6. Verification

- [x] 6.1 `cd src-tauri && cargo clippy -- -D warnings && cargo test && cargo fmt --check`
- [x] 6.2 `npx tsc --noEmit`
- [x] 6.3 Manual walk per proposal § Cómo verifico (dev, `pnpm tauri dev` — no release/build/install this cycle). Walked OK 2026-06-18.
