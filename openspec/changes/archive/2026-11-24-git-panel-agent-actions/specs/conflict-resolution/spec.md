## MODIFIED Requirements

### Requirement: Conflict tab toolbar actions
The conflict tab SHALL expose four toolbar actions: Accept Ours, Accept Theirs, Ask Agent, Save Resolution. The intent field above Ask Agent is labelled "Intent for the agent (optional)".

#### Scenario: Accept ours copies ours to merged
- **WHEN** user clicks "Accept Ours"
- **THEN** the merged panel contents are replaced with the ours panel contents

#### Scenario: Accept theirs copies theirs to merged
- **WHEN** user clicks "Accept Theirs"
- **THEN** the merged panel contents are replaced with the theirs panel contents

#### Scenario: Ask Claude to resolve injects context and switches focus
- **WHEN** user clicks "Ask Agent" (or presses `Ctrl+Shift+R`) with an optional intent
- **THEN** the backend builds the conflict prompt from `{ path, ours, theirs, merged, intent }` (`build_conflict_prompt`)
- **AND** `deliver_prompt_to_session` pastes it into the session as one bracketed paste and submits it, so a multi-line prompt arrives as one turn
- **AND** focus switches to the terminal and a toast reads "Sent to agent"

#### Scenario: Save resolution writes, stages, closes, re-checks
- **WHEN** user clicks "Save Resolution"
- **THEN** the merged contents are written to the file on disk, the file is staged, the conflict tab is closed, and the git panel's conflict list is refreshed; if no conflicts remain, the conflict list section disappears

### Requirement: Contextual resolve-conflict shortcut dispatch
The shortcut currently bound to revise-plan SHALL be converted to a contextual dispatcher that emits one of two events depending on the active context, without introducing a new key binding.

#### Scenario: Active tab is conflict type
- **WHEN** user presses the contextual shortcut while a `conflict` tab is active
- **THEN** the system emits `nergal:resolve-conflict-active-tab` (triggers "Ask Agent" on the active tab)

#### Scenario: Git panel with conflicts but no conflict tab active
- **WHEN** user presses the contextual shortcut while the git panel is the active tab and the session has one or more conflicted files
- **THEN** the system opens/focuses the conflict tab for the first conflicted file

#### Scenario: Plan panel with pending review falls back to revise-plan
- **WHEN** user presses the contextual shortcut while a plan panel is active and a plan review is pending
- **THEN** the system emits `nergal:revise-plan` (current behavior)

#### Scenario: Neither context active is a no-op
- **WHEN** none of the above conditions apply
- **THEN** the system emits no event and shows no toast (silent no-op)
