## ADDED Requirements

### Requirement: Obsidian quick capture shortcut
The shortcut registry in `src/stores/shortcuts.ts` SHALL include an entry with id `obsidian-quick-capture`, label "Quick Capture to Obsidian", keys `ctrl+alt+q`, category `action`. The handler MUST be gated on `obsidianEnabledAtom`; when invoked with no vault configured, the handler MUST show a Sileo toast pointing at Settings instead of executing the action.

#### Scenario: Shortcut entry registered

- **WHEN** the shortcut registry is rendered in the command palette
- **THEN** an entry with id `obsidian-quick-capture` SHALL appear under category `action`
- **AND** the keys badge SHALL show `Ctrl+Alt+Q`

#### Scenario: Gated handler with no vault

- **WHEN** `obsidianEnabledAtom` is false
- **AND** the user presses `Ctrl+Alt+Q`
- **THEN** the quick capture panel SHALL NOT open
- **AND** a Sileo toast SHALL surface "Configure Settings → Obsidian Integration"

### Requirement: Vault search shortcut
The shortcut registry SHALL include an entry with id `obsidian-vault-search`, label "Search the vault", keys `ctrl+alt+v`, category `action`. The handler MUST be gated on `obsidianEnabledAtom` with the same toast behavior as above.

#### Scenario: Shortcut entry registered

- **WHEN** the shortcut registry is rendered
- **THEN** the `obsidian-vault-search` entry SHALL appear under category `action`
- **AND** the keys badge SHALL show `Ctrl+Alt+V`

### Requirement: Open current file in Obsidian shortcut
The shortcut registry SHALL include an entry with id `obsidian-open-current`, label "Open in Obsidian (current file)", keys `ctrl+shift+v`, category `action`. The handler SHALL inspect the active tab to resolve a file path (file / diff / plan / spec types), then invoke `openInObsidian` if the path is inside the vault. The handler MUST be gated on `obsidianEnabledAtom`.

#### Scenario: Shortcut on a vault file

- **WHEN** the active tab is a file at `<vault_root>/Projects/foo.md`
- **AND** the user presses `Ctrl+Shift+V`
- **THEN** the system SHALL open that file in Obsidian via the URI scheme

#### Scenario: Shortcut on a non-vault file

- **WHEN** the active tab is a file outside the vault
- **AND** the user presses `Ctrl+Shift+V`
- **THEN** the handler SHALL no-op with a Sileo toast "File is outside the configured vault"

#### Scenario: Shortcut with no file context

- **WHEN** the active tab is the terminal (no file context)
- **AND** the user presses `Ctrl+Shift+V`
- **THEN** the handler SHALL no-op with a Sileo toast "No file in focus to open in Obsidian"

### Requirement: No collision with existing shortcuts
Before introducing the three new shortcuts, the system MUST verify that none of `ctrl+alt+q`, `ctrl+alt+v`, `ctrl+shift+v` is already claimed in the registry. The change author SHALL grep `src/stores/shortcuts.ts` for the three keys; any collision SHALL block the change until resolved.

#### Scenario: Verified clean as of pre-implementation grep

- **WHEN** the change is implemented
- **THEN** a grep over `src/stores/shortcuts.ts` for `ctrl+alt+q`, `ctrl+alt+v`, `ctrl+shift+v` SHALL return no pre-existing entries
- **AND** the three new entries SHALL be the only handlers for those combinations
