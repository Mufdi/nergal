# ARCHITECT-BRIEF — git-panel-chips

## Project mission

cluihud is a desktop wrapper for Claude Code CLI on Linux. It does NOT replace the terminal, reimplement Claude Code, or act as an agent framework. It augments the UX around an existing `claude` session by observing it via hooks and surfacing session-relevant panels.

**Useful changes** improve the experience of using Claude Code: plan editing UX, task visibility, hook-driven panels, keyboard navigation. **Out of scope**: replacing Claude-native features, terminal-level work.

## Context

Ship-flow-v3 (phases 1–9) landed 2026-04-27. Manual UX walks (task 9.3) surfaced bugs:

1. Ctrl+1/2/3 in ShipDialog fires correctly but BranchPicker only shows when `Commit+Push+PR` is **mouse-hovered**, not when keyboard-armed via Ctrl+3.
2. PR Viewer's "Apply with Claude" button shows disabled with no tooltip explaining why.
3. PR/Conflicts as document tabs feels wrong (close-x button suggests they're transient).
4. Files/PRs sidebar toggle has implicit focus-switching between commit history and the right column.

This change refactors the GitPanel into a 5-chip dedicated navigation system, mirroring the proven `SpecPanel` chip pattern (`Shift+←/→` between sub-tabs, line 380 of `src/components/spec/SpecPanel.tsx`).

The remaining manual-walk bugs (1, 3 from above + the others reported) are deferred — this refactor is the foundation that makes them easier to fix.

## Sprint Contract

See `proposal.md` § "Build contract".

## Dependencies / blockers

- None. Backend stash commands are additive. Frontend store changes are isolated to git-related atoms.
- Backwards-compat: NONE required. cluihud is single-user dev tool, no data migration.

## Risk tier: medium

- **High regression surface**: GitPanel is the daily-driver surface for the user's git workflow. A regression here blocks shipping.
- **No security/data risk**: no auth, no schema, no migrations, no external integration changes.
- **Reversible**: every change is UI-only; rollback is `git revert`.

## Gating decision

OFF (single-sequential reviewer default). No `migration` / `security` / `breaking-change` tags.

If during execution `files_touched > 21` (1.5× of 14 estimate), auto-escalate to 4-parallel reviewer per `/work` rules.

## Lazy skills considered

- `tauri-app-dev` (already loaded in project) — applies for the new Tauri commands.
- `rust-style` — applies for backend stash command additions.
- `frontend-build` — N/A, this is refactor not greenfield.
- `redesign-existing-projects` — N/A, this is a focused panel refactor not a full redesign.

No additional lazy skills required.

## Implementation order

Strict phase order (each phase verifies before the next):

1. Backend (stash commands) — independent, can ship alone.
2. Store refactor — must precede component refactor.
3. Files chip + skeleton — establishes the chip strip.
4. History chip — quickest extraction.
5. Stashes chip — uses Phase 1 backend.
6. PRs chip — heaviest migration (PR Viewer logic moves).
7. Conflicts chip — last domain chip; auto-redirect logic depends on PRs chip existing.
8. Chip nav + expansion — cross-cutting wiring.
9. Cleanup deletions — only after all chips work.
10. Verification + spec deltas.
