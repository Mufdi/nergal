# Design — linear-agent-integration

## Context

This change is the Linear counterpart of the archived, iterative-plan-review-APPROVED (5 rounds) `clickup-task-integration` (`openspec/changes/archive/2026-06-12-clickup-task-integration/`). The architecture — three verbs, 1:1 active binding + N pinned context, mirror-only composition, assembler extension, bracketed-paste delivery, fence framing, future-spawn unbind — is **inherited wholesale**, because the ClickUp change already rode Nergal's existing context-injection machinery and that machinery is source-agnostic. This document records the inherited decisions in brief and focuses on the Linear-specific **deltas**: what is different and why.

The load-bearing reuse (all already present, much of it extracted by the ClickUp change):

- **Assembler** (`pty.rs:605`, `assemble_injected_context`): on every spawn — fresh **and** resume — assembles the vault-note block and the ClickUp block into one `injected_context`. Resume re-injection is automatic because both paths run here. `concat_context_blocks` (`pty.rs:628`) currently merges two `Option<String>` sources.
- **Adapter contract** (`ContextInjection`): each adapter folds `injected_context` via its best channel. **No new variant** — Linear context is just more text in the same block.
- **`pending_prompts` / `queue_session_prompt`** (`pty.rs`): a stashed prompt the adapter folds into the launch command so it submits on spawn (used by `clickup_spawn_worktree_with_task`).
- **`paste_to_session(state, session_id, text, submit)`** (`pty.rs`, `pub(crate)` helper extracted by the ClickUp change): bracketed-paste wrap; `\r` written separately after the closing marker when `submit`. Agent-sessions-only.
- **`sanitize_for_pty`** (`pty.rs`): strips control sequences from composed external text before any PTY delivery.
- **`create_worktree(repo_path, slug)`** (`worktree.rs`) + `derive_worktree_slug` (`commands.rs`): the user-initiated worktree machinery.
- **Session binding columns precedent**: `018_clickup_session_binding.sql` added `active_clickup_task_id` + `pinned_clickup_task_ids`; `db.rs` maps them (find/create) and exposes `set_active_clickup_task` / `add_pinned_clickup_task` / `remove_pinned_clickup_task` / `get_pinned_clickup_tasks`.

## Inherited decisions (identical to clickup-task-integration — see its design.md for full rationale)

- **Decision 1 — three distinct verbs, not one**: send-as-prompt (imperative, live write + submit, no binding), spawn-worktree (fresh session on the issue, bound), attach-as-context (referential, rides spawn/resume).
- **Decision 2 — 1:1 active binding + N pinned context**: `active_linear_issue_id` (single write-back target + tab indicator) and `pinned_linear_issue_ids` (ordered, idempotent JSON array, context-only). Both injected (deduped by id); only active is the write-back subject. Rebind **replaces** with UI confirmation; the replaced issue stays in the mirror.
- **Decision 3 — composition from the mirror, never a live call**: `compose_issue_markdown` reads the mirror only; byte-budget cap with a defined attrition order, each step leaving a visible marker, the heading never dropped.
- **Decision 4 — assembler extension, not a parallel path**: extend the single `injected_context` string; every adapter unchanged.
- **Decision 5 — send-to-active uses bracketed paste + submit, NOT the raw write path**: a multi-line block written raw would fragment on each `\n` into partial turns. New worktree uses `pending_prompts`.
- **Decision 6 — composed context is the session's team-authored issue brief** (ClickUp Revision 2 stance): the workspace is a trusted team; the fence is labeled "Linear issue brief", not "untrusted data". Retained technical protections regardless: `sanitize_for_pty` on every delivery, fence-sentinel neutralization, and **confirm-before-submit** for send-as-prompt (the only auto-submitting verb).
- **Decision 7 — unbind/unpin are future-spawn operations**: injection happens at spawn/resume; once context is in a running agent's window it cannot be retracted. `linear_reinject_issue` is the only live-session context op and it only adds.
- **No send-gate** (ClickUp Revision 3): send-as-prompt delivers immediately regardless of run state; a mid-turn send rides the agent's own prompt queueing (Claude Code queues natively). No `UserPromptSubmit` hook, no global-settings edit, no CC-only mechanism. This is the starting point here — the gate is never built.

## Delta 1 — Linear's fixed schema simplifies composition

ClickUp composes name, status, url, description, **subtasks, checklists, custom fields (by type), comments, attachments**. Linear's model (`023_linear_mirror.sql`) has **no checklists and no custom fields**, and its first-class concepts differ. The composed Linear block is:

- **Heading**: `## {identifier} {title}` + `State: {state_name} · Priority: {label} · {url}` (identifier is Linear's `ENG-123`-style key; state is the workflow state name).
- **Description**: markdown body (`linear_issues.description`).
- **Metadata line(s)**: priority (mapped int→label, below), estimate (when present), assignee display name (when present).
- **Labels**: `linear_issue_labels` join → `Labels: a, b, c`.
- **Sub-issues**: `linear_issues WHERE parent_id = ?1 AND stale = 0` → `- {identifier} {title} — {state}`.
- **Comments**: `linear_comments` for the issue → `- **{author}**: {body}` (see Delta 3 — unpopulated in change #1). The author is NOT a plain column: `linear_comments.user_json` stores the whole Linear user object as a JSON blob, so the composer parses it (`display_name` ?? `name`, falling back to `"Unknown"` when the JSON is null or unparseable). The seeded test fixture carries a realistic `user_json` so this parse branch is actually exercised.

**Priority mapping** — to keep the composed brief consistent with what the panel shows, the composer uses the EXACT label set of the frontend `linearPriorityStr` (`components/linear/LinearPanel.tsx:52`, mirrored in `LinearTaskView.tsx`): `0 → No priority`, `1 → urgent`, `2 → high`, `3 → normal`, `4 → low`. Note `3 → "normal"` (Linear's own word), not "Medium", and `0` renders as "No priority"; the composer reuses these strings verbatim rather than inventing a parallel label set that would disagree with the panel. Rendered as a label, never the raw int.

**Attrition order** (simpler than ClickUp's four-stage): drop oldest comments first → collapse the sub-issue list to a count → head/tail-truncate the description. The heading (identifier + title + state + url) is never dropped. There are no checklist/custom-field stages.

**Alternatives considered**: replicate ClickUp's checklist/custom-field rendering as empty sections — rejected, Linear has no such data, dead code. Render priority as the raw int — rejected, opaque to the agent; the label is how the panel already shows it.

## Delta 2 — attachments and relations are NOT in the mirror, so compose omits them

ClickUp mirrors attachments (`clickup_attachments`) and composes them as `- title (url)` links. Linear's attachments and issue relations are **not mirrored**: the panel fetches them live on detail-open via `linear_issue_detail` (`linear/mod.rs:227`), and `023_linear_mirror.sql` has no attachments/relations table. Decision-3 (compose reads the mirror only, never a live call) therefore **excludes** attachments and relations from the composed block — composing them would require a live API call at compose time, which the contract forbids and which would add latency on every spawn/resume.

This is an accepted, documented divergence, not a gap: the agent gets the issue's substance (title, description, state, priority, assignee, labels, sub-issues). Inline images in the description survive as their markdown `uploads.linear.app/...` URLs (the agent sees the link text; image fetching is the panel's concern, not the agent's). If a later change mirrors attachments/relations, compose gains those sections for free.

**Alternative considered**: live-fetch the detail at compose time to include attachments/relations — rejected, violates Decision 3 (mirror-only), adds network latency to the spawn path, and couples context assembly to the poller's auth/rate-limit surface.

## Delta 3 — comments are mirrored-table-present but poller-unpopulated in change #1

`023_linear_mirror.sql` defines `linear_comments`, but the change-#1 poller does **not** populate it (no `upsert_comment` in `linear/mirror.rs` — verified). Compose reads `linear_comments` for the issue regardless: today it returns empty, so the Comments section is simply absent; when a future change (likely `linear-writeback` or a poller enrichment) syncs comments, compose renders them with **no further change**. Reading the table now rather than hard-omitting the section is the forward-compatible choice and matches ClickUp's `read_comments`-shaped contract.

**Alternative considered**: omit comments entirely until they are synced — rejected, it would require a second edit to `integration.rs` later; reading an empty table is harmless and self-activating.

## Delta 4 — composer reads via direct SQL, no new mirror.rs view

ClickUp's `integration.rs` calls `mirror::read_custom_values` / `read_comments` / `read_checklists` / `read_attachments` helpers. Linear's `mirror.rs` exposes only `read_issues` (panel view-model) and `read_teams` — there are no per-section read helpers. Rather than add a `read_*` API surface to `mirror.rs` for a single consumer, `integration.rs` composes via **direct `conn.query_row` / `query_map`** against `linear_issues` / `linear_issue_labels` / `linear_labels` / `linear_comments` (the same direct-query style ClickUp's `compose_sections` already uses for its core row). This keeps the composer self-contained and avoids widening the mirror's public API for change-#2-only needs.

## Delta 5 — slug + naming derive from the issue identifier/title

ClickUp's spawn derives the slug from the task name. Linear issues have both an `identifier` (`ENG-123`) and a `title`. The spawn slug derives from the **title** (human-readable, matching ClickUp's name-based slug), via the existing `derive_worktree_slug` (diacritics-stripped + timestamp). The new session's `name` is the issue title (or `identifier — title` if title alone is ambiguous — title is the default to mirror ClickUp). The composed block's heading already carries the identifier so the agent knows the canonical issue key.

## Delta 6 — verb keybindings live in the detail wrapper, not a global shortcut

ClickUp's bare-letter verb keys (S send, W spawn, P pin, B bind, R reinject, plus C/O/T) are NOT in `ClickUpTaskView`/`ClickUpPanel` and NOT in `stores/shortcuts.ts`. They live in a local `VERB_KEYS` keydown handler inside the detail/tab wrappers (`components/clickup/ClickUpTaskDetail.tsx:79-99` and `ClickUpTaskTab.tsx:26-74`), gated by `e.code` and the detail focus zone. The Linear equivalent wrapper is `components/linear/LinearTaskDetail.tsx` (it already hosts `useLinearIssueController` and the `data-focus-zone='linear'` zone). The verbs are mirrored there as a local `VERB_KEYS` handler, plus `ToolbarAction` buttons in the detail toolbar (the `data-nav-key` cursor already handles Enter/Space activation). The collision surface is therefore **the Linear detail zone's own keys** — its existing `copyid`/`open` nav keys (`LinearTaskView.tsx`) and any letter already bound in that zone — NOT global `shortcuts.ts`. "Open as tab" (ClickUp's T verb + `ClickUpTaskTab`) is OUT of scope for this change (the proposal lists three verbs, not a tab surface); only S/W/P/B/R are added.

## Delta 7 — the `Session` struct gains two fields → every struct literal must update

`active_linear_issue_id` + `pinned_linear_issue_ids` are non-`Default` additions to `struct Session` (`models.rs:44`). Rust requires every struct literal to name them, so the Schema phase touches more than the db mapping: `commands.rs:914`, `mcp/directory.rs:294`, `clickup/mod.rs:864` (the ClickUp worktree-spawn literal), the new `linear_spawn_worktree_with_issue` literal, and the test literals (`db.rs:1305/1501/1584/1712`, `pty.rs:1521`, `clickup/integration.rs:464`). All seed `None` / `Vec::new()` for the two new fields except where a test exercises a Linear binding. (`#[serde(default)]` covers wire deserialization, not Rust struct-literal construction — that is the compile trap.)

## Migration

`024_linear_session_binding.sql` is the exact structural mirror of `018_clickup_session_binding.sql`:

```sql
ALTER TABLE sessions ADD COLUMN active_linear_issue_id TEXT;
ALTER TABLE sessions ADD COLUMN pinned_linear_issue_ids TEXT;
```

Both nullable; `pinned_linear_issue_ids` is a JSON array (NULL = no pins). Registered in `db.rs`'s migration list after `023`. The `find_session` / `find_sessions` SELECTs gain two columns (indices shift after the existing clickup pinned column at index 15 → linear active=16, pinned=17); `create_session` INSERT gains two params; a `parse_pinned_linear_issue_ids` helper mirrors `parse_pinned_clickup_task_ids`.

## Risks

- **[Risk] Migration-number race**: `024` is the next free after `023_linear_mirror.sql`. Single registration point (`db.rs` list), reconciled at merge — not an independent "next free" grab.
- **[Risk] Byte budget**: a long description or (future) comment thread bloats `injected_context` → the Delta-1 attrition order caps it, each step marked, omissions logged. Budget shares the same `CONTEXT_BUDGET_BYTES = 32KB` as ClickUp (the vault block already claims 64KB of the 128KB session-log cap; vault + ClickUp + Linear must stay under it — three sources now share the remaining headroom, so the Linear budget matches ClickUp's 32KB and the combined worst case is bounded by attrition on each).
- **[Risk] Column residue on revert**: additive `ALTER TABLE … ADD COLUMN` is forward-safe but not cleanly reversible pre-SQLite-3.35. Noted, accepted (forward-only convention, same as `018`).
- **[Risk] Rebind data loss**: replacing the active issue silently could surprise → UI confirms; the dropped issue stays in the mirror.
- **[Risk] Stale injected context**: an issue changes in Linear after a session spawned → resume re-injects current (automatic via `pty.rs:605`); for a live session, `linear_reinject_issue` offers explicit refresh (never auto).
- **[Risk] Agent `Unsupported` for injection**: per the Obsidian/ClickUp contract, attach silently records but the session-tab chip indicates injection is unsupported for that agent; no PTY-typed fallback.
- **[Risk] Fence-sentinel injection via comment/description**: a crafted Linear body containing the literal fence sentinel could close the block early → `neutralize_fence_sentinels` mangles both sentinels before fencing (mirrors ClickUp; covered by a unit test).
