# SUPERSEDED — ship-flow-v2

This change was a partial pivot of `ship-flow` (v1) and never reached
implementation completion. After live testing during the implementation
phase the design pivoted again based on direct user feedback (auto-merge
checkbox killed, single-pane modal with progressive action buttons,
PR Viewer as a tab, annotations as instructions to Claude, etc.).

**Successor**: `openspec/changes/ship-flow-v3/`

What survived from v2 and lives in code:
- `Kbd` component (`src/components/ui/kbd.tsx`) with OS-aware glyphs and `tone` prop
- `cleanup_merged_session` total-deletion semantics (`src-tauri/src/commands.rs`)
- `git_auto_merge_default` config field (kept; v3 may repurpose or remove)
- ConflictsPanel space-toggle bug fix
- ShipDialog Step1 staging picker (will be inlined into v3 single-pane)
- BranchPicker custom dropdown (reused in v3 conditional flow)
- Sidebar Merge entry-point removal

What v3 reverses or rewrites:
- 2-step ShipDialog → single pane with 3 progressive action buttons
- Auto-merge checkbox → removed entirely (merge is an explicit action in PR Viewer)
- Visible "Merge" button in GitPanel commit bar → removed
- Auto-cleanup poll on PR-MERGED state → moved into the explicit "Merge into main"
  click path inside the PR Viewer
- `sessionsAutoMergedAtom` → removed (no auto-merge means no marker needed)

Kept here for historical traceability; do not implement.
