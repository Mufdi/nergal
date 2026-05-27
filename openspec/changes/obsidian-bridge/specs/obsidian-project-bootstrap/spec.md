## ADDED Requirements

### Requirement: Post-workspace-create prompt
The system SHALL, immediately after `create_workspace` resolves successfully, present a one-time modal prompt offering to create a matching vault note. The prompt MUST only appear when the resolved `obsidian_config.vault_root` for the new workspace is non-null. The prompt MUST be dismissible without side effects.

The prompt SHALL contain a checkbox "Apply suggested layout" (default unchecked) that, when checked, also populates per-workspace channel paths to project-scoped defaults.

#### Scenario: Workspace created with vault configured globally

- **WHEN** the user adds a new workspace
- **AND** `~/.config/cluihud/obsidian.toml` provides `vault_root = /home/user/Obsidian`
- **THEN** after `create_workspace` resolves, a modal SHALL appear titled "Create matching vault note?"
- **AND** the modal SHALL show the target path `/home/user/Obsidian/Projects/<workspace name>/index.md`
- **AND** the modal SHALL include the "Apply suggested layout" checkbox

#### Scenario: Workspace created with no vault

- **WHEN** the user adds a new workspace
- **AND** the resolved `vault_root` is null
- **THEN** no prompt SHALL appear
- **AND** the workspace SHALL be added silently as before

#### Scenario: User declines the prompt

- **WHEN** the user dismisses the prompt without confirming
- **THEN** no vault file SHALL be written
- **AND** no per-workspace config changes SHALL be applied

### Requirement: Index note template
On confirmation, the system SHALL write a markdown file at `<vault_root>/Projects/<workspace name>/index.md` (creating intermediate directories if needed). The file MUST contain:

- A `# <workspace name>` heading.
- A `## Links` section with a bullet pointing back at the workspace via `cluihud://open-workspace?path=<abs path>`.
- A `## Decisions` section (empty placeholder).
- A `## Log` section (empty placeholder).

The system SHALL NOT overwrite an existing `index.md` at that path; instead, the prompt SHALL offer an "Open existing" action and SHALL NOT write.

#### Scenario: Confirm writes a fresh template

- **WHEN** the user confirms the prompt for workspace "foo"
- **AND** `<vault_root>/Projects/foo/index.md` does not exist
- **THEN** the file SHALL be written with the template body
- **AND** intermediate directories SHALL be created as needed

#### Scenario: Existing index.md is not overwritten

- **WHEN** the user confirms the prompt
- **AND** `<vault_root>/Projects/foo/index.md` already exists
- **THEN** no write SHALL occur
- **AND** the prompt SHALL show an "Open existing in Obsidian" action that opens the existing file via `obsidian://`

### Requirement: Suggested layout easter egg
When the user checks "Apply suggested layout" and confirms the prompt, the system SHALL additionally write the following to the workspace's `obsidian_config`:

- `session_log_path` ← `<vault_root>/Projects/<name>/log.md`
- `moc_path` ← `<vault_root>/Projects/<name>/MOCs/`

The system SHALL leave `templates_path` and `quick_capture_path` unchanged (those are vault-wide concerns, not per-project).

#### Scenario: Suggested layout populates channels

- **WHEN** the user confirms with "Apply suggested layout" checked
- **THEN** the workspace's `obsidian_config.session_log_path` SHALL be set to `<vault_root>/Projects/<name>/log.md`
- **AND** `obsidian_config.moc_path` SHALL be set to `<vault_root>/Projects/<name>/MOCs/`
- **AND** `quick_capture_path` and `templates_path` SHALL retain their prior values (or remain null)

#### Scenario: Suggested layout does not clobber existing settings

- **WHEN** the user previously configured `session_log_path = <somewhere else>` for this workspace
- **AND** later they accept "Apply suggested layout" on a different workspace
- **THEN** the existing workspace's `session_log_path` SHALL NOT be modified (the easter egg only affects the new workspace's row)
