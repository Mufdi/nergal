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

---

# Mode B implementation review — 2026-07-03 (4-parallel, escalated by scope threshold)

## Reviewer: spec (sonnet)
**PASS.** All requirement scenarios traced to code, D9 ordering verified line-by-line, verification re-run independently (tsc/vitest/cargo/clippy green). Findings, all MINOR:
1. Some UI hint chips hardcode chord literals (FilesChip/GitPanel/ConflictsPanel/TopBar) instead of deriving from `resolvedShortcutsAtom` — stale-on-remap for those chips only; mirrors pre-existing repo convention, task 6.1 wording was soft ("prefer"). Follow-up candidate.
2. Task 4.2 hint-variant: which-key popover implements it; the StatusBar breadcrumb itself does not restyle on `hintNonContinuation`. Spec text only requires the breadcrumb — tasks.md-only gap.
3. Note: in raw mode, Esc/leader-re-tap are FORWARDED (not cancel) — literal reading of the passthrough requirement ("any combination, including bare keys"); cancel scenarios apply to awaiting mode. Deliberate, desirable (Esc must be sendable to the agent).
Full tasks.md coverage table: 1.1–6.4 implemented (4.2 partial per finding 2), 7.3–7.7 manual walks N/A.

## Reviewer: code-quality (sonnet)
**PASS (0 must-fix).** Hot-path allocation profile unchanged (leader block only runs while pending). Findings:
- 🟡 `validateCombo` accepted a bare `.` continuation that `resolveContinuation`'s raw-mode sentinel makes unreachable (silent dead binding) → **FIXED** by orchestrator: chord branch now rejects `Period` without shift + test (47/47 green).
- 🟡 `AnnotationsDrawer.tsx:134,193` stale "(Ctrl+Shift+J)" tooltips missed by the sweep → **FIXED**: now "(Ctrl+Space D)".
- 🟢 `registry.find(a => a.id === "leader")?.keys ?? "ctrl+space"` duplicated in 4 components → follow-up: hoist to a `leaderKeysAtom`.

## Reviewer: security (sonnet — proportionate to surface; no auth/crypto/SQL in diff)
**PASS.** No exploitable findings. INFO-level notes: (1) raw-mode PTY forwarding adds no new privilege boundary (`terminal_input` invoke already reachable by any same-origin script; optional `e.isTrusted` gate suggested as defense-in-depth — deferred: needs a WebKitGTK/IM manual walk to rule out false-negatives on real input); (2) cross-origin iframe cannot reach the handler (frame-scoped keydown, no postMessage bridge); (3) `nergal:open-provider-status` blast radius = read-only public-status popover; (4) `keyboard_ownership` round-trip clean, correctly absent from `BACKEND_OWNED_CONFIG_KEYS`; (5) startup `save_config` writes just-loaded config, no partial-state clobber.

## Reviewer: deps (haiku)
**PASS.** vitest ^4.1.9 current/maintained, devDependencies-only, lockfile diff isolated to vitest + 18 transitives, zero unrelated bumps, Cargo.toml/lock untouched, no script collisions, zero new vulns (pre-existing prod vulns hono/qs/brace-expansion are unrelated to this change).

## Gating combine
spec PASS · security PASS · quality warnings fixed · deps PASS → **PROCEED**.

## Follow-ups (non-blocking, for the backlog)
- Hoist leader-binding lookup into a shared `leaderKeysAtom`; migrate the hardcoded chord chips (FilesChip/GitPanel/ConflictsPanel/TopBar) to derive from it.
- Consider `if (!e.isTrusted) return;` at the top of the dispatcher after a manual WebKitGTK/IBus walk confirms real keydowns always carry isTrusted.
- Optional: StatusBar breadcrumb variant on `hintNonContinuation` (which-key already covers the hint).
