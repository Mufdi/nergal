## ADDED Requirements

### Requirement: PlanCapability enum models agent-specific plan persistence

The `AgentAdapter` trait SHALL expose `fn plan_capability(&self, session: &Session, cwd: &Path) -> PlanCapability`. The `PlanCapability` enum SHALL have exactly two variants in v1:

- `FileBased { dir: PathBuf, label: String }` — the agent persists plans as markdown files in a discoverable directory. `dir` is the absolute resolved path to scan; `label` is the agent's display name used in the panel UI.
- `NotApplicable` — the agent has no file-based plan model; the Plans panel SHALL be hidden for this session.

The default trait impl SHALL return `NotApplicable`. Adding new variants is a non-breaking change for adapters that don't claim them.

#### Scenario: CC adapter declares FileBased
- **WHEN** `cc_adapter.plan_capability(session, cwd)` is called
- **THEN** the result is `FileBased { dir: <resolved>, label: "Claude Code" }`
- **AND** `<resolved>` is computed per the CC plans-directory resolution rules in the `cc-adapter` spec

#### Scenario: OpenCode adapter declares NotApplicable
- **WHEN** `opencode_adapter.plan_capability(session, cwd)` is called
- **THEN** the result is `NotApplicable`
- **AND** the rationale (opencode plan mode is read-only and refuses to write `.opencode/plans/*.md` per upstream issue #11078) is captured in the source comment

#### Scenario: Codex adapter declares NotApplicable
- **WHEN** `codex_adapter.plan_capability(session, cwd)` is called
- **THEN** the result is `NotApplicable`

#### Scenario: Pi adapter declares NotApplicable
- **WHEN** `pi_adapter.plan_capability(session, cwd)` is called
- **THEN** the result is `NotApplicable`

### Requirement: list_session_plans command delegates to active adapter

The `list_session_plans` Tauri command SHALL resolve the active session's `agent_id`, look up the adapter via `AgentRegistry`, call `plan_capability(session, cwd)`, and return a structured response. The response wire shape SHALL be:

```
{ capability: "FileBased", dir: string, plans: PlanSummary[] }
| { capability: "NotApplicable", plans: [] }
```

`plans` MUST be sorted by modified time descending. The command MUST NOT hardcode any path — the current `cwd.join(".claude").join("plans")` literal is removed.

#### Scenario: CC session with plans
- **WHEN** `list_session_plans` is called for a CC session whose resolved plans dir contains `plan-a.md` and `plan-b.md`
- **THEN** the response shape is `{ capability: "FileBased", dir: "<resolved>", plans: [..., ...] }`
- **AND** the plans are sorted by modified time descending

#### Scenario: OpenCode session
- **WHEN** `list_session_plans` is called for an OpenCode session
- **THEN** the response is `{ capability: "NotApplicable", plans: [] }`

#### Scenario: Codex session
- **WHEN** `list_session_plans` is called for a Codex session
- **THEN** the response is `{ capability: "NotApplicable", plans: [] }`

#### Scenario: Pi session
- **WHEN** `list_session_plans` is called for a Pi session
- **THEN** the response is `{ capability: "NotApplicable", plans: [] }`

### Requirement: get_session_plan_capability command exposes capability without scanning

The Tauri command `get_session_plan_capability(session_id) -> PlanCapabilityWire` SHALL return the active capability for a session without performing a filesystem scan. This is used by the frontend to decide panel visibility before incurring scan cost.

#### Scenario: Capability query is cheap
- **WHEN** the frontend calls `get_session_plan_capability` for any session
- **THEN** the response is computed from in-memory adapter state + settings.json reads only
- **AND** no directory scan is performed

### Requirement: Plans entry hidden from right-panel chrome when NotApplicable

The frontend SHALL omit the "Plans" entry from the right-panel switcher chrome when the active session's `plan_capability` is `NotApplicable`. The omission SHALL be reactive: switching from a `FileBased` session to a `NotApplicable` session SHALL remove the entry without requiring a panel re-mount.

#### Scenario: Switching to Codex hides Plans
- **WHEN** the active session changes from a CC session (FileBased) to a Codex session (NotApplicable)
- **THEN** the Plans entry SHALL be removed from the right-panel switcher chrome
- **AND** if Plans was the currently visible panel, it SHALL transition to the configured fallback panel (default: Activities)

#### Scenario: Switching back to FileBased restores Plans
- **WHEN** the active session changes from a NotApplicable session to a FileBased session
- **THEN** the Plans entry SHALL reappear in the switcher

#### Scenario: No active session
- **WHEN** no session is active
- **THEN** the Plans entry SHALL behave as before (panel visible but empty) — no NotApplicable semantics applied

### Requirement: FileBased empty state surfaces resolved path and contextual hint

When the Plans panel renders for a `FileBased` session and the scanned directory is empty, the empty state SHALL display the resolved directory path AND a secondary hint specific to the agent.

#### Scenario: CC session with empty plans dir
- **WHEN** a CC session's resolved plans dir exists but contains no `.md` files
- **THEN** the panel renders primary text `"No plans found at <path>"`
- **AND** secondary text `"Check plansDirectory in ~/.claude/settings.json"`

#### Scenario: Resolved dir does not exist on disk
- **WHEN** the resolved dir for a FileBased session does not exist on disk
- **THEN** the same empty state renders (no error)
- **AND** the secondary hint guides the user to the configuration that controls the path

### Requirement: Decorative plans_directory cluihud config field removed

The `Config::plans_directory` field in `~/.config/cluihud/config.json` SHALL be removed from the active config schema. The corresponding form field in `SettingsPanel.tsx` SHALL be removed. Plan path resolution SHALL come from the agent adapter's `plan_capability()`, not from cluihud's own config.

#### Scenario: Legacy config tolerated on load
- **WHEN** cluihud loads a `config.json` containing a legacy `plans_directory` field from a previous version
- **THEN** the field SHALL be ignored without error
- **AND** the config SHALL load successfully
- **AND** on next save, the field SHALL NOT be re-emitted

#### Scenario: Settings UI no longer shows the field
- **WHEN** the user opens Settings → relevant section
- **THEN** no `plans_directory` form input is rendered
- **AND** the new behavior (path comes from agent) is documented inline or in a tooltip
