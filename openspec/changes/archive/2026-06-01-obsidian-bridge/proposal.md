## Why

Today Nergal runs around a coding-agent CLI in a PTY and watches its hook stream, but the knowledge that accumulates inside each session — what was touched, why, what was decided, what context drove it — evaporates the moment the user moves on. Many of the user's projects already have a parallel record in an Obsidian vault (decisions, design notes, project MOCs, scratch reasoning). The two contexts never meet: pasting between them is a manual chore that nobody does consistently, so neither side gets the value of the other.

This change wires the two together as a **bridge** (not an integration that owns either side). Nergal stays a CLI wrapper; Obsidian stays the vault. The bridge is a small set of contracts: writes from Nergal to declared channels of the vault, reads from the vault when the user asks, and bidirectional URI navigation so a wikilink in a session log lands in Obsidian and a button in Obsidian opens a Nergal session. None of this is auto-magic — every channel is opt-in via a vault_root config, and every write goes to a path the user picked.

The change is shipped as three milestones (M1 foundation + quick wins, M2 cross-app navigation, M3 passive growth via session-end snapshots). Each milestone delivers user-visible value on its own; the change can be archived after M3 lands without leaving orphan tasks.

Two related ideas (`#3` init context injection, `#H` pin vault note to session) are explicitly out of scope. They depend on a primitive — "context injection at spawn that works for every agent adapter" — that the agent-agnostic refactor (archived 2026-05-04) did NOT model. Those features will land in a separate change `obsidian-context-injection` once that primitive exists. Splitting now keeps `obsidian-bridge` non-blocked.

## What Changes

### M1 — Foundation + quick wins

- **F (vault config registry)** — New Settings section "Obsidian Integration" that declares a `vault_root` plus a per-channel path map (session_log, quick_capture, moc, templates, backlinks_enabled). Each channel is independently opt-in; missing paths leave the corresponding feature invisible. Per-workspace storage in SQLite with an optional global override at `~/.config/cluihud/obsidian.toml`.
- **#8 (quick capture)** — Global shortcut opens a floating input that appends the typed text (with timestamp + tag) to the configured `quick_capture` channel. Reuses the `FloatingPanel` chrome already proven by the scratchpad.
- **#12 (open in Obsidian)** — Buttons in file panel, OpenSpec viewer, and plan annotations open the corresponding note in Obsidian via `obsidian://open?vault=…&file=…`. If the note does not exist, Obsidian's native create-modal handles it.
- **#J (project bootstrap)** — Right after `create_workspace` succeeds, the frontend offers "Create matching vault note?". If accepted, writes `<vault_root>/Projects/<name>/index.md` with a template. Includes an opt-in "Apply suggested layout" easter egg that populates the F channel paths to project-scoped defaults.
- **#L (template gallery)** — A configurable vault folder is watched; each `template-*.md` appears in the command palette as "Send [name]" which writes the body to the active session's PTY. Templater-style power for users who already maintain prompt libraries in the vault.
- **G (wikilink rendering)** — A new remark plugin enchufed into the existing `react-markdown` pipeline in `TranscriptViewer.tsx` and `MarkdownView.tsx` converts `[[Note]]`, `[[Note|Alias]]`, `[[Note#Heading]]`, `[[Note^block-id]]`, and `![[Embed]]` into clickable links pointing at `obsidian://`. Code blocks are excluded by the AST shape. Terminal pane explicitly out of scope (would violate "Nergal doesn't reescribe what the agent renders").

### M2 — Cross-app navigation

- **`global-search-engine` (new infra)** — Single ripgrep-backed search engine over a configurable corpus (sessions + transcripts + plans + files + vault). Each call passes a scope filter; the engine ranks results, the consumer decides which scope to surface. Imports the design idea that absorbed the backlog item "Búsqueda global en cluihud" — vault-only search becomes one filtered consumer of the same engine. Foundation for any future Cmd+P over everything.
- **#7 (Ask the vault)** — Dedicated shortcut opens a modal that searches the engine with scope hard-locked to vault. Results show note title + match snippet + actions (Open in Obsidian / Send to agent / Cite inline).
- **#I (`@@` mention picker)** — In scratchpad, plan annotations, and message-to-agent inputs, typing `@@` opens a fuzzy picker that searches the engine with scope=vault (titles only, sub-100ms). Selecting inserts `> Source: [[Note]]` inline as a cite block.
- **#M (`cluihud://` URI scheme)** — Registers the scheme via `tauri-plugin-deep-link` so a wikilink in the vault like `[Open](cluihud://session/new?cwd=~/Projects/x&prompt=…)` opens Nergal (or focuses the existing instance) and runs the action. Supported actions: `session/new`, `open-file`, `open-workspace`. Completes the bidirectional pair with #12.

### M3 — Passive growth via session lifecycle

- **`post-session-runner` (new infra)** — A new CLI subcommand `cluihud post-session` runs as a detached background process. The Tauri app drops marker files in `~/.config/cluihud/pending-mocs/<session-id>.json` whenever a session ends (`SessionEnd` hook, explicit tab close, workspace removal, or app close), then spawns the runner. The runner takes a global PID lock, drains all pending markers, generates MOCs and backlinks, exits. Decouples shutdown UX from the heavy work; survives app crashes via re-scan on next launch.
- **#2 (session log)** — Continuous append (in-process) to the `session_log` channel as hook events flow. Every tool call, file modification, task completion, plan annotation, and Stop event is appended with timestamp. Log file is always up-to-date on disk; no special trigger needed. Channel format: one file per workspace, sessions delimited by `## Session <name> — <ISO>` headers.
- **#11 (MOC snapshot)** — On every session-end trigger (see runner above), the bg process reads the continuous log + cost summary + agent status + git diff stats and writes/updates a per-session note `<moc_channel>/<session-name>.md` with structured frontmatter (agent, model, tasks, files, decisions, links to PRs). Template-only — no LLM call.
- **N1 (reverse backlinks)** — When the MOC is generated, the bg process scans the MOC for `[[wikilinks]]` to notes elsewhere in the vault, and updates a `## Referenced in Nergal sessions` section in each linked note. Idempotent (own section markers, never touches anything else). Opt-in via `backlinks_enabled` in F.

### Existing capabilities affected

- **`keyboard-shortcuts`** — three new bindings (`Ctrl+Alt+Q` quick capture, `Ctrl+Alt+V` vault search, `Ctrl+Shift+V` "open active file/note in Obsidian"). Registry entries only; no behavior change to the dispatcher.
- **`command-palette`** — palette becomes capable of consuming dynamic action sources at runtime (today the registry is a static literal). Templates (M1) and a future "Ask the vault" entry (M2) plug in via a new contract.

## Capabilities

### New Capabilities

- `obsidian-vault-config` — Channel registry shared by every Obsidian-touching feature. Owns the schema and the activation gate (no vault_root → integration invisible).
- `obsidian-quick-capture` — Floating capture panel that appends to a vault file with timestamp + tag.
- `obsidian-deep-link` — Bidirectional `obsidian://` and `cluihud://` URI navigation. `obsidian://` is used for outbound links from Nergal (#12); `cluihud://` is registered as a handled scheme for inbound launches from the vault (#M).
- `obsidian-wikilink-rendering` — Remark plugin that converts wikilinks in markdown surfaces to `obsidian://` links.
- `obsidian-project-bootstrap` — Post-`create_workspace` prompt to scaffold a matching vault note and (optionally) suggested channel layout.
- `obsidian-templates` — Template gallery: watch a vault folder, surface each template in the command palette, send body to active session PTY on activation.
- `obsidian-vault-search` — Vault-scoped consumers of the search engine: modal #7 and `@@` picker #I.
- `global-search-engine` — Search infrastructure shared by vault search (M2) and future global-scope consumers.
- `obsidian-session-log` — Continuous, in-process append of hook events to the `session_log` channel.
- `obsidian-session-moc` — Per-session MOC snapshot + reverse backlink propagation, generated by the post-session runner from the continuous log.
- `post-session-runner` — Background process infrastructure: marker files, detached spawn, global PID lock, crash recovery on next launch.

### Modified Capabilities

- `keyboard-shortcuts` — Adds three new registry entries; touches `shortcutRegistryAtom` only.
- `command-palette` — Extends the palette to consume dynamic action sources alongside the static registry.

## Impact

- **Backend** — New `src-tauri/src/obsidian/` module (config, channels, capture writer, MOC builder, backlink updater, deep-link handler, search engine). New CLI subcommand `cluihud post-session` in `main.rs`. New `notify` watchers for the templates folder. New Tauri commands. Existing `hooks/server.rs` extended (writes to session_log channel on each relevant event, drops MOC marker on `SessionEnd`). `lib.rs` extended with `WindowEvent::CloseRequested` handler to drop markers + spawn runner.
- **Frontend** — New `src/stores/obsidian.ts` (vault config, quick capture state, deep-link routing). New components: quick capture floating panel, vault search modal, `@@` picker overlay, settings section "Obsidian Integration", project bootstrap prompt. `TranscriptViewer.tsx` + `MarkdownView.tsx` + `AnnotatableMarkdownView.tsx` get the new remark plugin. `CommandPalette.tsx` extended to merge dynamic sources. New shortcuts entries in `shortcuts.ts`.
- **File system** — Writes inside the user-declared vault root only (channel paths). Marker files in `~/.config/cluihud/pending-mocs/`. PID lock at `~/.config/cluihud/post-session.lock`. Bg process log at `~/.config/cluihud/logs/post-session.log` (rotative).
- **SQLite schema** — One new migration `008_obsidian_config.sql` adding table `obsidian_config` (per-workspace channel mapping) and unique index on `workspace_id`.
- **Cargo deps** — `tauri-plugin-deep-link = "2"`, `fs2 = "0.4"` (file locks for the bg runner), `time = "0.3"` (ISO 8601 for marker timestamps) or reuse existing `chrono`-equivalent.
- **NPM deps** — `unist-util-visit` + `unified` types may already be transitive deps of `react-markdown`; if not, explicit add. No new top-level UI deps.
- **Out of scope** — `#3` init context injection and `#H` pinned vault notes (deferred to `obsidian-context-injection` change). Auto-bootstrap of any vault path (every channel is explicit). Semantic search (v1 is ripgrep-only). LLM-generated MOCs (template-only).
