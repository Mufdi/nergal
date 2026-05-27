## Context

Nergal already owns three properties that this change leverages:
- Every PTY-spawned session emits a hook stream into `src-tauri/src/hooks/server.rs`. The dispatcher there owns the per-session state used by every other panel (`activityMapAtom`, `taskMapAtom`, `fileMapAtom`, `costMapAtom`, `agentStatusMapAtom`). `HookEvent::SessionEnd` is already a defined variant (events.rs:8-9) but is currently consumed only by the frontend (clears UI state). Same hooks for any agent (CC / Codex / OpenCode / Pi) — adapter-agnostic by construction.
- The CLI binary `cluihud` is bundled with the `.deb`, registered as the system handler for hook events, and uses `clap` derive subcommands (`main.rs`). Adding a new subcommand is a one-line enum variant.
- React markdown rendering is unified on `react-markdown@10.1.0` + `remark-gfm@4.0.1` (confirmed in `package.json`), used by `TranscriptViewer.tsx:57` and `MarkdownView.tsx:13`. Code blocks come through the AST as `code` / `inlineCode` nodes, so a `unist-util-visit` plugin can safely traverse `text` nodes without touching them.
- SQLite migrations follow `src-tauri/migrations/00N_<name>.sql` applied by `db.rs:86-94`. The `FloatingPanel` chrome at `src/components/floating/FloatingPanel.tsx` is generic (parameterized by `panelId`), keyed against `floating_panel_geometry` rows.

Constraints that shaped the design:
- **No vault auto-bootstrap.** Users without Obsidian or with a curated layout would experience phantom folders the moment they touched Nergal. Every write goes to a path the user explicitly mapped.
- **Vault stays the source of truth.** Nergal does not maintain a parallel index of vault content. Wikilink rendering delegates resolution to Obsidian (clicking a non-existent note triggers Obsidian's own create-modal). Search uses ripgrep over the live filesystem.
- **App-close must not block.** Heavy work (MOC snapshot + N1 backlink propagation) runs in a detached child process, not in the Tauri shutdown handler.
- **Crash-safe by separation.** The continuous log (`#2`) is written in-process on every relevant event, so even a hard SIGKILL preserves activity history. Only the per-session snapshot (`#11`) can be lost — and that loss is recoverable by re-running the runner over the existing log.

## Goals / Non-Goals

**Goals**
- Provide a contract surface (channel registry) so any future feature can plug into the vault without re-wiring config UX.
- Bidirectional `obsidian://` ↔ `cluihud://` URI navigation that doesn't depend on either app running.
- Wikilink rendering that survives code blocks, embeds, aliases, headings, block refs, and live tables.
- Continuous session log so an external observer can read what's happening without polling Nergal.
- MOC + backlink generation that can complete after Nergal exits, with recovery on next launch.
- Search engine shape that is the natural parent of every future "find X across cluihud" feature.

**Non-Goals**
- Replacing or proxying any part of Obsidian's own UI (graph view, properties editor, daily notes plugin, Templater, Dataview). Nergal does not render note bodies in-app beyond the wikilink-as-link conversion.
- Auto-detecting which vault notes are "relevant" to a session. Every reference is explicit (user-pinned, `@@`-cited, or wikilink in plan/scratchpad text).
- Two-way sync of any kind (no edit-in-Nergal-pushes-to-Obsidian, no edit-in-Obsidian-pushes-to-Nergal mid-session). Hot reload of pinned notes lives in the deferred `obsidian-context-injection` change.
- LLM-assisted MOC summarization. The MOC is template + structured data only; the user already has Claude in-session for prose.
- Semantic search. v1 is ripgrep over filenames + content. The search engine API leaves room for a v2 vector index but does not require it.
- Filesystem-Local REST API integration with the Obsidian plugin. Wikilinks render via the `obsidian://` URI scheme; if a user later wants Dataview-aware queries, that's an additive future change.

## Decisions

### 1. Channel registry shape (F)

**Decision**: Per-workspace SQLite row in a new `obsidian_config` table, with a single optional global override at `~/.config/cluihud/obsidian.toml`. Schema:

```sql
CREATE TABLE obsidian_config (
    workspace_id          TEXT PRIMARY KEY REFERENCES workspaces(id) ON DELETE CASCADE,
    vault_root            TEXT,
    vault_name            TEXT,
    session_log_path      TEXT,
    quick_capture_path    TEXT,
    moc_path              TEXT,
    templates_path        TEXT,
    backlinks_enabled     INTEGER NOT NULL DEFAULT 0,
    render_wikilinks      INTEGER NOT NULL DEFAULT 1,
    updated_at            INTEGER NOT NULL
);
```

All `*_path` columns nullable. A NULL `vault_root` disables every feature (no buttons, no shortcuts respond, no plugins load). The global TOML is read after the per-workspace row is fetched: any non-null TOML field overrides the workspace value for that field only. This lets a user with a single vault put `vault_root` in TOML once and never re-enter it per workspace.

**Why**
- Per-workspace because users with multiple projects may keep separate vaults (work / personal) or want different MOC destinations.
- TOML override because writing the same `vault_root` to every workspace row violates DRY for the common case.
- Tightly typed column list (vs. JSON blob) because the schema is the contract, and SQLite enforces it.
- Cascading delete because the row is meaningless without the workspace.

**Alternatives considered**
- *Single global TOML, no SQLite*: rejected. Loses per-workspace MOC destinations and forces a rewrite when the user wants the bridge for project A but not project B.
- *JSON column inside `workspaces`*: rejected. Hides the schema from anyone reading the SQL.
- *Multi-row key-value (`obsidian_config(workspace_id, key, value)`)*: rejected. Forces the writer to know every key name as a string constant and complicates "fetch full config" into N round-trips or a join.

### 2. Activation gate

**Decision**: The frontend exposes a derived atom `obsidianEnabledAtom = (vault_root != null)`. Every Obsidian-touching UI primitive (shortcut handler, settings section visibility, button rendering, remark plugin registration) gates on this atom. The backend mirrors the same check before doing IO: if `vault_root` is null, channel writers no-op silently.

**Why** — Users who never touch Obsidian must not see traces of the integration. Hiding by config means a clean default state without dead UI affordances.

### 3. Wikilink rendering (G) as a remark plugin

**Decision**: New file `src/lib/markdown/remarkObsidianLinks.ts`. The plugin uses `unist-util-visit` to walk the AST, ignores `code` / `inlineCode` nodes, and replaces matching `text` nodes with `link` nodes pointing at `obsidian://open?vault=<vault_name>&file=<encoded path>` plus an optional `#heading` or `#^block-id`.

Regex (single-pass, captures all four wikilink subtypes):

```
/(?<!\\)\[\[([^\[\]|#^]+?)(?:#([^\[\]|^]+))?(?:\^([^\[\]|]+))?(?:\|([^\[\]]+))?\]\]/
```

Plus a path detector for absolute paths starting with `vault_root` (strip prefix + `.md`, build URI). Escaped `\[[` is ignored (lookbehind).

**Why a remark plugin and not a regex on rendered HTML**
- Code blocks are skipped automatically by the AST shape (no manual heuristics).
- The output `link` node inherits the existing `components.a` styling and click handling in both `TranscriptViewer` and `MarkdownView`.
- Memoization at the message level (already done by React on `entry.content`) carries through.
- A future spec change (e.g. add a hover preview) extends the plugin without touching consumers.

**Scope (validated)**
| Surface | Plugin applied? | Notes |
|---|---|---|
| Terminal pane (wezterm-term canvas) | ❌ | Glyph atlas render, no markdown layer. Wikilink-in-PTY would require intercepting Claude's stdout, which violates project scope. |
| `TranscriptViewer.tsx` (assistant messages) | ✅ | Same `react-markdown` instance as today. |
| `MarkdownView.tsx` (plan panel) | ✅ | Same. |
| `AnnotatableMarkdownView.tsx` (spec annotations) | ✅ | Same stack, anchoring offsets unaffected. |
| Future MOC preview panel | ✅ | Inherits the plugin for free. |
| User-typed messages in scratchpad | ✅ — applies to user role too. If the user writes `[[Note]]` in scratchpad markdown preview, it linkifies. |

### 4. Post-session background process (M3 infra)

**Decision**: A detached child `cluihud post-session` runs every time a session ends. Architecture:

```
Tauri app                                Bg process (cluihud post-session)
─────────────                            ─────────────────────────────────
SessionEnd hook fires                    
  → write marker file                    
    ~/.config/cluihud/pending-mocs/      
      <session-id>.json                  
  → spawn detached                       
    setsid() + close stdio        ───►   take global PID lock
                                         ~/.config/cluihud/post-session.lock
WindowEvent::CloseRequested              if lock fails: exit 0 (sibling running)
  → for each active session:             
    write marker file                    while pending markers exist:
  → spawn detached (same binary)         · take per-session file lock
  → app.exit()                           · read marker
                                         · build MOC from session_log channel
On next launch                           · update reverse backlinks
  → scan pending-mocs/                   · delete marker
  → if any entries with                  release lock, exit
    age > 10 min: spawn runner            
    (recovery path)                      
```

**Decisions inside this design**
- **One runner at a time, globally.** The first `cluihud post-session` invocation takes a global lock; subsequent invocations see the lock and exit 0 immediately (their job is already in the queue via the marker they didn't need to create — the markers were written *before* they spawned). This avoids two runners racing on the same MOC.
- **Per-marker lock too, for the rare scenario where the user reopens Nergal while a runner is mid-flight on session X and session X gets a new marker before the runner finishes.**
- **Detachment via `setsid()` + closing fds 0/1/2**, on Linux only (target platform). `pre_exec` callback on `std::process::Command`.
- **Log to `~/.config/cluihud/logs/post-session.log`** (rotating at 5 MB, 3 generations). On next Nergal launch, the app inspects the tail; if the last entry is `ERROR`, surface a Sileo toast "Last session log failed — see <path>".
- **Markers older than 10 minutes at app start are considered stale recoveries.** The runner re-processes them on the recovery path.

**Why a detached process and not a tokio task**
- Tokio tasks die with the runtime. Tauri's shutdown sequence drops the runtime before the dispatcher would have had a chance to flush. Detaching is the only way to outlive the shutdown.
- The process boundary doubles as a fault boundary: if the MOC writer panics, only the bg process dies. The user's vault doesn't end up half-written.
- The same binary already has the CLI surface, the config loader, and the channel writer logic. No new dependency tree.

**Alternatives considered**
- *Tokio task on close*: rejected (above).
- *Shell out to a hidden Tauri instance*: rejected. Spawning another GUI process for one-shot work is excessive.
- *systemd user service*: rejected. Adds install complexity and requires user-level systemd, which not every distro has by default.

### 5. Session log format (#2)

**Decision**: One markdown file per workspace at `<session_log_path>` (which is a path *to a file*, not a directory — per F, the user names the file). New sessions append a header block:

```markdown
## Session "<name>" — <ISO start>
- Agent: <agent_id> (<model_name>)
- Workspace: <workspace name>
- Cwd: <cwd at start>

### Activity
- <ISO> · <event>
- <ISO> · <event>
…

### Session ended at <ISO end>
- Final cost: $<USD>
- Files touched: <count> (<list of basenames>)
- Tasks completed: <count>
```

Each event is one line: `- <ISO> · <verb> <subject>`. Verbs come from the hook event type:
- `Tool` `Read src/foo.rs`
- `Edit` `src/foo.rs`
- `Task` `created: "Refactor X"`
- `Plan` `ready: <plan_path>`
- `Stop` `(reason: <stop_reason>)`
- `Permission denied` `: <tool> — <reason>`

The MOC reads this file at session-end and produces the snapshot.

**Why append per event and not buffer**
- Crash safety: if Nergal segfaults, the user still has every event up to the second before crash.
- Trivial implementation: extend `process_event` in `hooks/server.rs` with a single `channel_writer.append(…)` per relevant arm. No background task, no buffering, no flush logic.
- Atomic appends: writing < 4 KB to a single file with `O_APPEND` is atomic per POSIX. Concurrent sessions writing to the same workspace log don't interleave at the byte level (they may interleave at the line level — that's the desired behavior for a chronological feed).

**Why one file per workspace and not per session**
- Workspace-level files match how the user thinks ("what happened on project X this week"). Per-session would produce hundreds of micro-files.
- The MOC (`#11`) is the per-session view. Two destinations cover both reading modes.

### 6. MOC + reverse backlinks (#11 + N1)

**Decision**: MOC writer is a pure transformation of (continuous log block for this session) + (DB state for cost, tasks, files, agent status) + (git diff stats for the worktree). No live atoms read — everything from disk + DB. Output goes to `<moc_path>/<session-name-slug>-<YYYY-MM-DD>.md`. Re-runs are idempotent (same slug = overwrite).

Backlink updater (N1): after MOC is written, walk its AST, collect every `[[Wikilink]]`, for each target note in the vault append (or update if already present) a `## Referenced in Nergal sessions` section. The section is delimited by HTML comment markers `<!-- nergal-backlinks-start -->` / `<!-- nergal-backlinks-end -->` so the updater can re-find and rewrite its own region without touching anything else in the note. Targets that don't exist in the vault are skipped (Obsidian will create them when clicked).

**Why bracket markers and not a section heading**
- A heading is human-editable; if the user moves or renames it, the updater can't find it next time.
- HTML comment markers are invisible in Obsidian's rendered view, survive renames of the surrounding content, and are unambiguous.

### 7. Global search engine (M2 infra)

**Decision**: A new Rust struct `SearchEngine` in `src-tauri/src/search/mod.rs`. Public API:

```rust
struct SearchQuery {
    text: String,
    scopes: Vec<SearchScope>,        // vault, sessions, transcripts, files, plans
    case_sensitive: bool,
    titles_only: bool,
    max_results: usize,
}

enum SearchScope {
    Vault,
    SessionTranscripts,
    OpenSpec,
    WorkspaceFiles { workspace_id: String },
    All,
}

fn search(query: SearchQuery) -> Result<Vec<SearchHit>>;
```

Implementation: shell out to `rg` (ripgrep, already a system dep on most distros — if missing, we fall back to a pure-Rust walker; ripgrep speed is the optimization, not the dependency). One pass per scope, results merged by relevance (filename match > content match > path match).

#7 (Ask the vault) calls `search` with `scopes: [Vault]`, modal UI.

#I (`@@` picker) calls `search` with `scopes: [Vault], titles_only: true, max_results: 8`, inline picker.

**Why a parent infra and not just a vault function**
- Future Cmd+P over everything (sessions, plans, transcripts, files, vault) reuses the same engine without re-architecting.
- Each consumer picks its scope filter. Adding a scope is a new enum variant + one path-resolution function.

**Alternatives considered**
- *Pure-Rust walker with the `walkdir` + `grep-regex` crates*: viable but ~3-5x slower than ripgrep on vaults > 1000 notes. Kept as fallback only.
- *Persistent index (sqlite-vss / tantivy)*: deferred. v1 doesn't pay the index-maintenance cost; users don't have vaults so large that ripgrep is intolerable.

### 8. Deep-link integration (#12 + #M)

**Decision**: 
- **Outbound (#12)**: emit `obsidian://open?vault=<name>&file=<encoded path>` via `tauri-plugin-shell`'s `open` (already in deps). No new infra. The vault_name comes from F (defaults to last segment of `vault_root` if unset).
- **Inbound (#M)**: add `tauri-plugin-deep-link = "2"`. Register scheme `cluihud://` in `tauri.conf.json` under the new `plugins.deep-link` block. On Linux, the plugin writes `MimeType=x-scheme-handler/cluihud;` to the bundled `.desktop` file (Tauri generates this at build time). At runtime the plugin emits a Tauri event on incoming URL; we wire it to a router in `src/lib/deepLinkRouter.ts`.

Supported `cluihud://` actions (v1):
- `cluihud://session/new?cwd=<path>&agent=<id>&prompt=<encoded text>` — opens a new session in the workspace containing `cwd`, optionally with initial prompt.
- `cluihud://open-file?workspace=<id>&path=<rel path>&line=<n>` — opens the file in the right panel.
- `cluihud://open-workspace?path=<abs path>` — opens (or creates) the workspace.

Unknown actions → toast warning "Unknown cluihud:// action: …".

**Why `tauri-plugin-deep-link` and not a custom `.desktop` patch**
- The plugin owns the build-time `.desktop` MimeType injection, single-instance handling (focuses the existing window instead of spawning a new one on second URL invocation), and the Tauri event wiring. Custom patching would duplicate this and break on every Tauri version bump.

### 9. Quick capture (#8) reuse of `FloatingPanel`

**Decision**: New atom `quickCaptureOpenAtom`, new component `QuickCapturePanel.tsx` mounting `FloatingPanel` with `panelId = "quick-capture"`. Body is a single textarea + submit (Enter to save, Shift+Enter for newline, Esc to cancel). Geometry persisted via the existing `floating_panel_geometry` table — no schema change.

**Why a floating panel and not a modal**
- Capture-while-working is the whole point. A modal blocks the workspace and breaks the "I had a thought, jot it down, keep going" rhythm. Floating panel is non-blocking, draggable, and persists position across opens.

**Default channel format**: append a section to the configured `quick_capture` file:

```markdown
## <ISO timestamp>
<typed text>

#cluihud-inbox
```

Tag is configurable per workspace (default `#cluihud-inbox`).

### 10. Project bootstrap (#J)

**Decision**: After `create_workspace` resolves successfully on the frontend, if F is configured, show a one-time prompt "Create matching vault note?". Acceptance writes a template to `<vault_root>/Projects/<workspace_name>/index.md`. Includes an opt-in "Apply suggested layout" checkbox that, when checked, also writes the F channel paths to project-scoped defaults:

```
session_log_path  = <vault_root>/Projects/<name>/log.md
moc_path          = <vault_root>/Projects/<name>/MOCs/
templates_path    = (left unchanged — templates are typically vault-wide)
```

**Why opt-in for the layout**
- Mufdi's flow (and only Mufdi's flow as of today) is to keep `Projects/<name>/*` together. Other users have their own org schemes. Forcing the layout would be exactly the "prescribed convention" we reframed F to avoid. The easter egg lets Mufdi click once and have it set up; everyone else just gets the bootstrap note.

### 11. Templates (#L)

**Decision**: New `notify` watcher on `<templates_path>` (created when `templates_path` is set in F, replaced when the path changes, dropped when unset). On every change event, re-scan the folder for `template-*.md` files; for each, parse optional YAML frontmatter (`name:` overrides the filename, `description:` shows in palette tooltip) and inject a `ShortcutAction` into the registry under category `templates` (new category, sorts after `action`).

Activation writes the body (frontmatter stripped) to the active session's PTY via `write_to_session_pty`. Trailing `\r` so it submits as a prompt if the session is at the user input prompt; otherwise it just enters the text.

**Why a category and not a separate palette**
- One palette is the user's anchor. Splitting templates into a "second palette" hides them from the muscle-memory entry point.

**Frontmatter shape**

```yaml
---
name: "Refactor request"
description: "Open a TDD refactor on the selected file"
---
```

### 12. Modifications to `keyboard-shortcuts` and `command-palette`

**Decision**: Add three new entries to the shortcut registry in `src/stores/shortcuts.ts`:
- `obsidian-quick-capture` → `ctrl+alt+q`, opens quick capture floating panel.
- `obsidian-vault-search` → `ctrl+alt+v`, opens vault search modal.
- `obsidian-open-current` → `ctrl+shift+v`, opens the currently focused file/spec/plan in Obsidian via `obsidian://`.

All three are gated by `obsidianEnabledAtom` — if no vault, the handler shows a toast pointing at Settings → Obsidian Integration.

For the palette, extend the contract: `shortcutRegistryAtom` becomes a derived atom that merges (static literal list) ⊕ (dynamic sources). The templates feature is the first dynamic source. A future "Recent vault notes" entry could plug in the same way.

## Risks / Trade-offs

**[Risk] Background process never runs (sandboxed environments)** — On some hardened Linux distros, child processes from a desktop app may be sandboxed. Mitigation: on first attempted spawn, verify the child is actually running (PID check after 200 ms). If not, fall back to a synchronous flush on close (the original alternative A) and warn the user via Sileo toast that bg processing is disabled.

**[Risk] Marker file accumulation on storage-full filesystems** — A failed flush leaves the marker behind. Mitigation: stale-marker detection on every launch (`age > 10 min` → retry). Also: marker payload is small (~1 KB) and they live in `~/.config/cluihud/` which is rarely the bottleneck.

**[Risk] Wikilink regex false positives in source code** — A markdown file embedding `[[double-bracket]]` syntax that isn't a wikilink (e.g. citing a math paper). Mitigation: the regex disallows `[`/`]`/`#`/`^`/`|` inside the inner segment; legitimate citations don't usually contain those, and we exclude `code` / `inlineCode` AST nodes entirely. Documented as a known false positive in `obsidian-wikilink-rendering` capability spec.

**[Risk] Templates folder watcher fires on every save (autosave in Obsidian)** — Causes registry re-injection storms. Mitigation: 200 ms debounce on the watcher, same pattern the scratchpad watcher uses.

**[Risk] `cluihud://` scheme handler hijacks a user's existing handler** — Unlikely (the scheme is namespaced) but possible. Mitigation: the plugin's `register` is idempotent and writes to the bundle `.desktop` file only. Document in M2 release notes.

**[Risk] N1 backlink section bloats long-running vault notes** — A heavily-referenced note (e.g. an architecture note touched by 200 sessions) ends up with a giant backlinks section. Mitigation: cap the section to the most recent 50 entries; older entries roll into a collapsed `<!-- nergal-backlinks-archive --><details>` block.

**[Trade-off] Per-workspace + global TOML override** — More moving parts than a single global config. Accepted because users with multiple vaults are real (work + personal). The TOML override covers the single-vault default in one line.

**[Trade-off] Templates as palette entries vs. dedicated UI** — Some users might want a sidebar list of templates with previews. Accepted as a v2 if demand surfaces. v1 keeps the palette as the anchor.

**[Trade-off] Continuous log per workspace, MOC per session** — Two destinations to keep consistent. Accepted because the access patterns differ enough: a long workspace-level stream for chronological scanning, plus per-session snapshots for "what did I do in session X". Reverse backlinks (N1) point at the MOC (stable per-session anchor), not the continuous log (line ranges drift).

**[Trade-off] Search via ripgrep shell-out, not a Rust crate** — Adds a runtime dep on `rg`. Accepted because ripgrep speed at vault scale is meaningfully better than `walkdir + grep-regex`, and `rg` is present on every Linux dev workstation we expect. Fallback to pure-Rust walker covers the missing-binary case.

## Open Questions

- **Vault name default**: should `vault_name` default to the basename of `vault_root` if not set in F? Probably yes — fewer user-visible config fields. Closed: yes, derive from `vault_root` basename, expose as override in Settings.
- **Continuous log per-session header**: should `## Session "<name>"` use the `name` (user-friendly) or the `id` (stable)? Closed: use the `name`. The user reads the file; if they rename the session mid-flight, that's the new header for the next event. Old entries keep the previous name (no rewrites).
- **Workspace deletion**: when a workspace is deleted, do we also delete the per-workspace `obsidian_config` row? Closed: yes, via `ON DELETE CASCADE`. Vault files are NOT deleted (they belong to the user, not the workspace).
- **Recovery toast on next launch**: if 3+ stale markers exist, should we silently re-run or ask? Closed: silently re-run on launch (logged), surface a one-line toast "Caught up on N pending session snapshots." Errors during recovery → louder toast.

## Migration Notes

- **No data migration of existing sessions**. The continuous log only captures events from M3 forward. Past sessions are not back-filled (would require re-parsing every transcript, out of scope).
- **Settings UX**: users updating from a pre-M1 version see the new "Obsidian Integration" section in Settings, empty by default. No prompts, no banners.
- **CLI binary upgrade**: `pnpm tauri build` + `dpkg -i` is the canonical install (per CLAUDE.md). The bundled CLI gains the `post-session` subcommand automatically.
