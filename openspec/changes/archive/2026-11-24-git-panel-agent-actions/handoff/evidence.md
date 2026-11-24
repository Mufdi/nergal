# Evidence: git-panel-agent-actions (FRI-609)

## Baseline

Base `05c263b` (main after the checkpoint-1 merges, 2026-11-24). Full check run on that commit before this branch started:

- `cargo clippy --all-targets -- -D warnings`: no issues.
- `cargo test`: 1310 passed, 1 ignored (5 suites).
- `cargo fmt --check`: exit 0.
- `npx tsc --noEmit`: no errors.
- `pnpm test`: 55 files, 615 tests passed.

No pre-existing failures.

## Plan review

iprev with an independent opus evaluator, 3 rounds (`handoff/iprev-r1.md` … `iprev-r3.md`):
- Round 1: 11 findings (1 high: `leader m` also opening the merge confirmation). All applied.
- Round 2: 7 findings (3 medium: stale Apply guard, merge of a different PR after a viewer switch, no automated test of the delivery contract). All applied.
- Round 3: APPROVED; 5 low findings folded into tasks and implementation before the build.

Mechanism refinement recorded in `.work-modules.json`: "Apply with Agent" submits through an awaited `deliver_prompt_to_session` instead of the fire-and-forget `paste_and_submit_settled`, so in-flight guards and error toasts are truthful. The user's decision (submit at once) is unchanged.

## UI brief

Hint bar standard (design D2): one `HintBar` per git panel view, pinned at the bottom of the scroll area (`sticky bottom-0` as the last child of a scroll container, or a `shrink-0` sibling right after it), `flex-wrap` so narrow panels wrap instead of clipping, 10px labels, 10px `Kbd` chips (`size="md"`), `border-t border-border/40 bg-card/95`. Placeholders keep plain glyph text.

Tokens reused, none new: `border-border/40`, `bg-card/95`, `text-muted-foreground/70`, the existing `Kbd` subtle tone. Existing visual language unchanged (the PR viewer picker and Conflicts picker keep their card/header; only the hint text moved into a bar at the card bottom).

## Build (groups 1–5, then group 7 review fixes)

Builders: sonnet subagents. Red tests by a separate sonnet test-writer, locked with `git hash-object` (re-locked once for group 7, by the test-writer).

Deviations recorded by the builder: confirm body wording `N annotation(s) unresolved.` (kept within `confirmBodyEscaping.test.ts`); PR viewer verbs `j/k/a/Space` now ignore Ctrl/Alt/Meta (before, `Ctrl+J` moved a hunk).

Final verification on the branch (2026-11-24):
- `cargo clippy --all-targets -- -D warnings`: clean.
- `cargo test`: 1314 passed, 1 ignored (5 suites).
- `rustfmt --edition 2024 --check src/pty.rs src/lib.rs src/pty_deliver_prompt_tests.rs`: clean.
- `npx tsc --noEmit`: no errors.
- `pnpm test`: 56 files, 652 tests passed.
- `work-gates.sh main git-panel-agent-actions`: 6a PASS, 6b PASS, 6c PASS (gitleaks clean), Red locks PASS (2 files unchanged), deps SKIP.
- grep: no `terminal_deliver`, `applyWithClaude`, `askClaude`, `onAskClaude`, `handleSendToClaude` in `src/` or `src-tauri/src/`.

Reviewers: adversarial (opus) PASS, 3 Important + 5 nits; frontend (sonnet) PASS, 4 Important + 5 nits. Resolution in REVIEW.md.

## Walk (user, 2026-11-24)

Table E1–E9 approved on the installed `.deb` (`163f68a`). Follow-up from the walk: the `Ctrl+Shift+K` file picker hint was missing; added to the PR viewer bar (with PR files) and the Conflicts bar (with conflicted files, even without regions), task 7.7. The user waived a walk for that hint.

Final check after the follow-up: clippy clean; cargo test 1314 passed, 1 ignored; tsc clean; pnpm test 56 files, 652 tests; work-gates 6a/6b/6c PASS, Red locks PASS.
