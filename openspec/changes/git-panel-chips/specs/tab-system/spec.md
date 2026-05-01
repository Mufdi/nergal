## MODIFIED Requirements

### Requirement: TabType union no longer includes pr or conflicts
The `TabType` union SHALL be `"plan" | "diff" | "spec" | "tasks" | "git" | "transcript" | "file"`. The `pr` and `conflicts` values are removed because both surfaces moved into the GitPanel chip system and no longer materialize as document tabs.

#### Scenario: Type union audit
- **WHEN** any code attempts to set `tab.type = "pr"` or `"conflicts"`
- **THEN** TypeScript rejects it (compile-time safety)

#### Scenario: PANEL_CATEGORY_MAP audit
- **WHEN** the panel category map is queried for any TabType
- **THEN** it returns `"document"` or `"tool"` for the 7 remaining types; `pr` and `conflicts` are absent

### Requirement: SINGLETON_TYPES no longer includes conflicts
`SINGLETON_TYPES` SHALL be `["tasks", "git"]`. `conflicts` is removed; the chip is intrinsically singleton-by-construction (per workspace, persisted in `gitChipModeAtom`).

#### Scenario: openTabAction singleton check
- **WHEN** `openTabAction` is invoked with `tab.type = "tasks"` or `"git"` and a tab of that type already exists
- **THEN** the existing tab is focused (no duplicate)
- **AND** the `conflicts` branch in the singleton check is gone — there is no tab of that type to deduplicate

## REMOVED Requirements

### Requirement: Conflicts tab pinned at leftmost position
**Reason**: No conflicts tab exists.
**Migration**: TabBar's pin-conflicts-first logic is dropped — `tabs = useAtomValue(activeTabsAtom)` returns the natural order.

### Requirement: TAB_ICONS includes pr and conflicts
**Reason**: No icons needed for tab types that don't exist.
**Migration**: `GitPullRequest` and `AlertTriangle` removed from `TAB_ICONS` in both `RightPanel.tsx` and `TabBar.tsx`.
