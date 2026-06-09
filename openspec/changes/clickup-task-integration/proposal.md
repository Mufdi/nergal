# clickup-task-integration

## Why

`clickup-sync` brings ClickUp tasks into Nergal as data you can read. But Nergal's value is the agent↔human loop — a task that only sits in a panel is a mirror, not an integration. This change makes a task something the agent acts on: send it as a prompt, spawn a worktree session to work it, or attach it as persistent context. And it gives a session memory of **which task it is working on** (the binding) so write-back (next change) is scoped and the loop can close.

The crucial insight: Nergal already solved "inject external context into an agent session, agent-agnostically" for Obsidian notes (`obsidian-context-injection`). This change rides that exact machinery — the `injected_context` assembler at `pty.rs:396` and the `ContextInjection` adapter contract — so ClickUp task context reaches Claude / Codex / OpenCode through their best native channel with no new injection plumbing. The difference is only the source: task markdown composed from the SQLite mirror instead of a vault file.

Delivers value alone: select a task → the agent works on it (one-shot prompt, or a fresh worktree session, or as standing context), and the session remembers its task.

## What Changes

- **Three task→agent verbs**:
  1. **Send as prompt (one-shot)** — compose the task's full markdown and, after an explicit confirm, submit it as a turn to the active session via a bracketed-paste write (`terminal_paste`, so the multi-line body is one paste) + submit, or to a new worktree session.
  2. **Spawn worktree with task** — create a new worktree session (the existing user-initiated `create_worktree` + `LaunchOptions` + `pending_prompts` machinery) with the task markdown as the initial prompt.
  3. **Attach as context (pin)** — fold the task markdown into the session's `injected_context` (rides spawn + resume; explicit re-inject on update), exactly like a pinned vault note.
- **1:1 session↔task binding** — a session has at most one **active** ClickUp task (the write-back target, surfaced in the session UI) plus zero or more **pinned** tasks (context-only). The active task is always injected; pinned tasks are injected too but carry no write-back role.
- **Context composition** — assemble a task into a labeled markdown block: name, description, status, subtasks, checklists, custom fields, comments, attachment links — read from the mirror.
- **Panel actions** — the read-only panel (and floating detail) gain the three verbs as keyboard-first actions on a selected task.

## Impact

- **Affected capabilities**: `clickup-agent-integration` (ADDED). Builds on `clickup-mirror` (the task source) and `clickup-task-panel` (the action surface).
- **Affected code**:
  - Rust: migration adding `active_clickup_task_id` + `pinned_clickup_task_ids` to `sessions`; extend the `injected_context` assembler at `pty.rs:396`; new `clickup/integration.rs` (task→markdown composer + binding commands); Tauri commands for the three verbs + bind/unbind/pin/unpin; reuse `create_worktree` (`worktree.rs:261`), `pending_prompts` (`pty.rs:388`), the bracketed-paste write (`terminal_paste`, `pty.rs:994`) for send-as-prompt.
  - React: action affordances in `src/components/clickup/`, session-tab indicator of the bound task, atoms for binding state.
- **Depends on**: `clickup-sync` (the mirror + panel). Independent of the context-bridge changes.

## Build contract

### Qué construyo

1. Migration (next free number after `clickup-sync`'s): `ALTER TABLE sessions ADD COLUMN active_clickup_task_id TEXT` + `ADD COLUMN pinned_clickup_task_ids TEXT` (JSON array, like `pinned_note_paths`).
2. `clickup/integration.rs`: `compose_task_markdown(task_id) -> String` (from the mirror), `assemble_clickup_context(session) -> Option<String>` (active + pinned).
3. Extend `pty.rs:396` assembler: concatenate the ClickUp context block into `injected_context` alongside the vault-note block (single labeled block per source).
4. Tauri commands: `clickup_send_task_as_prompt(session_id, task_id)` (live-PTY write), `clickup_spawn_worktree_with_task(workspace_id, task_id, slug?)` (create_worktree + pending_prompts), `clickup_bind_task(session_id, task_id)` / `clickup_unbind_task(session_id)`, `clickup_pin_task(session_id, task_id)` / `clickup_unpin_task(session_id, task_id)`, `clickup_reinject_task(session_id, task_id)` (live update).
5. Frontend: the three verbs as actions on a selected task (panel + floating detail), keyboard-bound (verify `shortcuts.ts`); session-tab indicator of the active task.

### Cómo verifico

- `cd src-tauri && cargo clippy -- -D warnings && cargo test && cargo fmt --check`
- `npx tsc --noEmit`
- Tests: `compose_task_markdown` over a fixture task with subtasks/checklists/comments; assembler concatenates vault + ClickUp blocks; binding is 1:1 (binding a second task replaces the active one or is rejected per design); pin set is ordered + idempotent (mirror `pinned_note_paths` semantics); resume re-injects the current active+pinned task content.
- Walk: select a task → send as prompt to active session (agent receives it) → spawn worktree with another task (new session starts on it) → attach a task as context, restart, confirm re-injection → bind a task and see the session-tab indicator.

### Criterio de done

- All three verbs work against a real task; the agent receives composed task context through its adapter's injection tier.
- A session shows its single active task; rebinding follows the design rule; pinned tasks are context-only.
- Resume re-injects the current active + pinned task content (edits since last spawn reflected), matching the Obsidian pin contract.
- No write to ClickUp occurs in this change (still read-only outward); writes are `clickup-writeback`.
- No `unwrap()`/`expect()` outside tests; no TODO/FIXME; comments WHY-only.

### Estimated scope

- files_estimate: 12
- risk_tier: critical
- tags: [migration, feature]
- visibility: private
- spec_target: clickup-agent-integration
