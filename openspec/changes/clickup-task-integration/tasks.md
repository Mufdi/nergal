# Tasks — clickup-task-integration

> Depends on `clickup-sync` (mirror + panel). Migration number = next free after
> `clickup-sync`'s, registered in `db.rs:132` in order.

## 1. Schema — session binding

- [ ] 1.1 New migration `0NN_clickup_session_binding.sql`: `ALTER TABLE sessions ADD COLUMN active_clickup_task_id TEXT;` and `ALTER TABLE sessions ADD COLUMN pinned_clickup_task_ids TEXT;` (nullable JSON array, same pattern as `pinned_note_paths`). Register in `db.rs:132`. **Number = N+1 relative to clickup-sync's migration** (reconcile at merge — not an independent "next free" grab that could collide on a fresh DB).
- [ ] 1.2 Extend the `Session` model + `find_session` mapping to carry the two new fields (active id + parsed pinned ids).

## 2. Context composition

- [ ] 2.1 `clickup/integration.rs`: `compose_task_markdown(mirror, task_id) -> String` — block framed as **UNTRUSTED external data** (a labeled fence stating the enclosed ClickUp content is data, not instructions): name+status+url heading, markdown description, subtasks (name+status), checklists (items+resolved), custom fields (by type; computed fields like `automatic_progress` read-only), recent N comments (author+text), attachments as `- title (url)` links. Reads the mirror only.
- [ ] 2.2 `assemble_clickup_context(mirror, session) -> Option<String>` — composes active ∪ pinned (dedupe by id), capped at a byte budget with a **defined attrition order across all sections** (comments oldest-first → checklists to counts → subtasks to counts → description head/tail; heading never dropped), each step marked, omissions logged. `None` when no active + no pinned.
- [ ] 2.3 Unit tests: composition over a fixture task (subtasks/checklists/comments/custom fields/attachments) + untrusted-data framing present; dedupe active∈pinned; attrition order on oversize (comments→checklists→subtasks→description, heading kept). Send-path test: multi-line body is wrapped in bracketed-paste markers (`\x1b[200~`/`\x1b[201~`), not raw-written. Defer test: a `Running` target queues the send; an `Idle` target delivers.

## 3. Assembler integration

- [ ] 3.1 Extend `pty.rs:396` block: after the vault-note assembly, call `assemble_clickup_context(...)` and concatenate into the same `injected_context` string (one labeled block per source). Preserve the `None`-when-empty behavior so spawns with neither notes nor tasks stay byte-identical.
- [ ] 3.2 Verify resume path: because the assembler runs on fresh + resume, resuming re-reads the current active+pinned task content automatically (test by mutating the mirror between spawns).

## 4. Verbs + binding commands

- [ ] 4.1 `clickup_bind_task(session_id, task_id)` — set `active_clickup_task_id`; if one exists, replace (UI confirms upstream). `clickup_unbind_task(session_id)` — clear it. **Unbind/unpin affect only future spawns/resumes** — they do not retract context already in a running agent's window (injection is at spawn/resume).
- [ ] 4.2 `clickup_pin_task(session_id, task_id)` / `clickup_unpin_task(session_id, task_id)` — ordered, idempotent JSON-array edits (mirror `pin_vault_note`/`unpin_vault_note` semantics).
- [ ] 4.3 `clickup_send_task_as_prompt(session_id, task_id)` — `compose_task_markdown` → **explicit user confirmation** (the content is untrusted + auto-submitted) → deliver via `terminal_paste` (`pty.rs:994`, bracketed paste so the multi-line body is one paste) + `\r` submit. **NOT** the `reinject_pinned_note`/`write_session_data` raw path (would fragment on `\n`). If the target `SessionStatus` (`models.rs:8`) is `Running`, **queue** the send and deliver on transition to `Idle` — never interrupt a generating agent.
- [ ] 4.4 `clickup_spawn_worktree_with_task(workspace_id, task_id, slug?)` — derive slug (task name if absent, existing slug rules: diacritics-stripped + timestamp), slug-collision check, `create_worktree(repo_path, slug)` (`worktree.rs:261`), set `pending_prompts[new_session_id] = compose_task_markdown(task_id)` (`pty.rs:388`), spawn, **and bind the new session to the task** (`active_clickup_task_id`) — binding is the loop-closure this verb exists for, so it binds by default.
- [ ] 4.5 `clickup_reinject_task(session_id, task_id)` — recompose + live-PTY write for an explicit refresh of a running session (never auto).
- [ ] 4.6 Register all commands in the invoke handler (`lib.rs`).

## 5. Frontend

- [ ] 5.1 Task actions in `src/components/clickup/` (panel rows + floating detail): Send as prompt, Spawn worktree, Attach (pin) / Unpin, Bind / Unbind. Keyboard-bound (verify `src/stores/shortcuts.ts` for collisions; `event.code`). **Send-as-prompt shows a confirm dialog with the composed (untrusted) block before submitting.** Unbind/unpin UI states it affects future spawns, not the live window.
- [ ] 5.2 Session-tab indicator of the active task (a chip naming the bound task; click focuses it in the panel).
- [ ] 5.3 Rebind confirmation UI when binding over an existing active task.
- [ ] 5.4 Atoms for binding/pin state per session, fed by command results + `clickup:changed`.

## 6. Verification

- [ ] 6.1 `cd src-tauri && cargo clippy -- -D warnings && cargo test && cargo fmt --check`
- [ ] 6.2 `npx tsc --noEmit`
- [ ] 6.3 Manual walk per proposal § Cómo verifico.
