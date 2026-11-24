## ADDED Requirements

### Requirement: PR viewer merges with M after a confirmation
In the PR viewer, `M` (with or without Shift, and without Ctrl, Alt or Meta) SHALL open a confirmation dialog for merging the displayed PR, and SHALL merge only when the user confirms. The PR viewer SHALL NOT handle `Ctrl+Enter`; `Ctrl+Enter` keeps its global meaning (toggle the fullscreen terminal). The Merge button SHALL show `M` as its shortcut chip, and a click on it SHALL open the same confirmation.

#### Scenario: M asks before merging
- **GIVEN** the PR viewer shows open PR #7 into `main` with no annotations
- **WHEN** the user presses `M`
- **THEN** a confirmation dialog titled "Merge PR #7?" names `main` as the target
- **AND** confirming runs the merge; cancelling merges nothing

#### Scenario: M with unresolved annotations
- **GIVEN** the PR viewer shows open PR #7 with 3 annotations
- **WHEN** the user presses `M`
- **THEN** the confirmation body states that 3 annotations are unresolved
- **AND** confirming merges the PR

#### Scenario: Ctrl+Enter no longer merges
- **GIVEN** the PR viewer is active and focus is not in a text field
- **WHEN** the user presses `Ctrl+Enter`
- **THEN** no merge starts and no merge confirmation opens
- **AND** the fullscreen terminal toggles

### Requirement: The M merge key is guarded
`M` SHALL do nothing while the PR is not open, while a merge runs, while a dialog or a merge confirmation is open, in annotation mode, while focus is in a text field, on a key repeat, or when the keypress completes a leader chord. The PR state SHALL be checked again after the user confirms.

#### Scenario: Leader m does not open the merge confirmation
- **GIVEN** the PR viewer is active
- **WHEN** the user presses `Ctrl+Space` then `m` (Complete Merge)
- **THEN** no merge confirmation opens

#### Scenario: A held M opens one confirmation
- **GIVEN** the PR viewer shows open PR #7
- **WHEN** the user holds `M` so the key repeats
- **THEN** exactly one confirmation opens

#### Scenario: PR merged while the confirmation is open
- **GIVEN** the merge confirmation for PR #7 is open
- **WHEN** a refresh marks PR #7 as merged and the user then confirms
- **THEN** no merge request is sent

#### Scenario: Displayed PR changes while the confirmation is open
- **GIVEN** the merge confirmation for PR #7 is open
- **WHEN** the viewer switches to PR #9 and the user then confirms
- **THEN** no merge request is sent for #7 or #9

#### Scenario: The Merge button asks too
- **GIVEN** the PR viewer shows open PR #7 with no annotations
- **WHEN** the user clicks "Merge into main"
- **THEN** the "Merge PR #7?" confirmation opens and nothing merges until the user confirms

#### Scenario: M is inert when the PR is closed
- **GIVEN** the PR viewer shows a merged or closed PR
- **WHEN** the user presses `M`
- **THEN** no confirmation opens

### Requirement: Git panel hint bars share one standard
Every keyboard hint bar in the git panel SHALL use one shared hint bar: a row at the bottom of its view, below the scroll area, with an opaque card background and a top border, that wraps when narrow, with 10px labels and 10px `Kbd` chips. This SHALL apply to the bottom hint row of the Files, History, Branches, Stashes, PRs list, Conflicts and PR viewer views and of the PR viewer and Conflicts file pickers.

#### Scenario: PRs list footer matches the other views
- **WHEN** the PRs list renders its footer
- **THEN** the footer is the shared hint bar, with 10px text and chips, pinned to the bottom of the list

#### Scenario: Short list keeps the bar at the bottom
- **GIVEN** the Files view lists 2 files in a tall panel
- **THEN** the hint bar sits at the bottom edge of the view, not under the last row

#### Scenario: Narrow panel wraps the hint bar
- **GIVEN** the Branches view hint bar holds 7 hints
- **WHEN** the panel is narrower than the hints
- **THEN** the hints wrap onto a second row instead of being clipped

### Requirement: The PR viewer lists its keys
The PR viewer hint bar SHALL list `j/k` (hunks), `a` (annotate the hunk), `Ctrl+←/→` (files), `Ctrl+Shift+K` (file picker) and `M` (merge), and SHALL hide `j/k` and `a` when the PR has no hunks, `Ctrl+Shift+K` when it has no files, and `M` when the PR is not open. The Conflicts hint bar SHALL list `Ctrl+Shift+K` (file picker) whenever there are conflicted files, even before a file shows regions. A hint beside a single control in the git panel (a pane header or a field label) MAY stay inline and SHALL use 10px text.

#### Scenario: PR viewer shows its keys
- **GIVEN** the PR viewer shows an open PR with at least one hunk
- **THEN** its hint bar shows chips for `j`/`k`, `a`, `Ctrl+←`/`Ctrl+→` and `M` with their labels

## MODIFIED Requirements

### Requirement: PRs chip is a picker → embedded viewer
The `PRs` chip SHALL render a file-picker-style PR list (j/k/Enter to select). Selecting a PR replaces the chip body with the `PrViewer` embedded in place plus a "All PRs" back-strip. `Backspace` returns to the picker.

#### Scenario: Backspace returns to picker
- **WHEN** the user is viewing a PR (`selected !== null`)
- **AND** presses `Backspace` outside an input/textarea
- **THEN** the chip returns to picker mode

#### Scenario: Apply with Claude tooltip on disabled
- **WHEN** the Apply-with-Agent button is disabled (no owning session, or owning session not active)
- **AND** the user hovers it
- **THEN** a Tooltip surfaces the specific gating reason (no matching branch / wrong active session)
- **AND** the button label includes the annotation count: `Apply with Agent (N)`

#### Scenario: Apply with Agent submits the prompt
- **GIVEN** the PR viewer has 2 annotations and the owning session is active
- **WHEN** the user clicks "Apply with Agent (2)" (or presses `Ctrl+Shift+R`)
- **THEN** `deliver_prompt_to_session` receives the structured prompt for that session
- **AND** the agent receives it as one turn, with no Enter needed from the user
- **AND** a second click or `Ctrl+Shift+R` before the delivery resolves sends nothing
- **AND** a toast titled "Apply with Agent" reports 2 annotations sent
