# agent-adapter — Delta

## ADDED Requirements

### Requirement: AgentCapability set includes THEME_SYNC

`AgentCapability` SHALL include a `THEME_SYNC` flag. Adapters that advertise `THEME_SYNC` MUST implement `apply_theme`. Adapters that do NOT advertise it SHALL inherit the default `apply_theme` impl returning `Err(NotSupported(THEME_SYNC))`.

#### Scenario: THEME_SYNC serializes on the wire

- **WHEN** an adapter with `THEME_SYNC` in its capabilities is queried by the frontend
- **THEN** the capability array SHALL include the string `"THEME_SYNC"`
- **AND** the frontend SHALL be able to gate UI affordances (e.g., a "theme sync" status indicator) on its presence

### Requirement: AgentAdapter exposes apply_theme

`AgentAdapter` SHALL expose:

```rust
async fn apply_theme(&self, palette: &ThemePalette) -> Result<(), AdapterError>;
```

The default implementation SHALL return `Err(AdapterError::NotSupported { capability: AgentCapability::THEME_SYNC })`. Adapters that advertise `THEME_SYNC` MUST override the default with an implementation that best-effort applies the palette to the agent's native theme system.

`apply_theme` is best-effort by contract: implementers MUST NOT propagate errors that would surface as user-facing notifications. Errors SHALL be logged by the registry dispatcher (see "Registry dispatches apply_theme to capable adapters" below).

#### Scenario: Default apply_theme returns NotSupported

- **WHEN** an adapter that does not declare `THEME_SYNC` has `apply_theme(palette)` called
- **THEN** the call SHALL return `Err(AdapterError::NotSupported { capability: AgentCapability::THEME_SYNC })`
- **AND** no filesystem writes SHALL be performed

#### Scenario: Adapter advertising THEME_SYNC overrides apply_theme

- **WHEN** an adapter declares `AgentCapability::THEME_SYNC` in `capabilities().flags`
- **THEN** the adapter MUST override `apply_theme` with an implementation that performs at least one of: write a theme file the agent reads at startup, write a hot-reloaded theme file the agent picks up live, or invoke a live theme-switch API exposed by the agent

### Requirement: ThemePalette struct passed across the IPC boundary

`ThemePalette` SHALL be a serializable struct defined in `src-tauri/src/agents/mod.rs` with the following fields:

- `id: String` — stable theme id (e.g. `"v1-dark"`, `"v11-tokyo-night"`, `"custom-<id>"`).
- `is_dark: bool` — true when surface luminance < 0.5.
- `surface: String` — terminal background as `#rrggbb`.
- `foreground: String` — terminal foreground as `#rrggbb`.
- `card: String` — raised panel color.
- `secondary: String` — muted surface.
- `muted_foreground: String` — secondary text.
- `border: String` — border color (may be rgba; adapters convert as needed).
- `accent: String` — primary accent color.

The struct SHALL serialize with `camelCase` field names for the TS boundary.

#### Scenario: ThemePalette round-trips through serde

- **WHEN** a `ThemePalette` is serialized to JSON and deserialized back
- **THEN** every field SHALL be preserved
- **AND** the JSON SHALL use `camelCase` keys (`isDark`, `mutedForeground`)

### Requirement: Registry dispatches apply_theme to capable adapters

`AgentRegistry` SHALL expose `async fn apply_theme_to_all(&self, palette: ThemePalette)` that iterates registered adapters, filters those whose `capabilities().flags.contains(THEME_SYNC)`, and invokes `apply_theme(&palette)` on each. Per-adapter errors SHALL be logged at `warn!` and SHALL NOT propagate.

A Tauri command `apply_theme_to_agents(palette: ThemePalette)` SHALL forward to this method. The frontend `applyTheme` flow SHALL invoke this command after the DOM `data-theme` mutation commits (next animation frame), with a 150ms trailing debounce.

#### Scenario: Non-capable adapter skipped

- **WHEN** `apply_theme_to_all` is called with one adapter declaring `THEME_SYNC` and one not declaring it
- **THEN** only the capable adapter's `apply_theme` SHALL be invoked
- **AND** the call to the non-capable adapter SHALL NOT happen

#### Scenario: Adapter failure does not block other adapters

- **WHEN** one adapter's `apply_theme` returns `Err(...)`
- **THEN** the registry SHALL log the error at `warn!` with the adapter id
- **AND** SHALL continue invoking `apply_theme` on the remaining capable adapters
- **AND** the top-level Tauri command SHALL return `Ok(())` regardless
