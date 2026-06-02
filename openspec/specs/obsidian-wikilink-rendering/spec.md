# obsidian-wikilink-rendering Specification

## Purpose
TBD - created by archiving change obsidian-bridge. Update Purpose after archive.
## Requirements
### Requirement: Wikilink rendering via remark plugin
The system SHALL provide a `remark` plugin `remarkObsidianLinks` that walks the markdown AST of every message rendered through `react-markdown` and replaces wikilink text matches with `link` AST nodes pointing at `obsidian://` URIs. The plugin MUST be plugged into the existing `remarkPlugins` arrays of `TranscriptViewer`, `MarkdownView`, and `AnnotatableMarkdownView`. The plugin MUST skip nodes whose parent is `code` or `inlineCode` so wikilinks inside fenced blocks or backticks remain literal.

The plugin SHALL recognize the following wikilink shapes via a single regex pass:

```
/(?<!\\)\[\[([^\[\]|#^]+?)(?:#([^\[\]|^]+))?(?:\^([^\[\]|]+))?(?:\|([^\[\]]+))?\]\]/
```

Captures map to: `(1)` note title, `(2)` heading (optional), `(3)` block id (optional), `(4)` alias (optional). Escaped wikilinks `\[[Note]]` MUST be left alone.

#### Scenario: Plain wikilink

- **WHEN** the markdown contains `See [[ProjectNotes]] for details`
- **AND** the resolved Obsidian config has `vault_name = "Vault"`
- **THEN** the rendered HTML SHALL contain `<a href="obsidian://open?vault=Vault&file=ProjectNotes">ProjectNotes</a>`

#### Scenario: Wikilink with alias

- **WHEN** the markdown contains `Refer to [[ProjectNotes|the spec]]`
- **THEN** the rendered link SHALL show "the spec" as the visible text
- **AND** the `href` SHALL still target `ProjectNotes`

#### Scenario: Wikilink with heading

- **WHEN** the markdown contains `Jump to [[ProjectNotes#Architecture]]`
- **THEN** the `href` SHALL be `obsidian://open?vault=Vault&file=ProjectNotes#Architecture`

#### Scenario: Wikilink with block reference

- **WHEN** the markdown contains `Anchor: [[ProjectNotes^abc123]]`
- **THEN** the `href` SHALL be `obsidian://open?vault=Vault&file=ProjectNotes#^abc123`

#### Scenario: Embed syntax treated as link

- **WHEN** the markdown contains `![[ProjectNotes]]`
- **THEN** the rendered output SHALL be a normal link (not an inline embed)
- **AND** the href SHALL target `ProjectNotes`

#### Scenario: Wikilink inside a code fence is left literal

- **WHEN** the markdown contains a fenced code block whose body includes `[[NotALink]]`
- **THEN** the rendered code block SHALL display `[[NotALink]]` literally with no `<a>` element

#### Scenario: Wikilink inside inline code is left literal

- **WHEN** the markdown contains the inline span `` `[[NotALink]]` ``
- **THEN** the rendered output SHALL show the literal characters inside a `<code>` element

#### Scenario: Multiple wikilinks per line

- **WHEN** the markdown contains `See [[NoteA]] and [[NoteB]] together`
- **THEN** the rendered HTML SHALL contain two separate `<a>` elements
- **AND** the literal text between them SHALL be preserved

#### Scenario: Wikilink inside a markdown table

- **WHEN** the markdown contains a `remark-gfm` table cell with `[[NoteA]]`
- **THEN** the wikilink SHALL be rendered as a link inside the table cell

### Requirement: Absolute path linkification
The plugin SHALL also recognize absolute filesystem paths whose canonical form starts with `vault_root` and ends with `.md`, replacing them with `obsidian://` links. Paths outside the vault SHALL NOT be touched.

#### Scenario: Vault-resident absolute path

- **WHEN** the markdown contains `Saved to /home/user/Obsidian/Projects/foo.md`
- **AND** the resolved config has `vault_root = /home/user/Obsidian`
- **THEN** the rendered link href SHALL be `obsidian://open?vault=Obsidian&file=Projects%2Ffoo`

#### Scenario: Path outside vault

- **WHEN** the markdown contains `Touched /tmp/scratch.md`
- **THEN** the path SHALL remain unlinked text

### Requirement: Toggle and gate
The plugin SHALL no-op when `render_wikilinks = false` in the resolved Obsidian config, OR when `vault_root` is null. The default for `render_wikilinks` SHALL be `true` whenever `vault_root` is set.

#### Scenario: Disabled by config

- **WHEN** the user sets `render_wikilinks = false` in Settings
- **AND** the markdown contains `[[NoteA]]`
- **THEN** the rendered output SHALL show the literal characters `[[NoteA]]` with no link

#### Scenario: No vault configured

- **WHEN** `vault_root` is null
- **AND** the markdown contains `[[NoteA]]`
- **THEN** the rendered output SHALL show the literal characters

### Requirement: Surfaces in scope
The plugin SHALL be active in: `TranscriptViewer` (assistant messages), `MarkdownView` (plan panel), `AnnotatableMarkdownView` (spec annotations), and any future MOC/log preview panel that uses the same `react-markdown` setup. The plugin SHALL apply to both agent-authored and user-authored message bodies. The terminal pane is explicitly out of scope.

#### Scenario: User-typed wikilink in transcript history

- **WHEN** the transcript shows a user message containing `[[ProjectNotes]]`
- **THEN** the rendered link SHALL be clickable, same as for agent messages

#### Scenario: Wikilink in terminal output is not linkified

- **WHEN** the agent's PTY output contains `[[ProjectNotes]]`
- **AND** that output is rendered into the wezterm-term canvas glyph atlas
- **THEN** the canvas SHALL display the literal characters
- **AND** no link affordance SHALL be available in the terminal pane

