# Design — Per-Agent Theme Sync

## Context

cluihud's terminal uses a canvas renderer (`src/components/terminal/terminalService.ts`) whose `paintRow` clears each row with `TERM_THEME.background` and only paints a per-cell `bg` if the cell's parsed bg differs from the theme background. CC's TUI mostly leaves `cell.bg = null` so theme changes propagate naturally through `paintAll`. pi, opencode, and codex all emit explicit ANSI `\x1b[48;2;R;G;Bm` runs that lock their canvas to their own internal palette.

The agent-agnostic refactor (archived 2026-05-04) gave us a clean trait surface in `src-tauri/src/agents/mod.rs`. This change extends it with a new capability rather than special-casing theming inside the runtime.

## Decision 1: Per-agent native theming, not renderer override

**Chosen**: Map cluihud's active theme to each agent's native theme format and apply it through their native channels.

**Alternatives considered**:

- **A. Renderer override** — detect the "dominant" alt-screen bg color in `paintRow` and treat it as transparent so `TERM_THEME.background` shows through. Universal across agents present and future, but fragile: legitimate intentional bg fills (popups, syntax bg highlights) would also be erased. High risk of regressing CC's already-working flow. Rejected.
- **B. OSC 11 dynamic color**: emit `\x1b]11;rgb:..\x1b\\` to each PTY on theme change. Standard protocol but modern Ink/Bubbletea/Ratatui TUIs don't listen for OSC 11 at runtime — they read once at startup if at all. Empirically near-zero coverage today. Rejected as primary, kept as future enhancement if upstream TUIs add support.
- **C. Hybrid (1 + 2 + per-agent)**: ship all three pathways. Rejected: over-engineered for the actual coverage gain; OSC 11 piece adds complexity for ~0% lift today.
- **D. Per-agent native theming** (chosen): leverage each TUI's documented theme channel. Cleanest fit with Nergal's "corre alrededor del agente, no lo reemplaza" philosophy in `CLAUDE.md`. Honest about per-agent fidelity gaps (codex limited, opencode partial). Easy to extend if a new agent ships.

**Trade-offs accepted**:
- Coverage is uneven (pi: live, opencode: best-effort live + next-spawn fallback, codex: next-spawn syntax-only).
- We become responsible for translating between cluihud's 13 themes and each agent's color schema. Mitigated by writing a single `cluihud-active` theme file per agent and overwriting it.

## Decision 2: New `THEME_SYNC` capability flag

The agent-adapter spec defines an explicit, finite capability bitflag set (`PLAN_REVIEW`, `ASK_USER_BLOCKING`, `TOOL_CALL_EVENTS`, `STRUCTURED_TRANSCRIPT`, `RAW_COST_PER_MESSAGE`, `TASK_LIST`, `SESSION_RESUME`, `ANNOTATIONS_INJECT`). The CLAUDE.md statement that adding flags is "non-breaking for adapters that don't claim them" tells us this is the right extension point.

**Chosen**: add `AgentCapability::THEME_SYNC` and a corresponding `apply_theme` trait method with default impl returning `Err(NotSupported(THEME_SYNC))`. Adapters opt in by overriding.

**Alternative considered**: a free function `crate::themes::apply_to_agent(agent_id, palette)` matching on `AgentId`. Rejected — couples the runtime to a static dispatch table that breaks the agent-agnostic-foundation contract (R0136). The trait method keeps adapter logic owned by the adapter module.

## Decision 3: `ThemePalette` shape

A minimal struct passed across the IPC boundary and to every adapter. Avoids per-agent shape negotiations — adapters extract what they need.

```rust
pub struct ThemePalette {
    /// Stable theme id, e.g. "v1-dark", "v11-tokyo-night", "custom-<id>".
    pub id: String,
    /// True when the theme's surface is dark (bg luminance < 0.5).
    pub is_dark: bool,
    /// Resolved hex colors (CSS `#rrggbb`).
    pub surface: String,         // terminal background
    pub foreground: String,      // terminal foreground
    pub card: String,            // raised panel
    pub secondary: String,       // muted surface
    pub muted_foreground: String,
    pub border: String,          // may be rgba; converted per-adapter
    pub accent: String,          // primary color
}
```

**Why hex strings, not a richer color type**: each agent's theme format normalizes differently (pi accepts 256-color ints + hex + var refs; opencode wants flat hex defs; codex only consumes a string name). String hex is the lowest-common-denominator and avoids a transport-level color type.

**Why include `id` and `is_dark`**: pi and opencode both want to know "is this a dark or light theme" for sensible defaults (e.g., choosing between built-in `dark` and `light` if the cluihud theme is unmapped or for codex's `tui.theme` fallback mapping).

## Decision 4: Frontend palette extraction from computed CSS

The frontend already binds `data-theme` on `<html>`, which drives CSS variables (`--terminal-surface`, etc.). Rather than re-implement the theme→palette mapping in two places, the frontend reads `getComputedStyle(document.documentElement)` after `applyTheme()` and packages the result into a `ThemePalette` for the Tauri command.

This keeps the source of truth in CSS (`src/styles/globals.css`) and avoids drift between `src/lib/themes.ts` and a Rust palette table.

**Trade-off**: requires the call to happen after the browser has committed the style change. A `requestAnimationFrame` ensures the read sees the new computed values.

## Decision 5: Idempotent file writes under a `cluihud-active` namespace

Every adapter writes a single namespaced file:

- pi: `~/.pi/agent/themes/cluihud-active.json`
- opencode: `~/.config/opencode/themes/cluihud-active.json`
- codex: `~/.codex/config.toml` (modifies `[tui]` table's `theme` key only)

For pi and opencode we also touch `settings.json` / `tui.json` ONCE to set `"theme": "cluihud-active"` if and only if that key isn't already pointing at us. This sidesteps the risk of clobbering a user-authored theme they happen to have selected; if `theme` is set to anything else, we still write the JSON file but leave the user's selection alone (their choice wins; cluihud's theme sync becomes a no-op for that session).

**Alternative considered**: write per-cluihud-theme files (`cluihud-v1-dark.json`, `cluihud-v11-tokyo-night.json`). Rejected — multiplies filesystem clutter, complicates the "active" pointer in settings, and offers no benefit since the user only sees the active theme.

## Decision 6: Best-effort live for opencode via HTTP API

opencode's TUI exposes an HTTP API on a port we already control (`OpenCodeAdapter::session_ports`). Endpoints discovered in the binary:

- `POST /tui/execute-command` body `{"command": "..."}`
- `POST /tui/open-themes` (opens picker — interactive, not useful here)

We try `POST /tui/execute-command {"command": "theme cluihud-active"}` first. If it succeeds (TUI applies the named theme live), great. If it fails (unknown command syntax, or opencode requires the picker for theme selection), we log a debug warning and rely on the config-file path picking up at next spawn.

**Why not require the live API to work**: opencode's binary docs don't confirm `/theme` accepts an argument. We document this as a spike-during-implementation: if `execute-command "theme NAME"` doesn't switch live, we fall back gracefully. Implementation.md captures the verification step.

## Decision 7: codex limitations are documented, not papered over

codex's `tui.theme` is documented as "Syntax-highlighting theme override using kebab-case theme names" with **restart required**. It does NOT change the TUI canvas background.

We still implement the codex `apply_theme` because:
1. It's the only theme key codex exposes — leaving codex with zero theme sync would be inconsistent.
2. The trait method MUST be implementable for opt-in capability semantics — we either declare `THEME_SYNC` and write something useful, or don't declare it.
3. Codex syntax highlighting CAN follow cluihud's dark/light variant via mapping (e.g., light themes → light syntax theme).

The implementation maps cluihud `is_dark` → codex `tui.theme = "monochrome"` or similar light/dark pair (TBD by the spike during implementation; if codex has no light theme name we leave the key unset for light cluihud themes). Limitation surfaced in CHANGELOG.

## Decision 8: No respawn on theme change

Respawning active sessions on theme change would lose conversation state, in-flight tool calls, and PTY scrollback. Not viable. The trade-off is that opencode/codex sessions running at theme-change time may not pick up the new theme until their next launch. Acceptable: users don't switch themes mid-conversation often, and the cluihud chrome (sidebar, status bar, top bar) DOES update live regardless.

## Open questions (resolved during implementation, not blocking)

1. Does `POST /tui/execute-command {"command": "theme NAME"}` actually switch opencode's theme live? Verify with `curl` against a running opencode TUI early in implementation; if not, drop the live branch and rely on next-spawn.
2. What's the exact opencode theme JSON schema version? Discovered in the binary as `https://opencode.ai/theme.json` — the file has `defs` + `theme.dark`/`theme.light` structure. Implementation will codify a minimal valid shape with our derived colors.
3. Does codex have multiple syntax theme names (just one "monochrome"?) — confirm by reading `~/.codex` examples or codex source. Affects the `is_dark` mapping.

## Risks & mitigations

| Risk | Mitigation |
|------|-----------|
| User has a custom pi/opencode theme they care about | `apply_theme` ONLY updates `settings.json` / `tui.json` if their `theme` key is unset OR already `cluihud-active`. Never clobber user choice. |
| Disk write storms when user clicks through themes quickly | Debounce the `apply_theme_to_agents` invoke on the frontend (~150ms trailing) — once user lands, write once. |
| Theme writes fail (perms, missing dir) | Log to backend log, never surface as user-facing error. Theme sync is a UX nicety, not load-bearing. |
| Codex theme key name changes upstream | Encapsulate the codex mapping in `agents/codex/adapter.rs`'s `apply_theme`; one file to update. |
| Adding `apply_theme` to the trait breaks existing test doubles | Default impl returns `NotSupported(THEME_SYNC)`, so existing mocks keep working without modification. |
