# codex-adapter — Delta

## ADDED Requirements

### Requirement: codex adapter advertises and implements THEME_SYNC (limited)

The codex adapter SHALL declare `AgentCapability::THEME_SYNC` in its capabilities bitset and SHALL implement `apply_theme(palette)` such that:

1. `~/.codex/config.toml` SHALL be edited atomically to upsert `tui.theme = "<derived>"`, where `<derived>` is mapped from `palette.is_dark` (today: both branches collapse to `"monochrome"` pending a future codex CLI light-theme name).
2. Other TOML keys (`mcp_servers`, etc.) SHALL be preserved with original formatting and comments where possible (use a TOML-edit library, not naive string replacement).
3. The file SHALL be created if missing, with a single `[tui]` table containing `theme = "<derived>"`.

The spec acknowledges a **known limitation**: codex's `tui.theme` only affects syntax-highlighting colors and requires a codex restart to take effect. The TUI canvas background does NOT change. This requirement ships to keep parity in the capability surface and to be ready when codex CLI widens its theme keys.

#### Scenario: Apply theme writes tui.theme into config.toml

- **WHEN** `apply_theme(palette)` is called
- **THEN** `~/.codex/config.toml` SHALL contain `theme = "<derived>"` under the `[tui]` table
- **AND** if `[tui]` did not exist, it SHALL be created
- **AND** the write SHALL be atomic

#### Scenario: Other config keys preserved

- **WHEN** `~/.codex/config.toml` contains an `[mcp_servers.foo]` table before `apply_theme` is called
- **THEN** the `[mcp_servers.foo]` table SHALL remain intact after the write
- **AND** comments and whitespace in unrelated lines SHOULD be preserved

#### Scenario: Missing config.toml is created

- **WHEN** `~/.codex/config.toml` does not exist
- **THEN** `apply_theme` SHALL create it with the minimal content `[tui]\ntheme = "<derived>"\n`

#### Scenario: Live update is NOT attempted

- **WHEN** `apply_theme` runs against active codex sessions
- **THEN** no IPC SHALL be performed to running codex processes
- **AND** the theme change applies only to subsequently spawned codex sessions
- **AND** even for those, only syntax highlighting reflects the change — the TUI background remains controlled by codex's internal defaults
