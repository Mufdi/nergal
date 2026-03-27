## Purpose

Display a project directory tree with lazy-loaded expansion and open files in a CodeMirror 6 editor tab with syntax highlighting.

## Requirements

### Requirement: Project tree in right sidebar
The system SHALL display the project directory tree in the right sidebar when the file browser panel is active. The tree SHALL support lazy-loaded directory expansion and file type icons.

#### Scenario: File browser shows project tree
- **WHEN** user opens the file browser panel
- **THEN** the right sidebar SHALL display the project directory tree rooted at the workspace path

#### Scenario: Lazy directory expansion
- **WHEN** user clicks a collapsed directory in the tree
- **THEN** the directory SHALL expand, loading its children on demand

#### Scenario: File type icons
- **WHEN** the tree displays files
- **THEN** each file SHALL show an icon based on its extension (e.g., TypeScript, Rust, JSON, Markdown)

#### Scenario: Search/filter in tree
- **WHEN** user types in the search input at the top of the tree
- **THEN** the tree SHALL filter to show only files/directories matching the query

### Requirement: CodeMirror 6 editor as tab
The system SHALL open files in a CodeMirror 6 editor rendered as a tab in the right panel. The editor SHALL support syntax highlighting, line numbers, code folding, search/replace, and dark theme.

#### Scenario: Open file from tree
- **WHEN** user double-clicks a file in the project tree
- **THEN** a new tab SHALL open in the right panel with the file content in CodeMirror 6

#### Scenario: Syntax highlighting
- **WHEN** a file is opened in the editor
- **THEN** the editor SHALL apply syntax highlighting appropriate to the file's language

#### Scenario: Editor dark theme
- **WHEN** the editor renders
- **THEN** it SHALL use a dark theme consistent with the cluihud color palette

#### Scenario: File editing
- **WHEN** user modifies content in the editor and saves (Ctrl+S)
- **THEN** the changes SHALL be written to the file on disk

### Requirement: Core language support
The editor SHALL include syntax highlighting for TypeScript, JavaScript, Rust, JSON, Markdown, CSS, and HTML. Additional languages SHALL be loadable on demand.

#### Scenario: TypeScript file highlighting
- **WHEN** user opens a .ts or .tsx file
- **THEN** the editor SHALL display TypeScript syntax highlighting

#### Scenario: Rust file highlighting
- **WHEN** user opens a .rs file
- **THEN** the editor SHALL display Rust syntax highlighting
