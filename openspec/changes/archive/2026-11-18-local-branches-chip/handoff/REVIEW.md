# Review: local-branches-chip

Reviewers ran once on the full diff (2026-11-18). All three used `sonnet`: the opus weekly limit was reached. None saw the Builders' reasoning.

## Adversarial (code-reviewer)

### Bugs, Important
- A1. The chip reloads only when the workspace has no cached list (`BranchesChip.tsx:70-73`), and `GitPanel` fills the cache once per workspace (`GitPanel.tsx:223-228`). After a session delete, the chip and its badge show a stale list. The spec says the list loads when the chip opens. → **Fix.**
- A2. `diff-tree --name-only` has no `-z` (`local_branches.rs:162`). Git quotes non-ASCII names, so the row shows a wrong name and the literal-pathspec diff comes back empty. → **Fix.**
- A3. "Missing" is decided by `NotFound` only, without git's `prunable` flag (`local_branches.rs:267-272, 298-322`). → **Fix** (require both).
- A4. `clear_missing_worktrees` runs before the `expected_sha` check (`branches.rs:157`). A stale-SHA refusal still changes the repo. → **Fix**: compare the SHA first, then clear.

### Compliance and design, Important
- A5. Terminology: the UI says "no session", while the code says `orphanBranchCount`. → **Accepted.** `orphanBranchCount` is an internal name pinned by a locked test. All user-facing copy says "no session".
- A6. The badge counts rows with no session, including rows that are "in use". → **Accepted and documented.** The badge signals branches that no session owns. The header gives the split.
- A7. Doc drift: `docs/architecture.md:41` and `docs/patterns.md` §4 list five chips. → **Fix.**
- A8. Stale comment at `src/stores/git.ts:83` says "persist per-workspace". → **Fix.**

### Spec conformance gaps (no test found)
- Delete with a locked missing worktree keeps the branch (command level). → **Add test.**
- Delete when the restore record cannot be written names the record path (command level). → **Add test.**
- Open session when the row insert fails. → Covered by the rival-owner test of `finish_branch_session`, which exercises the same rollback branch. **Accepted.**
- Delete waits for a running open, then names the session. The lock test covers the wait and the free DB. **Accepted**: the session-naming half is covered by `deleting_a_branch_a_session_owns_is_kept_naming_the_session`.
- Six-chip strip order, wrap and badge; History Zen file list; GitPanel prefill. → Frontend without a DOM: **manual walk** (tasks 6.4–6.10).

### Nits
- Double first fetch between `GitPanel` and the chip. → **Fix** with an in-flight guard per workspace.
- Load error swallowed. → Fixed by F1.
- Commit files cached by the short hash. → Accepted (unlikely collision inside one branch list).

## Security (security-auditor)

**PASS.** No boundary-crossing defect with a concrete result. The change also closes an existing option-injection path: the old `get_commit_files` passed an unvalidated `hash` to `diff-tree`.

Nits:
- S1. `create_worktree` callers do not take the repo lock. → Accepted: they always create a new timestamped branch (design Non-Goals).
- S2. `expected_sha` is a staleness guard, not authorization. → Accepted: it adds no capability over the PTY.
- S3. No `-z` on `diff-tree`. → Same as A2, fixed.
- S4. The restore log is created with the default umask. → Accepted: no secret data.
- S5. The diff output is unbounded. → Accepted: self-inflicted, local only.

## Frontend (frontend-architect, code level; rendered UI not available)

**FAIL → fixes below.**

- 🔴 F1. A first-load failure leaves "Loading branches…" forever, with no retry and no toast (`BranchesChip.tsx:81-83, 199-206`). → **Fix**: error state with a Retry button and an error toast.
- 🟡 F2. The actions are reachable only by Tab. → **Fix**: Enter on the cursor row opens a session and `D` deletes (with the confirm), as in `StashesChip.tsx:180,201`. Only when `branchActionsAllowed`. Footer Kbd hints.
- 🟡 F3. Commit and file rows have no focus ring and no keyboard path. → **Fix**: focus ring classes (`:focus` + `ring-inset`, WebKitGTK).
- 🟡 F4. The refresh button has no focus ring. → **Fix.**
- 🟡 F5. Action buttons have no row-specific accessible names. → **Fix**: `aria-label` with the branch name.
- 🟡 F6. The badge count and the header count disagree. → **Fix**: the header shows both numbers ("N · M without a session").
- 🟡 F7. The branch name is not quoted in the restore command. → **Accepted**: `nergal_slug` limits names to `[A-Za-z0-9._-]`, and the security reviewer agrees. A locked test pins the unquoted form.
- 🟡 F8. Two uncoordinated first fetches. → Same as the A-nit, fixed.
- Nits:
  - "pending" becomes "unpushed" in the row. **Fix.**
  - The confirm text with an unknown count. **Accepted**: a locked test pins "?".
  - Disable every action while one runs. **Fix.**
  - The "worktree missing" badge gets `tabIndex={0}`. **Fix.**
  - An empty state like `StashesChip`'s. **Fix.**

## Verdict

Block until F1 and A1–A4 are fixed. Rerun the gates after the fixes. No re-review: the fixes are local and each gets a test or a walk item.
