# linear-agent-integration

## Why

`linear-mirror` (change #1) brings Linear issues into Nergal as data you can read. But Nergal's value is the agent↔human loop — an issue that only sits in a panel is a mirror, not an integration. This change makes an issue something the agent acts on: send it as a prompt, spawn a worktree session to work it, or attach it as persistent context. And it gives a session memory of **which issue it is working on** (the binding) so write-back (change #3) is scoped and the loop can close.

The crucial insight is the same one `clickup-task-integration` already proved: Nergal solved "inject external context into an agent session, agent-agnostically" for Obsidian notes (`obsidian-context-injection`), and the ClickUp integration rode that exact machinery. This change rides it too — the `assemble_injected_context` assembler at `pty.rs:605` and the `ContextInjection` adapter contract — so Linear issue context reaches Claude / Codex / OpenCode through their best native channel with no new injection plumbing. The difference from ClickUp is only the source: issue markdown composed from the Linear SQLite mirror instead of the ClickUp mirror, and Linear's fixed schema (no checklists, no custom fields) instead of ClickUp's.

Delivers value alone: select an issue → the agent works on it (one-shot prompt, or a fresh worktree session, or as standing context), and the session remembers its issue.

## What Changes

- **Three issue→agent verbs** (identical contract to ClickUp's three verbs):
  1. **Send as prompt (one-shot)** — compose the issue's full markdown and, after an explicit confirm, submit it as a turn to the active session via a bracketed-paste write (`paste_to_session`, so the multi-line body is one paste) + submit. No persistent binding.
  2. **Spawn worktree with issue** — create a new worktree session (the existing user-initiated `create_worktree` + `pending_prompts` machinery) with the issue markdown as the initial prompt, binding the new session to the issue.
  3. **Attach as context (pin)** — fold the issue markdown into the session's `injected_context` (rides spawn + resume; explicit re-inject on update), exactly like a pinned vault note or ClickUp task.
- **1:1 session↔issue binding** — a session has at most one **active** Linear issue (the write-back target, surfaced in the session UI) plus zero or more **pinned** issues (context-only). The active issue is always injected; pinned issues are injected too but carry no write-back role.
- **Context composition** — assemble an issue into a labeled markdown block read from the mirror: identifier + title + state + url heading, markdown description, priority, estimate, assignee, labels, sub-issues (identifier + title + state), comments (when the mirror has them).
- **Panel actions** — the read-only panel (and floating detail) gain the three verbs as keyboard-first actions on a selected issue, reusing the existing `data-nav-key` cursor.

## Impact

- **Affected capabilities**: `linear-agent-integration` (ADDED). Builds on `linear-mirror` (the issue source) and `linear-task-panel` (the action surface).
- **Affected code**:
  - Rust: migration `024_linear_session_binding.sql` adding `active_linear_issue_id` + `pinned_linear_issue_ids` to `sessions` (exact mirror of `018_clickup_session_binding.sql`); `Session` model + `db.rs` mapping (find/create + binding helpers); new `linear/integration.rs` (issue→markdown composer + budget attrition); extend the `assemble_injected_context` assembler at `pty.rs:605` to concatenate a third source; Tauri commands for the three verbs + bind/unbind/pin/unpin/reinject in `linear/mod.rs`; reuse `create_worktree` (`worktree.rs`), `queue_session_prompt`/`pending_prompts` (`pty.rs`), `paste_to_session` + `sanitize_for_pty` (`pty.rs`) — all already extracted by the ClickUp change.
  - React: action atoms in `src/stores/linear.ts` (mirror `src/stores/clickup.ts`); verb affordances + send-confirm dialog in `src/components/linear/`; session-tab indicator of the bound issue.
- **Depends on**: `linear-mirror` (the mirror + panel). Independent of the context-bridge MCP changes.

## Build contract

### Qué construyo

1. Migration `024_linear_session_binding.sql`: `ALTER TABLE sessions ADD COLUMN active_linear_issue_id TEXT;` + `ADD COLUMN pinned_linear_issue_ids TEXT;` (nullable JSON array, same pattern as `pinned_clickup_task_ids`). Register in `db.rs` migration list in order.
2. `linear/integration.rs`: `compose_issue_markdown(conn, issue_id) -> Option<String>` (from the mirror), `assemble_linear_context(conn, session) -> Option<String>` (active ∪ pinned, deduped by id), byte-budget attrition (comments → sub-issues → description; heading never dropped), fence framing + sentinel neutralization (mirror `clickup/integration.rs`).
3. Extend `assemble_injected_context` (`pty.rs:605`) + `concat_context_blocks` (`pty.rs:628`): concatenate the Linear block as a third labeled source. Preserve the `None`-when-empty behavior so spawns with neither notes, tasks, nor issues stay byte-identical.
4. Tauri commands in `linear/mod.rs`: `linear_bind_issue` / `linear_unbind_issue`, `linear_pin_issue` / `linear_unpin_issue`, `linear_compose_issue_prompt` (compose-for-confirm), `linear_send_issue_as_prompt` (live-PTY bracketed paste + submit), `linear_reinject_issue` (live update, `submit` flag), `linear_spawn_worktree_with_issue` (create_worktree + queue_session_prompt + bind). Register all in `lib.rs`.
5. Frontend: the three verbs as actions on a selected issue (panel rows + floating detail), keyboard-bound (verify `src/stores/shortcuts.ts` for collisions; `event.code`); send-as-prompt shows a confirm dialog with the composed block before submitting; session-tab indicator of the active issue. Mirror `src/stores/clickup.ts` atoms.

### Cómo verifico

- `cd src-tauri && cargo clippy -- -D warnings && cargo test && cargo fmt --check`
- `npx tsc --noEmit`
- Tests: `compose_issue_markdown` over a fixture issue with sub-issues/labels/comments; assembler concatenates vault + ClickUp + Linear blocks; binding is 1:1 (rebind replaces per design); pin set is ordered + idempotent (mirror `pinned_clickup_task_ids` semantics); fence-sentinel in a comment cannot close the fence early; attrition order on oversize.
- Walk (dev, `pnpm tauri dev`): select an issue → send as prompt to active session (agent receives it) → spawn worktree with another issue (new session starts on it, bound) → attach an issue as context, restart, confirm re-injection → bind an issue and see the session-tab indicator.

### Criterio de done

- All three verbs work against a real mirrored issue; the agent receives composed issue context through its adapter's injection tier.
- A session shows its single active issue; rebinding follows the design rule; pinned issues are context-only.
- Resume re-injects the current active + pinned issue content (edits since last spawn reflected), matching the Obsidian + ClickUp pin contract.
- No write to Linear occurs in this change (still read-only outward); writes are `linear-writeback` (change #3).
- No `unwrap()`/`expect()` outside tests; no TODO/FIXME; comments WHY-only.

### Estimated scope

- files_estimate: 12
- risk_tier: critical
- tags: [migration, security, feature]
- visibility: private
- spec_target: linear-agent-integration
