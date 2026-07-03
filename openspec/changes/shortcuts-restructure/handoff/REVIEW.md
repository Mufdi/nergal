# REVIEW — shortcuts-restructure

## iprev round 1 (2026-07-03, evaluator: fresh subagent, model fable)

**Verdict: NEEDS-REVISION** — 1 CRITICAL, 5 MAJOR, 6 MINOR. All 12 vetted against source by the architect (F4/F5/F7 re-verified in code) and accepted; F9 subsumed by F1. Resolutions applied:

| # | Sev | Finding | Resolution |
|---|-----|---------|-----------|
| 1 | CRIT | Leader machine placed after Tab-forwarding/quake/Ctrl+K hardcodes → raw-mode `Ctrl+K` opens palette, Tab leaks to PTY mid-pending | Split ordering: **resolution** right after keymap-capture bail (owns every keydown while pending); **activation** after dialog/palette guards. design D9 + impl step 9 + task 3.1 |
| 2 | MAJ | command-palette MODIFIED header didn't match baseline ("Keybinding display") | Renamed |
| 3 | MAJ | Baseline "Scratchpad toggle shortcut" (`Ctrl+Alt+L`) + scratchpad context scenario not modified → post-merge contradiction | Two MODIFIED requirements added (`leader s`) |
| 4 | MAJ | Baseline command-palette "Backward compatibility" freezes the registry literal → contradicts removals | REMOVED with reason (was scoped to the Obsidian-templates change) |
| 5 | MAJ | `browser.rs` reserved `ctrl+shift+0 → toggle-mode` survives zen remap; leader dead in iframe focus | Kept as iframe-scoped deliberate exception (no Rust change); documented in spec *Shortcut scopes* + design + impl + docs task |
| 6 | MAJ | Leader lock vs fallback contradictory; hand-edited override is dead code today (`resolvedShortcutsAtom` ignores locked ids) | Normative now: UI-only lock, override honored for `leader` specifically (spec scenario added) |
| 7 | MIN | `sendSpecialKeyToActive` signature misstated (arg order; ctrl/alt omitted) | Corrected (`terminalService.ts:477`); ctrl/alt support validates raw-mode feasibility |
| 8 | MIN | "No Rust/backend changes" vs `keyboard_ownership` in config.rs | Proposal Impact corrected (one additive serde field); hash re-locked |
| 9 | MIN | Impl self-inconsistent on machine insertion point | Subsumed by #1 |
| 10 | MIN | Which-key grouping not derivable from registry (category ≠ family) | `group?: string` field added to `ShortcutAction` (spec + impl + task 2.1) |
| 11 | MIN | Surviving override can collide with remapped default (first-match-wins shadowing) | Startup cleanup extended: drop colliding overrides + notice (spec scenario added) |
| 12 | MIN | Quake `Ctrl+W`/`Ctrl+Shift+T` hardcodes bypass ownership switch silently | Carved out as deliberate mode-independent exception (spec scenario + design + docs task); shell readline via `leader .` |

CHECKS-PASSED (round 1): all ~30 implementation.md line refs exact; binding map consistent across artifacts, no duplicate assignments; keyboard-shortcuts delta hygiene clean; WebKitGTK `event.code` discipline maintained.

## iprev round 2 (2026-07-03, same evaluator, fresh verification pass)

**Verdict: APPROVED.** All 12 round-1 fixes verified resolved against the artifacts and re-checked against source (split dispatcher ordering coherent across design/impl/tasks and satisfies the consume-all + raw-mode spec scenarios; both scratchpad MODIFIED headers byte-exact vs baseline; browser.rs chord list exact; leader-lock semantics now stated identically in all five artifacts; exactly one "No swallowed no-ops" scenario; binding map unchanged and collision-free).

Two MINOR leftovers reported and fixed post-approval by the architect: implementation.md edge-case bullet still deferred the (already resolved) leader-lock decision → now references D9; proposal What-Changes migration bullet understated the F11 collision cleanup → amended, hash re-locked. `openspec validate` green.
