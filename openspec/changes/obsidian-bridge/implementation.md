## Implementation map

This change touches both processes of the Tauri app (GUI + CLI binary), 9 new backend modules, ~12 new frontend components, 2 hot codepaths (`hooks/server.rs::process_event`, `lib.rs` Tauri builder), and 2 modified specs. The order of execution below respects the dependency graph: foundation → M1 → M2 → M3, with verification gates between milestones.

## Pre-flight (before any code)

1. **Confirm no shortcut collisions.** Grep `src/stores/shortcuts.ts` for `ctrl+alt+q`, `ctrl+alt+v`, `ctrl+shift+v`. Per current state (validated 2026-05-26) none are claimed. If any collision lands between change drafting and execution, pick replacements before starting.
2. **Confirm `react-markdown` + `remark-gfm` versions.** `package.json:57,59` ships `^10.1.0` and `^4.0.1`. The plugin uses `unist-util-visit`; verify it's already a transitive dep (`pnpm ls unist-util-visit`). If not, explicit `pnpm add unist-util-visit @types/mdast`.
3. **Confirm `tauri-plugin-deep-link` for Tauri 2** version on crates.io. Pin to the latest 2.x.
4. **Confirm `which::which("rg")` returns Ok on the dev machine.** This is the search engine's preferred backend; the pure-Rust fallback also works but is slower.

## Foundation (tasks 1.1–1.10)

### 1.1 Cargo deps

Edit `src-tauri/Cargo.toml` (after the existing `which = "6"` line around line 60). Add:

```toml
tauri-plugin-deep-link = "2"
fs2 = "0.4"  # global+per-marker file locks for post-session runner
```

Run `cargo build` (it WILL fail later, that's fine — we want `Cargo.lock` refreshed early).

### 1.2 SQLite migration

Create `src-tauri/migrations/008_obsidian_config.sql`:

```sql
CREATE TABLE IF NOT EXISTS obsidian_config (
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

Register in `src-tauri/src/db.rs:86-94`: add `include_str!("../migrations/008_obsidian_config.sql"),` to the `migrations` slice.

### 1.3–1.4 DB methods + module skeleton

In `src-tauri/src/db.rs`, add a new section "── Obsidian Config ──" near the scratchpad section (around line 595). Implement:

```rust
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ObsidianConfig {
    pub vault_root: Option<String>,
    pub vault_name: Option<String>,
    pub session_log_path: Option<String>,
    pub quick_capture_path: Option<String>,
    pub moc_path: Option<String>,
    pub templates_path: Option<String>,
    pub backlinks_enabled: bool,
    pub render_wikilinks: bool,
}

pub fn get_obsidian_config(&self, workspace_id: &str) -> Result<Option<ObsidianConfig>> { … }
pub fn upsert_obsidian_config(&self, workspace_id: &str, cfg: &ObsidianConfig) -> Result<()> { … }
pub fn delete_obsidian_config(&self, workspace_id: &str) -> Result<()> { … }
```

Create `src-tauri/src/obsidian/mod.rs`:

```rust
pub mod config;
pub mod channels;
pub mod paths;
pub mod moc;        // populated in M3
pub mod post_session; // populated in M3
```

Wire `mod obsidian;` into `lib.rs` (between `mod openspec;` and `mod plan_state;`).

### 1.5–1.6 ObsidianConfig resolver

`src-tauri/src/obsidian/config.rs` owns:
- `ObsidianConfig` struct (re-exported from db for convenience).
- `ResolvedObsidianConfig` struct (same fields, all-resolved-after-TOML).
- `ObsidianConfig::resolve(workspace_id, db, toml_path) -> ResolvedObsidianConfig`. TOML overrides applied field-by-field (any non-null TOML field overrides the SQLite field).

`src-tauri/src/obsidian/paths.rs` owns:
- `vault_name_for(cfg) -> String` (uses `cfg.vault_name` or basename of `vault_root`).
- `relative_to_vault(cfg, abs) -> Option<String>` (strips prefix + `.md`).
- `to_obsidian_uri(cfg, abs, heading, block) -> Option<String>`.

### 1.7–1.8 Tauri commands

In `src-tauri/src/commands.rs`, add at the end of the file:

```rust
#[tauri::command]
pub fn get_obsidian_config(
    db: State<'_, SharedDb>,
    workspace_id: String,
) -> Result<ResolvedObsidianConfig, String> { … }

#[tauri::command]
pub fn save_obsidian_config(
    app: AppHandle,
    db: State<'_, SharedDb>,
    workspace_id: String,
    cfg: ObsidianConfig,
) -> Result<(), String> {
    // upsert then emit obsidian:config-changed
    …
}

#[tauri::command]
pub fn obsidian_enabled(
    db: State<'_, SharedDb>,
    workspace_id: String,
) -> Result<bool, String> { … }
```

Register all three in `lib.rs` `invoke_handler!` after the existing scratchpad block.

### 1.9–1.10 Frontend store + types

`src/lib/types.ts` — add:

```ts
export interface ObsidianConfig {
  vault_root: string | null;
  vault_name: string | null;
  session_log_path: string | null;
  quick_capture_path: string | null;
  moc_path: string | null;
  templates_path: string | null;
  backlinks_enabled: boolean;
  render_wikilinks: boolean;
}

export interface ResolvedObsidianConfig extends ObsidianConfig {}
```

`src/stores/obsidian.ts` — new file:

```ts
import { atom } from "jotai";
import { invoke, listen } from "@/lib/tauri";
import { activeWorkspaceAtom } from "./workspace";

export const obsidianConfigAtom = atom<ResolvedObsidianConfig | null>(null);

export const obsidianEnabledAtom = atom(
  (get) => get(obsidianConfigAtom)?.vault_root != null
);

export const loadObsidianConfigAtom = atom(null, async (get, set) => {
  const ws = get(activeWorkspaceAtom);
  if (!ws) { set(obsidianConfigAtom, null); return; }
  const cfg = await invoke<ResolvedObsidianConfig>("get_obsidian_config", { workspaceId: ws.id });
  set(obsidianConfigAtom, cfg);
});

// Listen for obsidian:config-changed and refetch
export async function setupObsidianListeners(store) {
  return await listen("obsidian:config-changed", () => store.set(loadObsidianConfigAtom));
}
```

Wire `setupObsidianListeners` into the app bootstrap in `src/App.tsx` (or wherever existing listener setup happens — likely `useEffect` in `App.tsx`).

## M1 (tasks 2.1.x — 2.6.x)

### Order of execution

1. Settings section first (2.1.1–2.1.4) — enables the rest of M1 to be tested with a real config.
2. Quick capture (2.2.1–2.2.7) — simplest end-to-end test.
3. Wikilink rendering (2.4.1–2.4.8) — passive enhancement, no new shortcut.
4. Open in Obsidian (2.3.1–2.3.4) — uses 2.4 helpers.
5. Project bootstrap (2.5.1–2.5.4) — exercises 2.1 SettingsPanel via "Apply suggested layout".
6. Templates (2.6.1–2.6.6) — last because it touches `shortcutRegistryAtom` (used by both #L and 2.1 shortcut entries).

### Settings section integration point

`src/components/settings/SettingsPanel.tsx`:
- Line 979 (the `SectionId` type union) — extend with `"obsidian"`.
- Line 985-ish (the `SECTIONS` array literal) — insert before "about": `{ id: "obsidian", label: "Obsidian Integration", icon: NotebookText }` (NotebookText is already imported from `lucide-react` on line 22 — verify).
- Line 1948 (the section-content switch) — add `{activeSection === "obsidian" && <ObsidianSection />}`. Implement `<ObsidianSection />` in the same file (or extract to `src/components/settings/ObsidianSection.tsx` if it exceeds ~150 lines).

### Quick capture mount point

The scratchpad floating panel is mounted somewhere near the App root. Locate via `grep -rn "ScratchpadFloating\|scratchpadOpenAtom" src/` — it's likely in `App.tsx` or a near-root layout component. Add `<QuickCapturePanel />` next to it.

### Wikilink remark plugin

Create `src/lib/markdown/remarkObsidianLinks.ts`:

```ts
import { visit, SKIP } from "unist-util-visit";
import type { Plugin } from "unified";
import type { Root, Text } from "mdast";

const WIKILINK_RE = /(?<!\\)\[\[([^\[\]|#^]+?)(?:#([^\[\]|^]+))?(?:\^([^\[\]|]+))?(?:\|([^\[\]]+))?\]\]/g;

export const remarkObsidianLinks: Plugin<[{ vaultName: string; vaultRoot: string; enabled: boolean }], Root> =
  (opts) => (tree) => {
    if (!opts.enabled) return;
    visit(tree, "text", (node, index, parent) => {
      if (!parent || index == null) return;
      if (parent.type === "code" || parent.type === "inlineCode") return;
      // split node into [text, link, text, link, …, text] siblings
      …
    });
  };
```

Wire into `TranscriptViewer.tsx:58` (the assistant-only block):

```tsx
<ReactMarkdown
  remarkPlugins={[
    remarkGfm,
    [remarkObsidianLinks, { vaultName: cfg.vault_name, vaultRoot: cfg.vault_root, enabled: cfg.vault_root != null && cfg.render_wikilinks }],
  ]}
  …
```

Same for `MarkdownView.tsx:13` and `AnnotatableMarkdownView.tsx`. The `cfg` comes from a React context provider mounted at the app root that reads `obsidianConfigAtom`.

### Templates dynamic-source plumbing

`src/stores/shortcuts.ts` refactor — current state (line 322): `shortcutRegistryAtom = atom<ShortcutAction[]>([ …static literal… ])`.

After change:
```ts
export const staticShortcutRegistryAtom = atom<ShortcutAction[]>([ …same literal… ]);
const dynamicShortcutSources: Atom<ShortcutAction[]>[] = [];
export function registerDynamicShortcutSource(source: Atom<ShortcutAction[]>) {
  dynamicShortcutSources.push(source);
}
export const shortcutRegistryAtom = atom((get) => {
  const base = get(staticShortcutRegistryAtom);
  const dynamic = dynamicShortcutSources.flatMap((src) => get(src));
  return [...base, ...dynamic];
});
```

The templates store registers itself on module load via `registerDynamicShortcutSource(obsidianTemplatesShortcutsAtom)`.

### M1 verification gate

Before starting M2:
- `cargo clippy -- -D warnings && cargo test && cargo fmt --check`
- `npx tsc --noEmit`
- Manual smoke: configure Settings, trigger Ctrl+Alt+Q, verify capture file. Open a vault file from the file panel context menu. Create a workspace, accept the bootstrap prompt with "Apply suggested layout". Open the command palette, verify the templates category appears with the configured `template-*.md` files.

## M2 (tasks 3.1.x — 3.4.x)

### Order of execution

1. Search engine (3.1.1–3.1.6) — foundational; vault search and `@@` picker both consume it.
2. Vault search modal (3.2.1–3.2.4) — exercises the engine end-to-end.
3. `@@` picker (3.3.1–3.3.3) — reuses the engine, separate UI.
4. Deep-link (3.4.1–3.4.6) — independent of the search infra; can land in parallel.

### Search engine implementation skeleton

`src-tauri/src/search/mod.rs`:

```rust
use crate::obsidian::config::ResolvedObsidianConfig;

#[derive(Debug, serde::Serialize)]
pub struct SearchHit {
    pub path: String,
    pub title: String,
    pub snippet: String,
    pub line_number: Option<u32>,
    pub score: u32,
    pub scope: String, // serde-serialized SearchScope
}

#[derive(Debug, serde::Deserialize)]
pub struct SearchQuery {
    pub text: String,
    pub scopes: Vec<SearchScope>,
    #[serde(default)] pub case_sensitive: bool,
    #[serde(default)] pub titles_only: bool,
    pub max_results: Option<usize>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(tag = "type")]
pub enum SearchScope {
    Vault,
    SessionTranscripts,
    OpenSpec,
    WorkspaceFiles { workspace_id: String },
    All,
}

pub async fn search(query: SearchQuery, ctx: SearchContext) -> Result<Vec<SearchHit>> { … }
```

The `SearchContext` carries: `db: SharedDb`, `obsidian_cfg: ResolvedObsidianConfig`, `transcripts_dir: PathBuf`. Built from Tauri state in the command wrapper.

### Cancellation strategy

Frontend uses an `AbortController` per modal/picker session. Backend command takes `query_id: String`. The backend keeps a `DashMap<String, ChildHandle>`; new invocations with the same `query_id` kill the previous child. The frontend regenerates `query_id` per modal-open (UUID v4) and reuses it for keystrokes within that session.

### Deep-link plugin wiring

`src-tauri/tauri.conf.json` — add to the top-level after `plugins.updater`:

```json
"deep-link": {
  "desktop": {
    "schemes": ["cluihud"]
  }
}
```

`src-tauri/capabilities/default.json` — add to the `permissions` array: `"deep-link:default"`.

`lib.rs` setup — add `.plugin(tauri_plugin_deep_link::init())` after the existing plugins. Inside the `setup` block, register an `app.deep_link().on_open_url(|event| { … })` callback that emits `deeplink:received` to the frontend with `event.urls()`.

`src/lib/deepLinkRouter.ts` — parse and dispatch:

```ts
export function setupDeepLinkRouter(store) {
  return listen<{ urls: string[] }>("deeplink:received", ({ payload }) => {
    for (const raw of payload.urls) {
      const url = new URL(raw);
      switch (url.host) {
        case "session":
          if (url.pathname.startsWith("/new")) { … }
          break;
        case "open-file": { … }
        case "open-workspace": { … }
        default: showToast(`Unknown cluihud:// action: ${url.host}`);
      }
    }
  });
}
```

### M2 verification gate

Same as M1: lint, test, type check, plus:
- Vault search modal smoke (Ctrl+Alt+V → query → all 3 actions).
- `@@` picker in scratchpad + plan annotations.
- `xdg-open "cluihud://session/new?cwd=$HOME/Projects/cluihud"` from another terminal.

## M3 (tasks 4.1.x — 4.4.x)

### Order of execution

1. Post-session runner skeleton (4.1.1–4.1.10) — without consumers, just the lifecycle infrastructure.
2. Session log writer (4.2.1–4.2.5) — produces the input the runner reads.
3. MOC builder (4.3.1–4.3.3) — consumed by the runner.
4. Backlink updater (4.4.1–4.4.3) — consumed by the runner.

### Tauri builder pattern change

`lib.rs:539-541` currently:

```rust
.run(tauri::generate_context!())
.expect("error while running tauri application");
```

Change to the longer form needed for window event handling:

```rust
.build(tauri::generate_context!())
.expect("error while building tauri application")
.run(|app_handle, event| {
    if let tauri::RunEvent::WindowEvent { event: tauri::WindowEvent::CloseRequested { .. }, .. } = event {
        // Iterate active sessions, write markers, spawn runner.
        if let Some(db) = app_handle.try_state::<SharedDb>() {
            let _ = drop_markers_for_active_sessions(&db, app_handle);
            let _ = obsidian::post_session::spawn_runner_detached();
        }
    }
});
```

The default close behavior is preserved — `CloseRequested` does not consume the event by default in Tauri 2 (`api.prevent_close()` would; we don't call it).

### Session log writer wiring

`hooks/server.rs::process_event` — add at the top of the function, after the `cluihud_session_id` early-return:

```rust
let cfg_opt = cluihud_session_id
    .and_then(|csid| db.lock().ok().and_then(|g| g.find_session(csid).ok().flatten()))
    .and_then(|sess| db.lock().ok().and_then(|g| g.get_obsidian_config(&sess.workspace_id).ok().flatten()))
    .map(|cfg| obsidian::config::resolve_with_toml(cfg, …));
```

Then in each event arm, if `cfg_opt.session_log_path.is_some()`, call the writer. Best-effort — wrap in `if let Err(e) = … { tracing::warn!(…); }`.

### MOC + backlinks deferred lifecycle

The runner reads the existing session_log_path, finds the session's block (between its header and the next/EOF). The block is the canonical source of truth — the runner does NOT need access to live atoms.

Cost / tasks come from SQLite directly (`db.get_cost`, `db.get_visible_tasks`).

Agent status is the tricky one — it currently lives only in the `agentStatusMapAtom` (frontend), which the bg process can't read. Solution: extend the `Stop` arm in `hooks/server.rs` to also persist the latest known agent status to SQLite. Add migration 009 if needed, or attach a column to `sessions` (`last_agent_status_json TEXT`).

### M3 verification gate

- All lint/test/type checks.
- Manual: start a CC session, run 3-4 tools, end with Ctrl+D in the terminal (triggers SessionEnd). Verify the session_log gets the footer block and a MOC appears in `moc_path`.
- Manual: close Nergal with two active sessions, verify two MOCs appear after Nergal exits (check `ps aux | grep "cluihud post-session"` and the resulting files).
- Manual: crash Nergal mid-session (kill -9), restart, verify the runner picks up the stale marker on next launch and the toast surfaces.

## Cross-cutting concerns

### Error visibility

All `obsidian::*` writes use `tracing::warn!` on failure, never panic. The post-session runner is the only piece that logs to a dedicated rotating file (because GUI logs aren't accessible to a detached bg process).

### Concurrency safety

- Channel writes use `O_APPEND` — safe for concurrent sessions in the same workspace.
- Marker files are written via tmp + rename — atomic on POSIX.
- Backlink updater takes per-target file locks (`fs2::FileExt::try_lock_exclusive`) so two simultaneous runners (across recovery edge cases) can't corrupt a shared target note.
- The post-session runner takes a global lock first, then per-marker locks. Two MOCs for the same session never overlap.

### Test coverage targets

- Unit tests for `remarkObsidianLinks` (heaviest user-facing logic, 8 scenarios in the spec).
- Unit tests for `MocBuilder` (4 scenarios).
- Unit tests for `BacklinkUpdater` (5 scenarios).
- Integration test: `cluihud post-session` invocation against a fixture marker + temp vault.
- No new tests required for: settings UI (touched by existing settings test patterns), shortcuts (config-only), Tauri command wrappers (thin glue).

### Deployment notes

- Bumping the CLI binary requires `pnpm tauri build && sudo dpkg -i src-tauri/target/release/bundle/deb/Nergal_*.deb` (per CLAUDE.md). The bg-process feature is the first one that *requires* the bundled CLI to be updated — running the GUI from `pnpm tauri dev` while the installed `/usr/bin/cluihud` is stale will produce silent failures (the GUI calls `cluihud post-session` from PATH).
- The `.desktop` file is regenerated by Tauri at build time; the deep-link plugin's `linux: { schemes: [...] }` block ensures the new MimeType entry appears.

### Rollback plan

The change is structured so each milestone can be reverted independently:

- Revert M3 only → bg process disabled, no session-end side effects. The `obsidian_config` table remains. M1+M2 features still work.
- Revert M2 only → no vault search modal, no @@ picker, no inbound URI scheme. The outbound `obsidian://` from M1 still works.
- Revert M1 → all bridge features off (channel registry removed; vault_root has no effect anywhere).

Each milestone's modifications to existing files are concentrated:
- `shortcuts.ts` (additive — remove the new entries)
- `TranscriptViewer.tsx`, `MarkdownView.tsx`, `AnnotatableMarkdownView.tsx` (additive — remove the plugin from the array)
- `SettingsPanel.tsx` (additive — remove the section entry + content block)
- `hooks/server.rs` (additive — remove the `if cfg.session_log_path …` blocks)
- `lib.rs` (modify-in-place — close handler addition; revert is a code delete)
- `tauri.conf.json` (additive — remove `plugins.deep-link`)
- `capabilities/default.json` (additive — remove `deep-link:default`)
