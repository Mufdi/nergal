# obsidian-vault-config Specification

## Purpose
TBD - created by archiving change obsidian-bridge. Update Purpose after archive.
## Requirements
### Requirement: Vault root activation gate
The system SHALL treat `vault_root` as the master switch for every Obsidian-touching feature. When `vault_root` is null for the active workspace (after applying the optional global TOML override), the system SHALL NOT render any Obsidian-related UI affordances, MUST NOT register any Obsidian shortcut handlers as active, and MUST NOT load any Obsidian-touching backend writers.

#### Scenario: Workspace with no vault configured

- **WHEN** the user opens a workspace whose `obsidian_config` row has `vault_root = NULL` and no global TOML overrides it
- **THEN** the "Open in Obsidian" buttons SHALL NOT appear in file panel, OpenSpec viewer, or plan panel
- **AND** the keyboard shortcuts `ctrl+alt+q`, `ctrl+alt+v`, `ctrl+shift+v` SHALL surface a Sileo toast "Configure Settings → Obsidian Integration" instead of triggering the action
- **AND** the wikilink remark plugin SHALL no-op (no AST rewrites)

#### Scenario: Workspace with vault_root set

- **WHEN** the user opens a workspace whose resolved config has a non-null `vault_root`
- **THEN** every Obsidian feature whose specific channel is also configured SHALL be active
- **AND** features whose channel paths are still null (e.g. `quick_capture_path = null`) SHALL show their UI affordance as disabled with a tooltip explaining which setting is missing

### Requirement: Per-workspace channel registry with global override
The system SHALL store the Obsidian configuration per workspace in a SQLite table `obsidian_config` keyed by `workspace_id` (foreign key to `workspaces.id`, ON DELETE CASCADE). The system SHALL also support an optional TOML file at `~/.config/nergal/obsidian.toml` whose fields override the matching SQLite columns when present.

The columns SHALL be: `vault_root TEXT`, `vault_name TEXT`, `session_log_path TEXT`, `quick_capture_path TEXT`, `moc_path TEXT`, `templates_path TEXT`, `backlinks_enabled INTEGER NOT NULL DEFAULT 0`, `render_wikilinks INTEGER NOT NULL DEFAULT 1`, `updated_at INTEGER NOT NULL`.

#### Scenario: Per-workspace configuration

- **WHEN** the user configures Settings → Obsidian Integration while workspace A is active
- **THEN** the settings SHALL be saved against workspace A's `obsidian_config` row
- **AND** switching to workspace B SHALL show B's own configuration (which may be empty or different)

#### Scenario: Global TOML override

- **WHEN** `~/.config/nergal/obsidian.toml` contains `vault_root = "/home/user/Obsidian"` and a workspace's SQLite row has `vault_root = NULL`
- **THEN** the resolved config for that workspace SHALL use `/home/user/Obsidian` as the vault root
- **WHEN** the same workspace's SQLite row sets `vault_root = "/home/user/Other"` and the TOML still has `/home/user/Obsidian`
- **THEN** the TOML value SHALL win (override semantics: any non-null TOML field beats the workspace field)

#### Scenario: Workspace deletion cascades

- **WHEN** a workspace is deleted from the sidebar
- **THEN** its `obsidian_config` row SHALL be deleted automatically via the foreign-key cascade
- **AND** the user's vault files SHALL NOT be touched

### Requirement: Settings section UX
The Settings dialog SHALL expose an "Obsidian Integration" section between "Scratchpad" and "About" in the sidebar of sections. The section MUST contain: one validated directory field for `vault_root`, one optional text field for `vault_name` (placeholder shows the basename of `vault_root`), four optional path fields for each channel, and two toggles (`backlinks_enabled`, `render_wikilinks`). Each field MUST have a short tooltip explaining which feature it enables.

#### Scenario: Open Settings → Obsidian Integration

- **WHEN** the user opens Settings and navigates to the "Obsidian Integration" section
- **THEN** the section SHALL display all eight fields in the order above
- **AND** path fields whose target directory does not exist SHALL show a warning icon but SHALL NOT block saving

#### Scenario: Save with vault_root set, other paths empty

- **WHEN** the user enters only `vault_root` and clicks Apply
- **THEN** the configuration SHALL be saved
- **AND** features requiring specific channels (e.g. quick capture) SHALL remain disabled with explanatory tooltips until those paths are also set

### Requirement: Activation event for downstream consumers
The system SHALL emit a Tauri event `obsidian:config-changed` after every successful save of `obsidian_config`, carrying the updated `ResolvedObsidianConfig` for the affected workspace.

#### Scenario: Settings save triggers downstream listeners

- **WHEN** the user saves changes in the Obsidian Integration section
- **THEN** `obsidian:config-changed` SHALL be emitted with the resolved post-override configuration
- **AND** the templates watcher SHALL tear down + re-spawn against the new `templates_path` if it changed
- **AND** the markdown remark plugin SHALL refresh its provider context so future renders pick up the new `vault_root` / `vault_name` / `render_wikilinks` toggle

