# obsidian-deep-link Specification

## Purpose
TBD - created by archiving change obsidian-bridge. Update Purpose after archive.
## Requirements
### Requirement: Outbound `obsidian://` navigation
The system SHALL provide a helper `openInObsidian(vault_name, abs_path)` that builds a URI of the form `obsidian://open?vault=<url-encoded vault_name>&file=<url-encoded relative path>` and dispatches it via `tauri-plugin-shell`'s `open`. The helper SHALL resolve `abs_path` relative to the active workspace's `vault_root`; if the absolute path is outside the vault, the helper MUST return an error and SHALL NOT spawn anything.

The system SHALL expose this outbound navigation as: (a) a context-menu entry "Open in Obsidian" on file panel rows whose path resolves inside the vault; (b) a header button in the OpenSpec viewer when the spec note has a vault-resident counterpart; (c) a toolbar button in the plan panel; (d) a global shortcut `Ctrl+Shift+V` that opens the currently focused file/spec/plan.

#### Scenario: Open a vault-resident file

- **WHEN** the file panel shows a row at `/home/user/Obsidian/Projects/foo/notes.md`
- **AND** the active workspace has `vault_root = /home/user/Obsidian`
- **AND** the user clicks "Open in Obsidian" in the row's context menu
- **THEN** the system SHALL build `obsidian://open?vault=Obsidian&file=Projects%2Ffoo%2Fnotes` (no `.md` suffix)
- **AND** invoke `tauri-plugin-shell`'s `open` with that URI

#### Scenario: File outside vault

- **WHEN** the file panel row is at `/tmp/somefile.md` and the vault is `/home/user/Obsidian`
- **AND** the user invokes "Open in Obsidian"
- **THEN** the helper SHALL return an error
- **AND** a Sileo toast SHALL surface "File is outside the configured vault"

#### Scenario: Shortcut with no focused file

- **WHEN** the user presses `Ctrl+Shift+V` while the active tab is the terminal (no file context)
- **THEN** the handler SHALL no-op with a quiet toast "No file in focus to open in Obsidian"

### Requirement: Inbound `nergal://` scheme registration
The system SHALL register `nergal://` as a custom URI scheme via `tauri-plugin-deep-link` (version 2). Registration MUST be declared in `tauri.conf.json` under `plugins.deep-link.desktop.schemes` and MUST be effective on Linux via the generated `.desktop` file's `MimeType=x-scheme-handler/nergal;` entry.

When a URL matching the scheme is opened from any external application, the system MUST receive it via the plugin's `on_open_url` callback and emit a Tauri event `deeplink:received` to the frontend.

#### Scenario: External invocation focuses existing instance

- **WHEN** Nergal is already running
- **AND** the user (or another app) invokes `xdg-open "nergal://session/new?cwd=$HOME/Projects/foo&prompt=hi"`
- **THEN** the existing Nergal window SHALL gain focus (single-instance behavior provided by the plugin)
- **AND** the URL SHALL be delivered to the frontend via `deeplink:received`

#### Scenario: Cold-start invocation

- **WHEN** Nergal is NOT running
- **AND** the user invokes a `nergal://` URL
- **THEN** Nergal SHALL launch
- **AND** the URL SHALL be queued and dispatched to `deeplink:received` after frontend setup completes

### Requirement: Supported `nergal://` actions (v1)
The frontend router SHALL handle the following actions, dispatched on incoming `deeplink:received` events:

- `nergal://session/new?cwd=<abs path>&agent=<optional id>&prompt=<optional url-encoded text>` — opens (or creates) the workspace containing `cwd`, spawns a new session with the optional agent override, and writes the prompt to the PTY if present.
- `nergal://open-file?workspace=<workspace id>&path=<relative path>&line=<optional integer>` — switches to the workspace, opens the file in the right panel, scrolls to the line if provided.
- `nergal://open-workspace?path=<abs path>` — opens or creates the workspace at the given path.

Unknown actions SHALL produce a Sileo toast "Unknown nergal:// action: …" and SHALL NOT crash the router.

#### Scenario: New session action with prompt

- **WHEN** `nergal://session/new?cwd=$HOME/Projects/foo&prompt=Implement+auth` is received
- **AND** a workspace at `$HOME/Projects/foo` already exists
- **THEN** the system SHALL spawn a new session in that workspace
- **AND** the literal text `Implement auth` SHALL be written to the new session's PTY after spawn-ready

#### Scenario: Open-file action with line

- **WHEN** `nergal://open-file?workspace=abc123&path=src/foo.rs&line=42` is received
- **THEN** the system SHALL switch to workspace `abc123`
- **AND** open `src/foo.rs` in the right panel
- **AND** scroll the viewer to line 42

#### Scenario: Unknown action

- **WHEN** `nergal://teleport?destination=mars` is received
- **THEN** the router SHALL emit a Sileo toast "Unknown nergal:// action: teleport"
- **AND** SHALL NOT throw

