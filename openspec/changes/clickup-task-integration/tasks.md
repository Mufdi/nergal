# Tasks — clickup-task-integration

> Depends on `clickup-sync` (mirror + panel). Migration number = next free after
> `clickup-sync`'s, registered in `db.rs:132` in order.

## 1. Schema — session binding

- [x] 1.1 New migration `0NN_clickup_session_binding.sql`: `ALTER TABLE sessions ADD COLUMN active_clickup_task_id TEXT;` and `ALTER TABLE sessions ADD COLUMN pinned_clickup_task_ids TEXT;` (nullable JSON array, same pattern as `pinned_note_paths`). Register in `db.rs:132`. **Number = N+1 relative to clickup-sync's migration** (reconcile at merge — not an independent "next free" grab that could collide on a fresh DB).
- [x] 1.2 Extend the `Session` model + `find_session` mapping to carry the two new fields (active id + parsed pinned ids).

## 2. Context composition

- [x] 2.1 `clickup/integration.rs`: `compose_task_markdown(mirror, task_id) -> String` — block framed as **UNTRUSTED external data** (a labeled fence stating the enclosed ClickUp content is data, not instructions): name+status+url heading, markdown description, subtasks (name+status), checklists (items+resolved), custom fields (by type; computed fields like `automatic_progress` read-only), recent N comments (author+text), attachments as `- title (url)` links. Reads the mirror only.
- [x] 2.2 `assemble_clickup_context(mirror, session) -> Option<String>` — composes active ∪ pinned (dedupe by id), capped at a byte budget with a **defined attrition order across all sections** (comments oldest-first → checklists to counts → subtasks to counts → description head/tail; heading never dropped), each step marked, omissions logged. `None` when no active + no pinned.
- [x] 2.3 Unit tests: composition over a fixture task (subtasks/checklists/comments/custom fields/attachments) + untrusted-data framing present; dedupe active∈pinned; attrition order on oversize (comments→checklists→subtasks→description, heading kept). Send-path test: multi-line body is wrapped in bracketed-paste markers (`\x1b[200~`/`\x1b[201~`), not raw-written. Defer test: a `Running` target queues the send; an `Idle` target delivers.

## 3. Assembler integration

- [x] 3.1 Extend `pty.rs:396` block: after the vault-note assembly, call `assemble_clickup_context(...)` and concatenate into the same `injected_context` string (one labeled block per source). Preserve the `None`-when-empty behavior so spawns with neither notes nor tasks stay byte-identical.
- [x] 3.2 Verify resume path: because the assembler runs on fresh + resume, resuming re-reads the current active+pinned task content automatically (test by mutating the mirror between spawns).

## 4. Verbs + binding commands

- [x] 4.0 **(Revision 1, final — read design.md Revision 1 first; it is the contract for 4.0a-4.0e)** Send-gate for the defer:
  - [x] 4.0a `setup.rs`: new HookDef `{ event: "UserPromptSubmit", command: "cluihud hook send user-prompt", is_async: true }` alongside the existing sync `inject-edits` entry. **Wrapper-form synthesis** in the insertion path: if existing cluihud entries in the target settings file use the `cluihud-conditional.sh` wrapper, synthesize the new command in wrapper form (strip an existing wrapper entry's trailing args, append this entry's); bare only when no wrapper entries exist. Unit tests: wrapper synthesis, mixed forms, no-prior-entries, idempotent re-run. (CC only — Codex descoped: `codex/setup.rs:70-80` merge deletes sibling entries per event; backlog follow-up.)
  - [x] 4.0b `SendGate` managed state — ONE `Mutex` over `{run_state: HashMap, queued: HashMap (one slot, replace+log), guard_verified: HashSet}`. Dispatcher writer arms: `UserPromptSubmit` (`server.rs:732`) → Running + guard_verified; `Stop` (`server.rs:681/716`) → Idle + destructive pop, delivery OUTSIDE the lock, WITHOUT auto-`\r`, + notification. Key STRICTLY by `cluihud_session_id`, skip `None` (never the `unwrap_or(session_id)` display fallback). Do NOT read/write the persisted `sessions.status` column (`db.rs:402` has no callers — dead).
  - [x] 4.0c `paste_to_session(state, session_id, text, submit: bool)` helper extracted from `terminal_paste` (`pty.rs:994`): bracketed wrap, `\r` as a separate write AFTER the closing marker when `submit`; agent-sessions-only contract (the aux `::` branch, `pty.rs:1010-1025`, stays in `terminal_paste`). Write error → drop + log + `clickup:send-dropped{reason}`.
  - [x] 4.0d Purges (run_state + queued + guard_verified; emit `clickup:send-dropped` only when an entry was actually removed): `kill_session_pty` (`pty.rs:929`), `SessionEnd` arm (`server.rs:586`), PTY-reader EOF branch (`pty.rs:194-200`, same place as the Obsidian crash finalizer) — the EOF purge uses an **instance-identity gate**: the reader closure captures its own `eof_pty_id`; purge iff `session_ptys.get(&eof_session)` is absent OR equals that pty_id (crash → own → purge; kill-no-respawn → absent → idempotent purge; kill→respawn → new pty_id ≠ own → skip). NEVER a plain presence/absence check — crash does not de-register from `session_ptys`, so absence-gating would no-op the crash purge entirely.
  - [x] 4.0e `clickup_cancel_queued_send` + `clickup_force_deliver_queued_send` — both pop destructively under the same gate lock (no double-delivery with a concurrent Stop drain). Frontend events: `clickup:send-queued`/`-delivered`/`-dropped{reason}`.
- [x] 4.1 `clickup_bind_task(session_id, task_id)` — set `active_clickup_task_id`; if one exists, replace (UI confirms upstream). `clickup_unbind_task(session_id)` — clear it. **Unbind/unpin affect only future spawns/resumes** — they do not retract context already in a running agent's window (injection is at spawn/resume).
- [x] 4.2 `clickup_pin_task(session_id, task_id)` / `clickup_unpin_task(session_id, task_id)` — ordered, idempotent JSON-array edits (mirror `pin_vault_note`/`unpin_vault_note` semantics).
- [x] 4.3 `clickup_send_task_as_prompt(session_id, task_id)` — `compose_task_markdown` → **explicit user confirmation** (the content is untrusted + auto-submitted; the confirm payload carries `guard_active` derived from `guard_verified` at runtime, with the static settings.json hint — global file only — choosing the notice text: "run `cluihud hook setup`" vs "restart this session") → if the gate reads Idle: deliver via `paste_to_session(…, submit: true)` (bracketed paste + `\r` after the closing marker). **NOT** the `reinject_pinned_note`/`write_session_data` raw path (would fragment on `\n`). If Running: **enqueue** under the same lock (check-and-enqueue atomic) and return "queued"; the Stop drain delivers later WITHOUT auto-submit.
- [x] 4.4 `clickup_spawn_worktree_with_task(workspace_id, task_id, slug?)` — derive slug (task name if absent, existing slug rules: diacritics-stripped + timestamp), slug-collision check, `create_worktree(repo_path, slug)` (`worktree.rs:261`), set `pending_prompts[new_session_id] = compose_task_markdown(task_id)` (`pty.rs:388`), spawn, **and bind the new session to the task** (`active_clickup_task_id`) — binding is the loop-closure this verb exists for, so it binds by default.
- [x] 4.5 `clickup_reinject_task(session_id, task_id)` — recompose + live-PTY write for an explicit refresh of a running session (never auto).
- [x] 4.6 Register all commands in the invoke handler (`lib.rs`).

## 5. Frontend

- [x] 5.1 Task actions in `src/components/clickup/` (panel rows + floating detail): Send as prompt, Spawn worktree, Attach (pin) / Unpin, Bind / Unbind. Keyboard-bound (verify `src/stores/shortcuts.ts` for collisions; `event.code`). **Send-as-prompt shows a confirm dialog with the composed (untrusted) block before submitting**, including the guard-active/inactive notice from the confirm payload. Unbind/unpin UI states it affects future spawns, not the live window.
- [x] 5.1b Pending-send surface: indicator for a queued send with **cancel** and **deliver-now** actions (wired to 4.0e); toasts for `clickup:send-queued`/`-delivered`/`-dropped{reason}` outcomes (the UI never shows "queued" without learning the outcome).
- [x] 5.2 Session-tab indicator of the active task (a chip naming the bound task; click focuses it in the panel).
- [x] 5.3 Rebind confirmation UI when binding over an existing active task.
- [x] 5.4 Atoms for binding/pin state per session, fed by command results + `clickup:changed`.

## 6. Verification

- [x] 6.1 `cd src-tauri && cargo clippy -- -D warnings && cargo test && cargo fmt --check`
- [x] 6.2 `npx tsc --noEmit`
- [ ] 6.3 Manual walk per proposal § Cómo verifico.
