# REVIEW — harden-fs-command-path-traversal

## Escalated 3-parallel review (risk_tier critical) · sonnet · 2026-07-03

**Consolidated verdict: PASS** — zero BLOCKER/MAJOR across all three lenses.

### security-reviewer (adversarial) — PASS

Attacked all 7 vectors, no bypass:
- `..` matched on parsed `Component::ParentDir` (encoded variants become literal
  harmless filenames); `starts_with` empirically confirmed component-wise
  (`/foobar`.starts_with(`/foo`) = false — no sibling-name escape);
  `git show :N:<path>` can't be flag-injected (arg always starts with `:`);
  symlinked-ancestor escape rejected even in the new-leaf walk-up; fail-closed on
  base-canonicalize failure; error strings never echo the input.
- 2 MINOR/informational (not required to ship): (1) structural TOCTOU if a *local
  racing process* plants a symlink between check and write — outside this change's
  threat model (untrusted webview content, not local code-exec); (2)
  `save_conflict_resolution` passes raw `path` to `stage_file` — already `..`-vetted
  by the guard `?` above it + `git add --` prevents flag injection; defense-in-depth
  nit only.

### spec-reviewer — PASS

All 7 commands routed (grepped; no surviving raw join on the guarded surface; the
remaining `.join`s are on trusted git-output paths). All spec scenarios trace through.
Module placement (leaf `fs_guard.rs`) accepted vs the cycle colocation would create.
1 LOW-MEDIUM finding: `read_openspec_artifact`/`write_openspec_artifact` retain
active/archive/master dispatch logic beyond the guard call that no test exercises —
containment holds (guard invoked in every branch) but the dispatch could regress
silently. → **follow-up** (extract the change_dir resolution to a testable pure helper);
not a blocker.

### code-quality-reviewer — PASS

`canonicalize_with_missing_tail` termination + re-append order traced correct; no
false-positive rejections; `strip_prefix` in file_conflict_versions can't legitimately
fail (same canonicalize on same input). 2 optional nits: less-clear error message when
an openspec base dir doesn't exist (only reachable with a bogus change_name); `cwd`
canonicalized twice in file_conflict_versions (redundant syscall, not a bug).

## Orchestrator decision on findings

All findings MINOR/follow-up; none blocks a 🔐 critical change whose containment
property is proven. Recorded as follow-up candidates (not filed as new changes now):
1. Extract openspec change_dir active/archive/master dispatch → testable helper + 3-4
   tests (spec-reviewer).
2. Route `stage_file` via a vetted-path re-derivation for consistency (security nit).
Both are non-security-critical hardening/coverage; batch into a later cleanup if desired.

## Gates

- Gate 1-3: PASS (clippy -D warnings clean, 752 tests incl. 7 new fs_guard, fmt clean,
  tsc clean).
- Gate 4 (security): the change IS the security control; 3-parallel escalation ran.
- Gate 5 (deps): `dunce` already a dependency — no manifest change.
- Gate 6 (scope): 4 files, all named in the proposal.
