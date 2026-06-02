## ADDED Requirements

### Requirement: Vault search modal
The system SHALL provide a modal UI dedicated to vault-scoped search, opened by the global shortcut `Ctrl+Alt+V`. The modal MUST: (a) show a single search input with the scope chip locked to "Vault", (b) render results as a list of cards with title, one-line snippet, and an action row, (c) support keyboard navigation (↑/↓ to move selection, Enter to invoke the default action, Esc to close).

The modal SHALL be gated by `obsidianEnabledAtom`. When invoked without a vault, the shortcut handler SHALL surface a Sileo toast pointing at Settings instead of opening the modal.

#### Scenario: Open and run query

- **WHEN** the user presses `Ctrl+Alt+V` with a configured vault
- **THEN** the modal SHALL appear with the input focused and the scope chip showing "Vault"
- **WHEN** the user types "architecture"
- **THEN** the system SHALL invoke `search({ text: "architecture", scopes: [Vault] })` with a 50 ms debounce
- **AND** the result list SHALL update with the returned hits

#### Scenario: No vault configured

- **WHEN** `Ctrl+Alt+V` is pressed and `vault_root` is null
- **THEN** the modal SHALL NOT open
- **AND** a Sileo toast SHALL surface "Configure Settings → Obsidian Integration"

#### Scenario: Empty query state

- **WHEN** the modal is opened and the input is empty
- **THEN** the result list SHALL be empty with a placeholder "Type to search the vault…"

### Requirement: Result actions
Each hit in the modal SHALL expose three actions:

1. **Open in Obsidian** (default, bound to Enter) — invokes `openInObsidian` for the hit path.
2. **Send to agent** — writes a block to the active session's PTY of the form `> Source: [[<title>]]\n<snippet>\n`, then returns focus to the terminal.
3. **Cite in scratchpad** — opens the scratchpad floating panel (auto-creating a tab if needed) and inserts the same source block at the caret.

#### Scenario: Default action

- **WHEN** the user selects a hit and presses Enter (no modifier)
- **THEN** the modal SHALL close
- **AND** Obsidian SHALL open at the hit's path

#### Scenario: Send to agent

- **WHEN** the user clicks "Send to agent" on a hit
- **AND** an active session exists
- **THEN** the modal SHALL close
- **AND** the system SHALL write the source block to the active session's PTY

#### Scenario: Send to agent with no active session

- **WHEN** the user invokes "Send to agent" with no active session
- **THEN** the action SHALL no-op with a Sileo toast "Open a session first"

### Requirement: `@@` mention picker in text inputs
In every multi-line text input that the user uses to compose context (scratchpad editor, plan annotation comment input, message-to-agent overlay), typing the literal token `@@` SHALL open an inline floating picker anchored to the caret. The picker SHALL query the search engine with `scopes: [Vault], titles_only: true, max_results: 8, text: <chars typed after @@>`. The picker SHALL update on every keystroke with a 50 ms debounce.

Keyboard navigation: ↑/↓ moves the selection, Enter inserts the cite block and replaces `@@<typed>` with `> Source: [[<note title>]]\n`, Esc closes the picker without insertion. Clicking outside SHALL also close without insertion.

#### Scenario: Trigger picker in scratchpad

- **WHEN** the scratchpad editor has focus
- **AND** the user types `@@arc`
- **THEN** the picker SHALL appear anchored to the caret
- **AND** show up to 8 results whose titles contain "arc"

#### Scenario: Insertion replaces token

- **WHEN** the picker is open showing results for `@@arc`
- **AND** the user selects "architecture-notes" and presses Enter
- **THEN** the editor buffer SHALL replace `@@arc` with `> Source: [[architecture-notes]]\n`
- **AND** the caret SHALL be positioned on the line below the inserted block

#### Scenario: Esc cancels without insertion

- **WHEN** the picker is open
- **AND** the user presses Esc
- **THEN** the picker SHALL close
- **AND** the `@@` token SHALL remain in the buffer as typed

#### Scenario: No vault, no picker

- **WHEN** `obsidianEnabledAtom === false`
- **AND** the user types `@@anywhere`
- **THEN** the picker SHALL NOT appear (the trigger handler is not registered)
