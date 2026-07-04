# REVIEW — split-god-modules

## Reviewer: code-quality + equivalence (single, sonnet) · 2026-07-04

**Verdict: PASS** — zero findings. Exhaustive independent verification of a verbatim split.

- **Verbatim spot-checks**: 10 fns byte-diffed orig-vs-new (6 commands across git/ship_pr/
  conflicts/plans/files/config_misc + 4 db across domains) — all byte-identical bodies.
- **The 4 compiler-surfaced bug fixes verified correct**: exactly one `git_push` now
  (body = the original's single copy, not a merge artifact); a script paired all 120
  `#[tauri::command]` attrs to their fns — 120/120, zero orphans, name-set identical to
  the original (the regained-attr fn landed on the right fn); the 2 added imports are a
  subset/relocation of commands.rs's existing top-level imports — no new external dep.
- **Membership deviations (D1)**: resolve_openspec_dir/search/probe_workspace_path/
  set_active_clickup_task all byte-identical in their new homes; assignments reasonable.
- **Visibility**: a by-name visibility diff found ONLY 4 widenings, all the shared
  cross-domain helpers, private→pub(crate) (NOT pub) — the minimum for re-export; zero in
  db/. No unrelated fn touched.
- **`#[cfg]` sites**: independently recounted — 10 total (5 non-test + 3 test-mod in
  commands, 2 test-mod in db); all present + correctly distributed in the new tree,
  including the unix/windows/not-any triple in `submit_plan_decision` (all 3 branches +
  comments intact). None dropped.
- **Test equivalence**: `#[test]` fn name sets identical — 13/13 commands, 26/26 db. No
  test deleted or added to paper over a gap.
- **Suite**: clippy --all-targets clean, 779 tests (baseline), fmt clean.

## Orchestrator vet (pre-review)

120 commands = 120, 86 db pub fns = 86, lib.rs ZERO diff, 8 non-test `#[cfg]` = 8, zero
duplicate command fn names, fresh_db@v32 intact.

## Gates

- Gate 1-3: PASS (clippy --all-targets, 779 tests, fmt, tsc).
- Gate 6 (scope): 22 new files + 2 deleted = the split; verbatim, no logic change.
- Gate 4.x (cross-engine `#[cfg]`): the windows-check/macos-cross-check CI jobs are the
  authoritative gate for the moved-but-gated fns (D6) — validated on push, not locally.
