# REVIEW — remove-dead-askuser-slice

## Orchestrator vet (S deletion, compiler-guided; no reviewer) · 2026-07-04

**Verdict: PASS.** Gate 0.1 confirmed by the user (commit to notifier-only).

- **Zero dead-slice refs remain**: grep for `submit_ask_answer`/`pending_ask_fifos`/
  `askUserAtom`/`AskUserModal` finds only the module-doc lines describing what was removed
  (historical record, correct) — no live code.
- **`ASK_USER_BLOCKING` survives** (D2): present in mod.rs (bitflag def/Display/FromStr)
  + declared by CC/Codex, asserted absent for Pi; the existing
  `capabilities_include_all_known_flags` test asserts CC still contains it — test-verified
  post-deletion, not just visual.
- **Live attention flow survives**: `pendingAsksAtom`/`pendingAttentionAtom` still consumed
  by hooks.ts/TopBar/SessionRow/SessionIndicator (grepped post-deletion).
- **433 deletions, 13 insertions**: whole `AskUserModal.tsx` (337) + `submit_ask_answer`
  command/trait/CC override + `pending_ask_fifos` + `askUserAtom` plumbing. Module doc
  rewritten to the current architecture (plan FIFOs live, ask-user = non-blocking notifier,
  dialog flow removed, git history is the archive).
- Compiler flagged nothing beyond the task list — clean deletion.

## Gates

- Gate 1-3: PASS (clippy --all-targets clean, 779 cargo tests, fmt clean, tsc clean,
  81 vitest).
- Gate 6 (scope): 7 files, all in the deletion checklist.
