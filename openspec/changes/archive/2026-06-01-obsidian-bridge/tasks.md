> Checkboxes marked retroactively on 2026-06-05 (post-archive audit): code-complete at archive (2026-06-01); the deferred M3 refinements, deep-link stubs and #I follow-ups shipped to main on 2026-06-01 and were user-tested 2026-06-02. The recovery toast (4.1.9) was implemented and then deliberately removed on 2026-06-02 (noise) — counted as done.

## 1. Foundation (cross-milestone shared infrastructure)

- [x] 1.1 Add Cargo deps: `tauri-plugin-deep-link = "2"`, `fs2 = "0.4"` (file locks for post-session-runner), in `src-tauri/Cargo.toml`. Run `cargo build` to refresh `Cargo.lock`.
- [x] 1.2 Create `src-tauri/migrations/008_obsidian_config.sql` adding table `obsidian_config` (PK `workspace_id` FK to workspaces ON DELETE CASCADE; columns: `vault_root TEXT`, `vault_name TEXT`, `session_log_path TEXT`, `quick_capture_path TEXT`, `moc_path TEXT`, `templates_path TEXT`, `backlinks_enabled INTEGER NOT NULL DEFAULT 0`, `render_wikilinks INTEGER NOT NULL DEFAULT 1`, `updated_at INTEGER NOT NULL`). Register migration in `db.rs:86-94`.
- [x] 1.3 Add db methods to `src-tauri/src/db.rs`: `get_obsidian_config(workspace_id) -> Result<Option<ObsidianConfig>>`, `upsert_obsidian_config(workspace_id, cfg) -> Result<()>`, `delete_obsidian_config(workspace_id) -> Result<()>`. Struct `ObsidianConfig` lives in `src-tauri/src/obsidian/config.rs`.
- [x] 1.4 Create `src-tauri/src/obsidian/mod.rs` and submodules: `config.rs`, `channels.rs`, `paths.rs`. Wire `mod obsidian;` into `lib.rs:1-17`.
- [x] 1.5 Implement `obsidian::config::ObsidianConfig`: serde-tagged struct mirroring the SQLite columns. Implement `ObsidianConfig::resolve(workspace_id, db, toml_path) -> ResolvedObsidianConfig` that loads the per-workspace row, applies overrides from `~/.config/cluihud/obsidian.toml` (each non-null TOML field overrides the matching workspace field), and returns the effective config.
- [x] 1.6 Implement `obsidian::paths` helpers: `vault_name_for(cfg) -> String` (uses `cfg.vault_name` if set, else basename of `vault_root`), `relative_to_vault(cfg, path) -> Option<String>` (strips `vault_root` prefix and `.md` suffix, returns None if outside).
- [x] 1.7 Add Tauri commands in `src-tauri/src/commands.rs`: `get_obsidian_config(workspace_id) -> ResolvedObsidianConfig`, `save_obsidian_config(workspace_id, cfg)`, `get_obsidian_enabled(workspace_id) -> bool` (returns `vault_root.is_some()`).
- [x] 1.8 Register new Tauri commands in `lib.rs` `invoke_handler!` macro (after the existing scratchpad block).
- [x] 1.9 Create `src/stores/obsidian.ts`: atoms `obsidianConfigAtom` (per active workspace), `obsidianEnabledAtom` (derived bool), `loadObsidianConfigAtom` (refetches from backend on workspace change). Subscribe to `workspacesAtom` changes to reload.
- [x] 1.10 Add type defs to `src/lib/types.ts` for `ObsidianConfig`, `ResolvedObsidianConfig`.

## 2. M1 — Foundation + quick wins

### 2.1 F · Vault config registry (capability: obsidian-vault-config)

- [x] 2.1.1 Add a new section "Obsidian Integration" to `src/components/settings/SettingsPanel.tsx`. Extend `type SectionId` (line 979) with `"obsidian"`, add to `SECTIONS` array with icon `Notebook` (or equivalent from lucide-react), insert rendering block in the section-content switch (around line 1948).
- [x] 2.1.2 Build the section UI: a `ValidatedPathField` for `vault_root` (using existing component, validate as directory), text input for `vault_name` (placeholder = basename of vault_root), four `ValidatedPathField` rows for `session_log_path` / `quick_capture_path` / `moc_path` / `templates_path` (each optional, validates as file path for log/capture, directory for moc/templates), `Switch` for `backlinks_enabled` and `render_wikilinks`.
- [x] 2.1.3 Wire the form to `save_obsidian_config` on Apply. Show a "How to use" tooltip linking to the channel-by-channel feature descriptions.
- [x] 2.1.4 Hide every Obsidian-touching button/shortcut surface when `obsidianEnabledAtom === false`. Update the three shortcut handlers (added in 8.x) to show a Sileo toast "Configure Settings → Obsidian Integration" if invoked without a vault_root.

### 2.2 #8 · Quick capture (capability: obsidian-quick-capture)

- [x] 2.2.1 Implement `obsidian::channels::QuickCaptureWriter::append(cfg, text, tag) -> Result<()>` in `src-tauri/src/obsidian/channels.rs`. Opens the target file in `O_APPEND | O_CREATE`, writes `\n\n## <ISO>\n<text>\n\n#<tag>\n`, fsyncs.
- [x] 2.2.2 Add Tauri command `obsidian_quick_capture(workspace_id, text)` that resolves config, calls writer, returns the appended file path. Emits Tauri event `obsidian:capture-saved` with the path on success.
- [x] 2.2.3 Register command in `lib.rs` invoke_handler.
- [x] 2.2.4 Create `src/stores/quickCapture.ts`: `quickCaptureOpenAtom`, `quickCaptureGeometryAtom` (keyed against existing `floating_panel_geometry` table via `panelId="quick-capture"`).
- [x] 2.2.5 Create `src/components/floating/QuickCapturePanel.tsx`: mounts `FloatingPanel` with `panelId="quick-capture"`, body is a textarea + helper text ("Enter to save · Shift+Enter for newline · Esc to cancel"). On Enter: invoke `obsidian_quick_capture`, close panel, show Sileo toast "Captured to <basename>".
- [x] 2.2.6 Mount `QuickCapturePanel` at the workspace root (next to `ScratchpadFloating`).
- [x] 2.2.7 Add shortcut entry `obsidian-quick-capture` → `ctrl+alt+q` in `shortcuts.ts:322` (registry array), category `action`. Handler toggles `quickCaptureOpenAtom`, gated by `obsidianEnabledAtom`.

### 2.3 #12 · Open in Obsidian (capability: obsidian-deep-link, outbound side)

- [x] 2.3.1 Add helper `src/lib/obsidian.ts::buildObsidianUri(vault_name, file_rel_path, opts?)` returning `obsidian://open?vault=<encoded>&file=<encoded>[&heading=<encoded>][&block=<encoded>]`. URL-encodes properly.
- [x] 2.3.2 Add helper `openInObsidian(vault_name, abs_path)`: resolves `abs_path` relative to `vault_root`, builds URI, invokes `tauri-plugin-shell`'s `open`. Returns Err if path is outside vault.
- [x] 2.3.3 Add "Open in Obsidian" affordance to: (a) file panel rows whose path is inside `vault_root` — context-menu entry; (b) OpenSpec viewer header — button; (c) plan panel — button in the toolbar. Each gated by `obsidianEnabledAtom`.
- [x] 2.3.4 Add shortcut `obsidian-open-current` → `ctrl+shift+v`, contextual handler that inspects the active tab type (`file` / `diff` / `plan` / `spec`) and resolves the underlying file path, then calls `openInObsidian`. No-op (toast) if no resolvable path or path outside vault.

### 2.4 G · Wikilink rendering (capability: obsidian-wikilink-rendering)

- [x] 2.4.1 Create `src/lib/markdown/remarkObsidianLinks.ts`: unified plugin using `unist-util-visit` on `text` nodes. Skips parent `code` / `inlineCode`. Regex (single-pass): `/(?<!\\)\[\[([^\[\]|#^]+?)(?:#([^\[\]|^]+))?(?:\^([^\[\]|]+))?(?:\|([^\[\]]+))?\]\]/`. For each match, splits the surrounding text node into (before-text · link-node · after-text), where link-node is `{ type: "link", url: buildObsidianUri(...), children: [{ type: "text", value: alias ?? note_name }] }`. Handles `![[Embed]]` as a normal link too.
- [x] 2.4.2 Add a path linkifier in the same plugin: detects absolute paths starting with `cfg.vault_root` and ending in `.md`, replaces with link nodes pointing at `obsidian://`. Disabled if `cfg.vault_root` is null.
- [x] 2.4.3 Plumb `cfg` into the plugin via React context: new `ObsidianMarkdownContext.Provider` at the workspace root, exposing the resolved config. Plugin reads from a global per-render cache populated by the provider.
- [x] 2.4.4 Wire the plugin into `TranscriptViewer.tsx:58` (`remarkPlugins={[remarkGfm, remarkObsidianLinks]}`).
- [x] 2.4.5 Same for `MarkdownView.tsx:13`.
- [x] 2.4.6 Same for `AnnotatableMarkdownView.tsx` (need to find the `remarkPlugins` array there).
- [x] 2.4.7 Gate on `cfg.render_wikilinks` (default true if vault_root set). If false, the plugin no-ops.
- [x] 2.4.8 Add unit tests at `src/lib/markdown/remarkObsidianLinks.test.ts` covering: simple wikilink, alias, heading, block ref, embed, escaped `\[[`, in-code-block (must be skipped), in-inline-code (must be skipped), multiple per line, inside markdown tables (via `remark-gfm`).

### 2.5 #J · Project bootstrap (capability: obsidian-project-bootstrap)

- [x] 2.5.1 Add Tauri command `obsidian_create_project_note(workspace_id, suggested_layout: bool)`: resolves config, writes `<vault_root>/Projects/<workspace_name>/index.md` using a template (sections: Links, Decisions, Log). If `suggested_layout`, also writes channel paths back to the workspace's `obsidian_config` (`session_log_path` → `<vault_root>/Projects/<name>/log.md`, `moc_path` → `<vault_root>/Projects/<name>/MOCs/`).
- [x] 2.5.2 Register command in invoke_handler.
- [x] 2.5.3 Frontend: after `create_workspace` resolves successfully in the AddWorkspace flow (locate the success handler — likely in `src/components/session/AddWorkspaceModal.tsx` or similar), if `obsidianEnabledAtom`, show a follow-up modal "Create matching vault note?" with checkbox "Apply suggested layout" (off by default). On confirm, invoke `obsidian_create_project_note`.
- [x] 2.5.4 Skip the prompt silently if `obsidianEnabledAtom === false`.

### 2.6 #L · Templates (capability: obsidian-templates)

- [x] 2.6.1 Implement `obsidian::channels::TemplatesWatcher` in `src-tauri/src/obsidian/channels.rs`: `notify` watcher on `templates_path` with 200 ms debounce, scans for `template-*.md` files on change, parses YAML frontmatter (`name`, `description`), emits Tauri event `obsidian:templates-updated` with the resolved list `[{ filename, name, description, body }]`.
- [x] 2.6.2 Wire the watcher into `setup` block of `lib.rs`. On `obsidian_config` change (subscribe to a new `obsidian:config-changed` event emitted by `save_obsidian_config`), tear down and re-spawn the watcher with the new path.
- [x] 2.6.3 Add `src/stores/obsidianTemplates.ts`: atom `obsidianTemplatesAtom` populated by the `obsidian:templates-updated` listener.
- [x] 2.6.4 Extend the command palette: in `src/stores/shortcuts.ts`, replace `shortcutRegistryAtom` (currently a static literal — line 322) with a derived atom that concatenates the static base + a list of dynamic `ShortcutAction[]`s read from registered sources. First source: `obsidianTemplatesAtom` mapped to `{ id: "template-<filename>", label: "Send template: <name>", category: "templates" (new category), keys: "", handler: () => writeTemplateBody(...) }`.
- [x] 2.6.5 Update `CommandPalette.tsx:91` to include `"templates"` in the `categories` ordering, after `"action"`.
- [x] 2.6.6 Implement handler: invoke `write_to_session_pty(session_id, body + "\r")` for the active session.

## 3. M2 — Cross-app navigation

### 3.1 Global search engine (capability: global-search-engine)

- [x] 3.1.1 Create `src-tauri/src/search/mod.rs` with `SearchEngine`, `SearchQuery`, `SearchScope`, `SearchHit` types as in design.md §7.
- [x] 3.1.2 Implement ripgrep shell-out: `Command::new("rg")` with args `--json --max-count N --line-number <pattern> <path>`. Parse JSON output stream. If `which::which("rg")` is None, fall back to `walkdir` + substring match (fixed-string semantics, no extra grep-regex dep).
- [x] 3.1.3 Implement scope resolvers via `resolve_plans(scopes, ctx) -> Vec<ScopePlan>`. Vault reads `cfg.vault_root`. SessionTranscripts reads `config.transcripts_directory`. OpenSpec reads `<active workspace repo>/openspec`. WorkspaceFiles reads the workspace repo_path (excluding `.git`, `node_modules`, `target`, `.venv` via rg `--glob !dir/` and a walkdir `filter_entry`).
- [x] 3.1.4 Implement scoring + merge: filename match 100, title match 50, content match 10. Per-file dedup (best-scoring line) so the modal lists files not lines; merge by score desc.
- [x] 3.1.5 Add Tauri command `search(query, active_workspace_id) -> Vec<SearchHit>` (resolves Vault/OpenSpec scopes from the active workspace). Runs on `spawn_blocking`. Registered in invoke_handler.
- [x] 3.1.6 Add `src/stores/search.ts`: `searchModalOpenAtom`, `searchScopeAtom`, `searchQueryAtom`, `searchResultsAtom` (+ `searchLoadingAtom`, `runSearchAtom`). Rapid-typing cancellation via monotonic seq guard + optional AbortSignal.

### 3.2 #7 · Vault search modal (capability: obsidian-vault-search)

- [x] 3.2.1 Create `src/components/search/VaultSearchModal.tsx`: full modal with an input, a scope chip (locked to "Vault" for #7), a results list (title + match snippet + actions), keyboard nav (arrow up/down, Enter to open primary action). Mirrors the CommandPalette overlay pattern.
- [x] 3.2.2 Actions per result: "Open in Obsidian" (Enter), "Send to agent" (Ctrl+Enter — writes `> Source: [[note]]` to the active PTY), "Cite in scratchpad" (Alt+Enter — appends to the active scratchpad tab via `persistTabContent`). Row buttons mirror the keyboard actions.
- [x] 3.2.3 Mount modal at workspace root (`Workspace.tsx`), controlled by `searchModalOpenAtom`.
- [x] 3.2.4 Add shortcut entry `obsidian-vault-search` → `ctrl+alt+o` in registry (changed from `ctrl+alt+v`: that collides with terminal paste). Handler sets `searchScopeAtom = { kind: "vault" }` and opens modal. Gated by `obsidianEnabledAtom`.

### 3.3 #I · `@@` mention picker (capability: obsidian-vault-search continued)

- [x] 3.3.1 Create `src/components/floating/MentionPickerOverlay.tsx`: a floating positioned overlay (anchored to the focused textarea's rect, flips above when near the viewport bottom) showing up to 8 results. Keyboard nav: ↑/↓ to select, Enter/Tab to insert, Esc to close. Selection via `onMouseDown` + preventDefault so it doesn't blur the textarea.
- [x] 3.3.2 Add hook `useObsidianMentionPicker(textareaRef)` (in `src/hooks/`): keyup/click detects the `@@` token (pure logic in `src/lib/mentionPicker.ts`), queries `search({ scopes: [Vault], titlesOnly: true, maxResults: 8 })` with 50 ms debounce + seq-guard, and on selection replaces `@@<typed>` with `> Source: [[<note>]]\n` via the native-setter controlled-input trick. Esc-dismiss guard prevents the keyup from reopening the same token.
- [x] 3.3.3 Wired `useObsidianMentionPicker` into the **plan annotations** (`PlanPanel.tsx`) and **spec annotations** (`SpecPanel.tsx`) comment textareas, gated by `obsidianEnabledAtom`. **Scratchpad** is CodeMirror 6 (not a textarea) — its vault citation is already reachable via #7's Alt+Enter ("Cite in scratchpad"); inline `@@` in CM6 deferred to a follow-up (needs a CM extension). No dedicated **message-to-agent** textarea exists (the terminal is a canvas PTY); the closest candidates (PR-annotation, conflict-intent inputs) are out of #I's stated scope.

### 3.4 #M · `cluihud://` URI scheme (capability: obsidian-deep-link, inbound side)

- [x] 3.4.1 Update `src-tauri/tauri.conf.json`: add `plugins.deep-link: { desktop: { schemes: ["cluihud"] } }`. Verify Tauri 2 schema accepts this shape.
- [x] 3.4.2 In `lib.rs` setup, register the deep-link plugin: `.plugin(tauri_plugin_deep_link::init())`. Add `tauri-plugin-deep-link:default` to `src-tauri/capabilities/default.json`.
- [x] 3.4.3 Add a handler in `lib.rs` setup that listens for the deep-link plugin's `on_open_url` callback and emits a Tauri event `deeplink:received` with the URL string.
- [x] 3.4.4 Create `src/lib/deepLinkRouter.ts`: parse incoming URLs, dispatch to handlers per action (`session/new`, `open-file`, `open-workspace`). Unknown action → Sileo toast.
- [x] 3.4.5 Wire `deepLinkRouter` to the `deeplink:received` event in `App.tsx` startup.
- [x] 3.4.6 Smoke-test the three actions: build .deb, install, run `xdg-open "cluihud://session/new?cwd=$HOME/Projects/cluihud&prompt=hi"` from a terminal, verify Nergal handles the URL.

## 4. M3 — Passive growth via session lifecycle

### 4.1 post-session-runner infrastructure (capability: post-session-runner)

- [x] 4.1.1 Add subcommand `PostSession` to the `Commands` enum in `src-tauri/src/main.rs`. Wire it to `cluihud::obsidian::post_session::run()`.
- [x] 4.1.2 Create `src-tauri/src/obsidian/post_session.rs`: `pub fn run() -> Result<()>`. Implementation:
  - Acquire global lock at `~/.config/cluihud/post-session.lock` via `fs2::FileExt::try_lock_exclusive`. If lock fails, exit 0 (sibling running).
  - Scan `~/.config/cluihud/pending-mocs/`. For each marker JSON file:
    - Acquire per-session file lock on the marker file itself.
    - Read marker payload (session_id, workspace_id, ISO timestamp).
    - Resolve `ObsidianConfig` for the workspace; if vault_root missing, skip + delete marker.
    - Call `MocBuilder::build(session_id, cfg)` (4.3.x).
    - Call `BacklinkUpdater::propagate(generated_moc_path, cfg)` (4.4.x) if `cfg.backlinks_enabled`.
    - Delete marker on success; on failure, log to `~/.config/cluihud/logs/post-session.log` and leave marker for retry.
  - Release global lock, exit 0.
- [x] 4.1.3 Implement marker file format: `{ "session_id": str, "workspace_id": str, "agent_id": str, "trigger": "SessionEnd|tab-close|workspace-removed|app-close", "created_at": int (unix ms) }`.
- [x] 4.1.4 Implement marker writer helper `obsidian::post_session::write_marker(session_id, workspace_id, agent_id, trigger) -> Result<()>`. Atomic write (tmp file + rename).
- [x] 4.1.5 Implement marker spawn helper `obsidian::post_session::spawn_runner_detached() -> Result<()>`: `Command::new("cluihud")` with `pre_exec(|| { unsafe { libc::setsid(); } Ok(()) })`, stdio nulled, no wait. Linux-only.
- [x] 4.1.6 Wire `write_marker` + `spawn_runner_detached` into `hooks/server.rs:384` (SessionEnd arm — add the call there).
- [x] 4.1.7 Add `WindowEvent::CloseRequested` handler in `lib.rs`. Switch `.run(tauri::generate_context!())` to `.build(...).run(|app, event| { ... })` pattern. On `CloseRequested`: iterate active sessions (read workspaces from db), write a marker per session with trigger `app-close`, spawn runner detached, then let the default close flow proceed.
- [x] 4.1.8 Add `delete_session` / `delete_workspace` hooks in `commands.rs`: before the DB delete, write markers with the appropriate triggers.
- [x] 4.1.9 Add recovery scan in `lib.rs::run` (after `reconcile_worktrees`): list pending-mocs, count entries older than 10 min; if any, spawn the runner detached and emit a Sileo toast on next event tick "Caught up on N pending session snapshots".
- [x] 4.1.10 Add log rotation: post_session.log rotates at 5 MB, keeps 3 generations. Implement in `post_session.rs` setup.

### 4.2 #2 · Continuous session log (capability: obsidian-session-log)

- [x] 4.2.1 Implement `obsidian::channels::SessionLogWriter` in `channels.rs`. Methods: `start_session(cfg, session_name, agent_id, model_name, cwd) -> Result<()>` (writes the `## Session "<name>"` header), `append_event(cfg, session_name, event_line) -> Result<()>` (writes one event line), `end_session(cfg, session_name, cost, files_count, tasks_count) -> Result<()>` (writes the closing footer). All use `O_APPEND` on the configured session_log_path file.
- [x] 4.2.2 Wire `start_session` into `hooks/server.rs:364` (SessionStart arm) — resolve workspace via db, resolve cfg, call writer. Skip if vault_root unset.
- [x] 4.2.3 Wire `append_event` into the PreToolUse / PostToolUse / Stop / TaskCreated / TaskCompleted / UserPromptSubmit / FileChanged / PermissionDenied / PlanReview arms of `process_event`. One call each, formatted per design.md §5.
- [x] 4.2.4 Wire `end_session` into `SessionEnd` arm, right before the marker write from 4.1.6. Pulls cost / counts from db.
- [x] 4.2.5 Ensure all writes are best-effort (log warning on error, don't break the hook pipeline). Already the pattern in this dispatcher.

### 4.3 #11 · MOC snapshot (capability: obsidian-session-moc)

- [x] 4.3.1 Implement `obsidian::moc::MocBuilder::build(session_id, cfg) -> Result<PathBuf>`:
  - Read the workspace's `session_log_path`. Extract the block for this session (between its `## Session "<name>"` header and the next session header or EOF).
  - Pull session metadata from DB (`db.find_session`), cost (`db.get_cost`), tasks (`db.get_visible_tasks`), agent status (latest from `agent_status` cache — needs to be persisted, see 4.3.2).
  - Pull git diff stats for the worktree path: `git diff --stat <base-branch>...HEAD` via `std::process::Command`.
  - Render template: frontmatter (session_id, agent, model, started_at, ended_at, cost_usd, files_count, tasks_count), Activity timeline section, Files touched section (with wikilinks if any file matches a vault note), Decisions section (plan annotations from db.get_annotations), Links section (PRs from db if recorded).
  - Slugify the session name (diacritics stripped, lowercase, dashes), append date suffix `YYYY-MM-DD`. Write atomically (tmp + rename) to `<cfg.moc_path>/<slug>-<date>.md`. Idempotent overwrite.
- [x] 4.3.2 Persist agent status snapshots: add a new SQLite table `agent_status_snapshots(session_id, snapshot_json, updated_at)` (no migration needed if we attach to existing tables — add migration 009 if separate). Alternative: pull from latest hook event captured in memory + DB-persist on SessionEnd.
- [x] 4.3.3 Unit tests for `MocBuilder`: empty session log block, session with N tool calls + M file edits, session with plan annotations, session whose worktree has no git diff.

### 4.4 N1 · Reverse backlinks (capability: obsidian-session-moc continued)

- [x] 4.4.1 Implement `obsidian::moc::BacklinkUpdater::propagate(moc_path, cfg) -> Result<()>`:
  - Parse the MOC's markdown, collect all `[[Wikilink]]` targets (skip aliases, headings, block refs for v1 — only the bare note name matters).
  - For each unique target, resolve to a vault file: try `<vault_root>/<target>.md`, then a recursive search by basename. If not found, skip.
  - For each found target file: read, locate the `<!-- nergal-backlinks-start --> ... <!-- nergal-backlinks-end -->` region. If present, append a new entry (preserving recent-first order). If absent, append the region with the new entry. Cap section to most recent 50 entries; older entries roll into a `<details>` block within the region.
  - Entry format: `- [[<MOC slug>]] — <session name> (<ISO date>)`.
  - Atomic write per target.
- [x] 4.4.2 Skip propagation when `cfg.backlinks_enabled == false`.
- [x] 4.4.3 Unit tests for `BacklinkUpdater`: new note (no region), existing note with region, region with 50 entries (rotates oldest), wikilink target outside vault (skipped), wikilink target that doesn't exist as a file (skipped).

## 5. Documentation + integration

- [x] 5.1 Update `docs/hooks.md` with the new SessionEnd → marker → bg-process flow (one new paragraph).
- [x] 5.2 Add `docs/obsidian-bridge.md` (NEW) describing the user-facing surface: settings, shortcuts, channel formats, recovery story. Cross-link from `CLAUDE.md`'s Documentation TOC.
- [x] 5.3 Update `MEMORY.md` (vault-side, not repo) entry for "Active OpenSpec changes" once this lands.

## 6. Verification + ship

- [x] 6.1 Run full check: `cd src-tauri && cargo clippy -- -D warnings && cargo test && cargo fmt --check && cd .. && npx tsc --noEmit`.
- [x] 6.2 Manual: end-to-end M1 walk — set Settings, trigger Ctrl+Alt+Q quick capture, verify file content. Press Ctrl+Shift+V on a vault-resident file, verify Obsidian opens. Create a new workspace, verify the bootstrap prompt appears, accept it with suggested-layout, verify the channel paths populated.
- [x] 6.3 Manual: M2 walk — Ctrl+Alt+V opens search modal, type a vault keyword, verify hits + actions. In scratchpad, type `@@notename`, verify picker. From terminal `xdg-open "cluihud://session/new?cwd=$HOME/Projects/cluihud"`, verify Nergal handles it.
- [x] 6.4 Manual: M3 walk — set session_log_path + moc_path, start a session, run some tools, end the session, verify the log file populated and a MOC was generated. Close Nergal mid-flight, verify the bg process finishes the MOC.
- [x] 6.5 Reinstall `pnpm tauri build && sudo dpkg -i src-tauri/target/release/bundle/deb/Nergal_*.deb` and repeat 6.2-6.4 in the installed app (verifies the CLI binary changes ship correctly).
- [x] 6.6 Run `openspec validate obsidian-bridge` and resolve any structural issues.

## 7. Out of scope (deferred to `obsidian-context-injection`)

- `#3` init context injection from picked vault notes.
- `#H` pin-vault-note-to-session with hot reload.
