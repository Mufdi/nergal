# Implementation Plan — Per-Agent Theme Sync

## Order of operations

Bottom-up, leaving the frontend wiring for last so each piece is independently testable:

1. **Trait surface** (`mod.rs`): capability flag + `ThemePalette` + `apply_theme` default. Compiles in isolation, no adapter changes yet.
2. **Registry dispatch** (`registry.rs`): central helper `apply_theme_to_all`. Unit-testable with mock adapters.
3. **pi adapter**: full implementation. Highest payoff (hot-reload) and lowest risk (well-documented).
4. **opencode adapter**: spike first (live-switch yes/no), then implement.
5. **codex adapter**: thinnest impl (config.toml edit).
6. **Tauri command + frontend wiring**: connect all the dots. Manual walk per agent.
7. **Spec deltas**: write after implementation lands (post-implementation spec update per `openspec/config.yaml` apply rule).

## File-by-file plan

### `src-tauri/src/agents/mod.rs`

Existing capability enum is `bitflags::bitflags`. Add `const THEME_SYNC = 1 << 8;` (8 is the next free bit). Update `AgentCapability::to_serialized_strings` (or equivalent — locate by search) to emit `"THEME_SYNC"`.

Add struct:

```rust
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemePalette {
    pub id: String,
    pub is_dark: bool,
    pub surface: String,
    pub foreground: String,
    pub card: String,
    pub secondary: String,
    pub muted_foreground: String,
    pub border: String,
    pub accent: String,
}
```

Extend the `AgentAdapter` trait:

```rust
#[async_trait]
pub trait AgentAdapter: Send + Sync {
    // ... existing methods ...

    /// Apply a cluihud theme to the agent's native theme system.
    ///
    /// Default impl returns `NotSupported(THEME_SYNC)`. Adapters that opt
    /// in MUST set `THEME_SYNC` in their advertised capabilities.
    ///
    /// Best-effort by design: writes are atomic, hot-reload is preferred,
    /// next-spawn fallback acceptable. Errors logged at the runtime, never
    /// surfaced to the user.
    async fn apply_theme(&self, _palette: &ThemePalette) -> Result<(), AdapterError> {
        Err(AdapterError::NotSupported {
            capability: AgentCapability::THEME_SYNC,
        })
    }
}
```

### `src-tauri/src/agents/registry.rs`

```rust
pub async fn apply_theme_to_all(&self, palette: ThemePalette) {
    let adapters = self.adapters.read();
    for adapter in adapters.iter() {
        if !adapter.capabilities().flags.contains(AgentCapability::THEME_SYNC) {
            continue;
        }
        if let Err(e) = adapter.apply_theme(&palette).await {
            tracing::warn!(
                agent = %adapter.id().as_str(),
                error = %e,
                "apply_theme failed; theme sync is best-effort"
            );
        }
    }
}
```

### `src-tauri/src/commands.rs`

```rust
#[tauri::command]
pub async fn apply_theme_to_agents(
    state: tauri::State<'_, AppState>,
    palette: ThemePalette,
) -> Result<(), String> {
    state.registry.apply_theme_to_all(palette).await;
    Ok(())
}
```

Register in `lib.rs` `invoke_handler` macro alongside existing commands.

### `src-tauri/src/agents/pi/adapter.rs`

`Self::new` — add `THEME_SYNC`:

```rust
flags: AgentCapability::SESSION_RESUME
    | AgentCapability::ASK_USER_BLOCKING  // (preserve existing flags)
    | AgentCapability::THEME_SYNC,
```

New impl block:

```rust
async fn apply_theme(&self, palette: &ThemePalette) -> Result<(), AdapterError> {
    let home = dirs::home_dir().ok_or_else(|| {
        AdapterError::Transport(anyhow::anyhow!("HOME not set"))
    })?;
    let themes_dir = home.join(".pi/agent/themes");
    tokio::fs::create_dir_all(&themes_dir).await.map_err(...)?;

    let theme_json = build_pi_theme(palette);
    let target = themes_dir.join("cluihud-active.json");
    write_atomic(&target, serde_json::to_vec_pretty(&theme_json)?).await?;

    let settings_path = home.join(".pi/agent/settings.json");
    update_settings_theme_if_safe(&settings_path, "cluihud-active").await?;
    Ok(())
}
```

`build_pi_theme` maps palette to all 51 tokens. Derivation:

| pi token | Source |
|---|---|
| `accent`, `border`, `borderAccent`, `toolTitle`, `mdLink`, `syntaxKeyword`, `syntaxOperator` | `palette.accent` |
| `borderMuted`, `muted`, `mdQuote`, `mdQuoteBorder`, `mdHr`, `toolDiffContext`, `syntaxComment`, `syntaxPunctuation`, `thinkingOff` | `palette.muted_foreground` |
| `success`, `toolDiffAdded`, `syntaxString` | `#22c55e` (constant; cluihud has no green token) |
| `error`, `toolDiffRemoved` | `#ef4444` |
| `warning` | `#f59e0b` |
| `dim` | `palette.muted_foreground` |
| `text`, `toolOutput`, `userMessageText`, `customMessageText`, `mdCodeBlock` | `""` (terminal default — picks up our `--terminal-foreground`) |
| `thinkingText`, `mdLinkUrl`, `mdCodeBlockBorder` | `palette.muted_foreground` |
| `selectedBg`, `userMessageBg`, `customMessageBg`, `toolPendingBg` | `palette.card` |
| `toolSuccessBg` | shaded `palette.card` toward green (computed); fallback `palette.card` |
| `toolErrorBg` | shaded toward red; fallback `palette.card` |
| `customMessageLabel`, `syntaxFunction`, `syntaxType` | `palette.accent` |
| `mdHeading` | `palette.accent` |
| `mdCode`, `bashMode` | `palette.accent` |
| `mdListBullet` | `palette.accent` |
| `syntaxVariable`, `syntaxNumber` | `palette.foreground` |
| `thinkingMinimal`/`Low`/`Medium`/`High`/`Xhigh` | accent variants (use accent + opacity ladder) |

Constant non-cluihud colors documented above kept inline; if a future cluihud theme adds semantic tokens (success/error), swap to those.

`write_atomic`: write to `<path>.tmp` then `tokio::fs::rename`. Shared helper goes in `src-tauri/src/agents/mod.rs` near `ThemePalette`.

`update_settings_theme_if_safe`: read JSON (or `{}` if missing); if `obj["theme"]` is missing OR equals `"cluihud-active"`, set/keep; else leave alone and log at debug.

### `src-tauri/src/agents/opencode/adapter.rs`

`Self::new` — add `THEME_SYNC` to the existing flags union.

`apply_theme`:

```rust
async fn apply_theme(&self, palette: &ThemePalette) -> Result<(), AdapterError> {
    let home = dirs::home_dir().ok_or_else(...)?;
    let themes_dir = home.join(".config/opencode/themes");
    tokio::fs::create_dir_all(&themes_dir).await?;

    let theme_json = build_opencode_theme(palette);
    write_atomic(&themes_dir.join("cluihud-active.json"),
        serde_json::to_vec_pretty(&theme_json)?).await?;

    let tui_json = home.join(".config/opencode/tui.json");
    update_settings_theme_if_safe(&tui_json, "cluihud-active").await?;

    // Best-effort live switch via HTTP API (verified via spike).
    if LIVE_SWITCH_VERIFIED {
        for entry in self.session_ports.iter() {
            let port = *entry.value();
            let url = format!("http://127.0.0.1:{port}/tui/execute-command");
            let body = serde_json::json!({ "command": "theme cluihud-active" });
            let _ = reqwest::Client::new()
                .post(&url)
                .json(&body)
                .timeout(std::time::Duration::from_millis(1500))
                .send()
                .await;
        }
    }

    Ok(())
}
```

`build_opencode_theme` mirrors the schema seen in the opencode binary:

```json
{
  "$schema": "https://opencode.ai/theme.json",
  "defs": { "bg": "#...", "fg": "#...", "card": "#...", "secondary": "#...", "muted": "#...", "border": "#...", "accent": "#..." },
  "theme": {
    "background": { "dark": "bg", "light": "bg" },
    "text": { "dark": "fg", "light": "fg" }
    // ... token map ...
  }
}
```

Token list extracted from the opencode binary strings dump (see `handoff/opencode-token-list.md` to be created during the impl). Minimum viable set: `background`, `text`, `border`, `accent`, `primary`, `secondary`, `success`, `error`, `warning`, `info`.

### `src-tauri/src/agents/codex/adapter.rs`

`Self::new` — add `THEME_SYNC`.

`apply_theme`:

```rust
async fn apply_theme(&self, palette: &ThemePalette) -> Result<(), AdapterError> {
    let home = dirs::home_dir().ok_or_else(...)?;
    let path = home.join(".codex/config.toml");
    let theme_name = if palette.is_dark { "monochrome" } else { "monochrome" }; // TBD during spike — codex may only ship one syntax theme
    upsert_toml_key(&path, &["tui", "theme"], theme_name).await?;
    Ok(())
}
```

`upsert_toml_key`: parse existing TOML (use `toml_edit` crate to preserve formatting + comments), set the dotted key, write atomically. Add `toml_edit` to `Cargo.toml` if not already present.

**Limitation acknowledged**: codex's `tui.theme` only changes syntax highlighting and requires restart. The TUI canvas background does NOT change. Document this in the spec delta + CHANGELOG.

### `src/lib/themes.ts`

Add helper:

```ts
export function extractPaletteFromComputedStyle(): ThemePalette {
  const cs = getComputedStyle(document.documentElement);
  const surface = cs.getPropertyValue("--terminal-surface").trim();
  const foreground = cs.getPropertyValue("--terminal-foreground").trim();
  const card = cs.getPropertyValue("--card").trim();
  const secondary = cs.getPropertyValue("--secondary").trim();
  const mutedForeground = cs.getPropertyValue("--muted-foreground").trim();
  const border = cs.getPropertyValue("--border").trim();
  const accent = cs.getPropertyValue("--primary").trim();
  const id = document.documentElement.dataset.theme ?? "v1-dark";
  return {
    id,
    isDark: computeLuminance(surface) < 0.5,
    surface, foreground, card, secondary, mutedForeground, border, accent,
  };
}

function computeLuminance(hex: string): number {
  // standard relative luminance
}
```

### `src/App.tsx`

Replace the existing effect:

```tsx
useEffect(() => {
  applyTheme(themeMode, customThemes);
  const handle = requestAnimationFrame(() => {
    const palette = extractPaletteFromComputedStyle();
    debouncedApplyToAgents(palette);
  });
  return () => cancelAnimationFrame(handle);
}, [themeMode, customThemes]);
```

`debouncedApplyToAgents` lives outside the component (module-level `debounce(150, …)`), calls `invoke("apply_theme_to_agents", { palette })`, swallows errors.

### `src/lib/types.ts`

```ts
export interface ThemePalette {
  id: string;
  isDark: boolean;
  surface: string;
  foreground: string;
  card: string;
  secondary: string;
  mutedForeground: string;
  border: string;
  accent: string;
}
```

## Patterns to reuse from the codebase

- **Atomic writes**: `src-tauri/src/config.rs` already uses a write-temp-then-rename pattern. Lift to a shared helper at `agents/mod.rs::write_atomic`.
- **Settings file reconciliation**: `src-tauri/src/agents/claude_code/adapter.rs` reads `~/.claude/settings.json` for hook installation. Mirror that JSON merge logic.
- **HTTP requests to local agent servers**: the opencode SSE client at `src-tauri/src/agents/opencode/sse_client.rs` already uses `reqwest`. Reuse the same client/timeout policy.
- **DashMap iteration over session ports**: `OpenCodeAdapter::start_event_pump` shows the pattern; `apply_theme` follows the same shape.

## Edge cases

| Case | Handling |
|---|---|
| `~/.pi/agent/` missing | `create_dir_all` first; if HOME is unset, return `Transport` error (logged, not surfaced). |
| User has `~/.pi/agent/themes/cluihud-active.json` they hand-edited | We OVERWRITE. The file name is namespaced as ours; expectation is that users edit a different file. Documented. |
| settings.json has `"theme"` pointing at another theme | We write our JSON file but leave `theme` alone. Effectively a no-op for that session. Logged. |
| Theme switch fires while opencode HTTP server is still booting | Best-effort: timeout 1.5s, swallow error. Next switch will catch up. |
| Cluihud closes mid-write | Atomic rename means we either have the old file or the new file, never a torn write. |
| pi running with `--no-themes` flag | pi ignores theme files. Our write is harmless; next pi launch without the flag picks it up. |
| Codex `config.toml` has malformed TOML | `toml_edit` parse failure — log error, do not corrupt file. |
| User switches theme 10 times in 2 seconds | Frontend debounce (150ms trailing) collapses to a single backend call with the final palette. |

## Verification commands (recap)

```bash
cd src-tauri && cargo clippy -- -D warnings && cargo test && cargo fmt --check
cd .. && npx tsc --noEmit
pnpm tauri dev   # manual walk per agent
```

## Out of scope (future work)

- Light-mode codex theme mapping if/when codex CLI ships one. Today both branches collapse to `monochrome`.
- Sending OSC 11 to PTYs as a third pathway. Drop-in addition behind the same `apply_theme` method when upstream TUIs add runtime listeners.
- Per-cluihud-theme persistent files (`cluihud-v11.json` etc.). Single `cluihud-active.json` is sufficient and avoids clutter.
- Mapping custom cluihud themes (`custom-*`) — the helper reads computed CSS so they work automatically; no special path needed.
- pi/opencode "respect my user theme" mode toggle in cluihud settings. If the user wants to opt out, they set their `theme` key to something other than `cluihud-active` and we leave them alone.
