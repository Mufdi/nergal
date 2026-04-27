## MODIFIED Requirements

### Requirement: Tab types support mixed content
The tab bar SHALL support these content types in the same bar: plan, diff, spec, tasks, git, transcript, file, conflict. Each type SHALL have a distinct icon. Singleton types (tasks, git) SHALL have at most one tab per session. `conflict` tabs SHALL be singleton per `(session, file path)` pair — opening a conflict tab for an already-open file focuses the existing tab.

#### Scenario: Singleton tab reuse
- **WHEN** tasks tab is open and user triggers "open tasks" again
- **THEN** the existing tasks tab is focused, no duplicate created

#### Scenario: Multiple file tabs
- **WHEN** user opens diff for "auth.rs" and then pins diff for "main.rs"
- **THEN** both file-specific tabs coexist in the tab bar

#### Scenario: Conflict tab reuse per file
- **WHEN** a conflict tab for `src/foo.ts` is open and user clicks Resolve on the same file again
- **THEN** the existing conflict tab is focused; no duplicate created

#### Scenario: Multiple conflict tabs for different files
- **WHEN** session has conflicts in `src/a.ts` and `src/b.ts` and user clicks Resolve on both
- **THEN** two conflict tabs coexist in the tab bar, one per file
