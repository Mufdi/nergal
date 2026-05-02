# REVIEW — scratchpad-floating-panel

## Iprev round 1 attempts (all failed — quota / capacity)

| Evaluator | Status | Reason |
|-----------|--------|--------|
| Claude (`claude -p`) | ❌ | "You've hit your limit · resets 6:40pm (America/Santiago)" |
| Gemini (`gemini -p`) | ❌ | "API Error: You have exhausted your capacity on this model" |
| Codex (`codex exec`) | ❌ | CLI not installed |

**Decision**: Architect performed a self-critical review documented below. This is **NOT** a cross-model iprev — same context, same blind spots possible. Flagged for re-run when external evaluator quota returns.

---

## Self-review by Architect (round 1) — extensibility-focused

> Lens: validate that proposal.md "Extensibility hooks" are load-bearing, not ornamental. Stress-test claims against implementation reality.

### Findings

**1. (HIGH) `scratchpad_meta.tab_id` identity is undefined → storage-abstraction claim is partially ornamental**

The proposal in §Impact lists `scratchpad_meta(tab_id TEXT PRIMARY KEY, ...)` but never defines what `tab_id` represents. Three plausible interpretations:

- (a) Filename without extension → coupling: changing `scratchpadPath` keeps the same `tab_id`, but renaming a file (e.g. via Obsidian) breaks it.
- (b) Absolute file path → changing `scratchpadPath` orphans every metadata row.
- (c) Stable UUID stored in DB only → requires a sidecar (filename-to-UUID map), which is exactly what the "files in disk are sufficient" claim wanted to avoid.

Without resolving this, the "sync with Obsidian without migration" claim is not delivered: Obsidian renaming files would orphan metadata under (a) and (c), and (b) is brittle.

**Required fix**: pick (c) with the UUID embedded in the filename itself (e.g. `scratch-{uuid}.md`) — readable enough for Obsidian, stable across path/rename, recoverable from disk. Make this explicit in proposal.

**2. (HIGH) Send-to-prompt extensibility claim contradicts no-coupling-with-sessions**

§Extensibility hooks → "Send-to-prompt deliberately out of scope, but enabled" promises that `invoke("terminal_input", { sessionId: activeSessionIdAtom, text: selection })` is a thin future hook. But §Impact says "El store `scratchpad.ts` no importa `activeSessionIdAtom`". Implementing send-to-prompt requires importing it — that breaks the no-coupling invariant.

**Required fix**: either (i) remove the send-to-prompt extensibility claim and accept it as a future scope decision (no current promise), or (ii) reframe as "send-to-prompt would live in a separate adapter file that imports both — the scratchpad store stays pure". Option (ii) preserves both claims honestly.

**3. (HIGH) Floating panel chrome reusability claim is ornamental**

§Extensibility hooks → "Si se agrega un segundo floating tool, se reutiliza el chrome." But §Impact specifies `scratchpad_panel(geometry_json TEXT)` as a **singleton row**. A second floating tool cannot reuse the same row; reusing the chrome requires migrating to a multi-row keyed schema — that's a breaking schema change.

**Required fix**: change schema to `floating_panel_geometry(panel_id TEXT PRIMARY KEY, geometry_json TEXT, opacity REAL, ...)`. Scratchpad uses `panel_id = 'scratchpad'`. Multi-tool future is now genuinely free.

**4. (HIGH) Watcher own-write tracking strategy is undefined → race risk**

§Impact mentions `notify-debouncer-full` and §Open questions defers strategy to Mode B. But the brief casually notes "ignorar notify events que coincidan con un write reciente del propio editor" without specifying. Combined with autosave-debounce-300ms + watcher-debounce-200ms, there is a real risk:

- Editor write A → notify event A1 (200ms later) → frontend reads file → buffer replaced with own data (idempotent if no edit happened in between, but not guaranteed under fast typing).
- If user types between A's write and A1's notify, the buffer has unsaved deltas → notify-driven reload **clobbers in-memory edits**.

**Required fix**: specify own-write tracking explicitly — per-file `last_self_write_at` timestamp, ignore notify events for files whose `mtime - last_self_write_at < 500ms`. Document this in proposal §Impact.

**5. (MEDIUM) Multi-instance "no extra logic" claim is false**

§Extensibility hooks → "Multi-instance: dos cluihud corriendo sobre el mismo path se mantienen sincronizados sin lógica adicional." This is false: two writers on the same file with `tmp + rename` will lose data without a lock or merge protocol. The watcher will surface the most recent write, but interleaved writes will silently drop one user's edits.

**Required fix**: downgrade the claim to "Multi-instance read-mostly works; concurrent writes need a lock — out of v1 scope. The watcher detects external writes including from a sibling cluihud, allowing each instance to refresh."

**6. (MEDIUM) Path change semantics for open tabs is undefined**

§Impact → "Cambiar el path no migra: re-apunta y muestra toast." But what happens to tabs currently open in the editor? Three choices:

- (a) Close all tabs, reload from new path.
- (b) Keep open tabs in memory, save them at new path on next autosave.
- (c) Discard in-memory edits, reload from new path.

(b) silently moves files between paths (surprise), (c) loses unsaved work. (a) is least surprising but interrupts the user.

**Required fix**: spec (a) explicitly — "all tabs close, panel reloads from new path. In-flight unsaved buffers flush to old path before close."

**7. (MEDIUM) Purga 30d uses unspecified timestamp**

§What Changes → "Purga automática a los 30 días en startup." Which timestamp? `mtime` of the file in `.trash/`, `ctime`, or a sidecar? `mtime` is preserved across `mv`, so a freshly trashed file with old `mtime` gets purged immediately. `ctime` resets on `mv`, but is not portable.

**Required fix**: when soft-deleting, rename the file to embed the trash timestamp: `scratch-{uuid}-trashed-{epoch}.md`. Purge based on the embedded epoch. Survives `mtime` quirks and clock skew.

**8. (MEDIUM) Backdrop interaction model is underspecified**

The proposal says the floating panel "no bloquea interacción con los panels detrás" but doesn't specify pointer-events behavior. If the panel has a transparent margin or rounded corners with shadow that extends beyond the visible chrome, mouse events on the shadow region either (a) reach the panel or (b) reach the panel below.

**Required fix**: specify "panel root has `pointer-events: auto` on the visible card; shadow/margin region has `pointer-events: none` so clicks pass through to the workspace below."

**9. (LOW) Background color inconsistency**

Proposal says `rgba(20, 20, 22, 0.9)` in §What Changes. DESIGN.md tokens: `--card` = `#0a0a0b`, `--secondary` = `#1c1c1e`. `rgb(20,20,22)` ≈ `#141416`, which is closer to `--background` (#141415) than `--card`. Using `--card` (#0a0a0b) with alpha 0.9 would match the island system; the current value silently introduces a fourth surface tier.

**Required fix**: use `rgba(10, 10, 11, 0.9)` (= `--card` at 0.9 alpha). Note in proposal that it's an existing-token-with-alpha, not a new tier (DESIGN.md §6.1 anti-pattern: "No new background colors").

**10. (LOW) Spec scaffolding for `openspec/specs/scratchpad/` not noted**

`spec_target: scratchpad` (new) — but the proposal does not say when/where the spec is created. Convention in this repo: specs are authored in `openspec/specs/<id>/spec.md` post-archive via `/openspec-sync archive`. Worth a single line in §Impact stating that the spec scaffold is generated at archive time, not pre-implementation.

**Required fix**: add a sentence in §Impact under "DB / settings" mentioning that the new spec lives at `openspec/specs/scratchpad/` and is created during archive.

**11. (LOW) Test coverage for FS ops is too vague**

§Implementation steps step 1 says "Tests unitarios sobre tmpdir" with no list. Needed at minimum:

- `tmp + rename` is atomic under partial-write simulation.
- `mv` to `.trash/` preserves filename and timestamp embedding.
- Purge respects the embedded trash-epoch (not `mtime`).
- Watcher does NOT loop on own writes (own-write tracking validated).

**Required fix**: enumerate these in proposal step 1.

---

### Severity rollup

- HIGH: 4 (items 1-4) → must address before Mode B
- MEDIUM: 4 (items 5-8) → should address; partial deferral acceptable with explicit notes
- LOW: 3 (items 9-11) → polish, address opportunistically

VERDICT: REVISE

---

## Iprev round 2 — pending external evaluator

When Claude/Gemini quota returns, re-run iprev against the **revised** proposal (post-revision below). Until then, this self-review serves as a placeholder — flagged risk: same-blindspot bias.

---

## Build divergences

<!-- Reviewers append here post-Mode B -->
