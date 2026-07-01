# plans-directory

## ADDED Requirements

### Requirement: Per-workspace plans directory override

The system SHALL let a user configure, per workspace, a directory where Nergal
looks for Claude Code's plan files. The override is **additive** — it augments
auto-detection (it tells Nergal where to look), it does NOT change where Claude
Code writes plans. The Settings UI SHALL show the auto-resolved default directory
alongside the override field so the user sees what Nergal detected.

#### Scenario: Override augments the search order

- **WHEN** a workspace has a `plans_dir` override set and Claude Code writes a
  plan to that directory
- **THEN** the plan panel surfaces the plan (the override directory is searched
  first, in addition to the auto-detected directories)

#### Scenario: Clearing the override falls back to auto-detection

- **WHEN** the `plans_dir` override is cleared (empty)
- **THEN** resolution falls back to the auto-detected directories (configured
  `plansDirectory`, project-local `<cwd>/.claude/plans`, home-global
  `~/.claude/plans`) with no behaviour change

#### Scenario: Settings shows the auto-resolved default

- **WHEN** the user opens the plans directory setting for a workspace
- **THEN** the auto-resolved default directory is shown (as the field
  placeholder), and any configured override is prefilled

### Requirement: Robust relative-path resolution

When the configured `plansDirectory` is a relative path, the system SHALL search
it resolved against BOTH the working directory and the home directory, so plan
discovery does not depend on which base the agent resolves relatives against on a
given OS.

#### Scenario: Relative plansDirectory resolved against both bases

- **WHEN** `plansDirectory` is a relative path (e.g. `.claude/plans`)
- **THEN** both `<cwd>/<rel>` and `<home>/<rel>` are included in the search set

### Requirement: Empty-state guidance

When the plan panel has no plan to show, the system SHALL display a hint that
points the user at the plans directory setting, so a user whose plans are written
to an unsearched location is guided to the override rather than left with a silent
empty panel.

#### Scenario: Empty panel links to the setting

- **WHEN** the plan panel is empty (no active plan)
- **THEN** a hint is shown that opens the plans directory setting
