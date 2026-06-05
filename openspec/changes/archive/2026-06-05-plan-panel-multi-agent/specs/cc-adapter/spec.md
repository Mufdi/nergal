## MODIFIED Requirements

### Requirement: Plan watcher respects user's plansDirectory configuration

CC's plans-directory resolution SHALL be centralized in a single resolver consumed by both the plan watcher (`agents/claude_code/plan.rs`) and the `list_session_plans` Tauri command (via `plan_capability()`). The CC adapter's `plan_capability(session, cwd)` SHALL return `FileBased { dir: <resolved>, label: "Claude Code" }` where `<resolved>` is computed as:

1. Read `~/.claude/settings.json`, `<cwd>/.claude/settings.json`, and `<cwd>/.claude/settings.local.json` in CC's precedence order (project-local files override home).
2. Extract the merged `plansDirectory` string if any layer sets it.
3. Expand a leading `~/` against `$HOME`.
4. If the resulting path is absolute → use as-is.
5. If the resulting path is relative → join against `cwd`.
6. If no `plansDirectory` is set in any layer → fallback to `<cwd>/.claude/plans/`.

The plan watcher startup in `lib.rs` SHALL call this resolver instead of consuming `config.plans_directory`. The two code paths that previously diverged (watcher used cluihud config; `list_session_plans` hardcoded `<cwd>/.claude/plans/`) MUST be unified through this resolver. This path convention SHALL remain CC-specific; other adapters' `plan_capability()` implementations MUST NOT use this resolver.

#### Scenario: Plan loaded from worktree-local plans dir (default case)
- **WHEN** a CC session in `/path/to/worktree` has plans at `/path/to/worktree/.claude/plans/<plan>.md`
- **AND** no `plansDirectory` is set in any settings layer
- **THEN** `plan_capability()` returns `FileBased { dir: "/path/to/worktree/.claude/plans", label: "Claude Code" }`
- **AND** both the watcher and `list_session_plans` SHALL use that path

#### Scenario: Absolute plansDirectory respected
- **WHEN** `~/.claude/settings.json` contains `"plansDirectory": "/custom/path/plans"`
- **THEN** `plan_capability()` returns `FileBased { dir: "/custom/path/plans", label: "Claude Code" }`

#### Scenario: Relative plansDirectory resolves against cwd
- **WHEN** `~/.claude/settings.json` contains `"plansDirectory": ".claude/plans"` AND the session `cwd` is `/path/to/repo`
- **THEN** `plan_capability()` returns `FileBased { dir: "/path/to/repo/.claude/plans", label: "Claude Code" }`

#### Scenario: Tilde expansion
- **WHEN** `~/.claude/settings.json` contains `"plansDirectory": "~/notes/plans"`
- **THEN** `plan_capability()` returns `FileBased { dir: "<HOME>/notes/plans", label: "Claude Code" }`

#### Scenario: Project-local settings override home settings
- **WHEN** `~/.claude/settings.json` sets `"plansDirectory": "/global"`
- **AND** `<cwd>/.claude/settings.json` sets `"plansDirectory": "/project"`
- **THEN** `plan_capability()` returns `FileBased { dir: "/project", label: "Claude Code" }`

#### Scenario: settings.local.json overrides settings.json in cwd
- **WHEN** `<cwd>/.claude/settings.json` sets `"plansDirectory": "/team"`
- **AND** `<cwd>/.claude/settings.local.json` sets `"plansDirectory": "/personal"`
- **THEN** `plan_capability()` returns `FileBased { dir: "/personal", label: "Claude Code" }`

#### Scenario: settings.json missing or no plansDirectory
- **WHEN** no settings layer contains `plansDirectory`
- **THEN** `plan_capability()` returns `FileBased { dir: "<cwd>/.claude/plans", label: "Claude Code" }`
