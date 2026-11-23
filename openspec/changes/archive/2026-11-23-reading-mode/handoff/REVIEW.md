# Review: reading-mode (FRI-494)

## Adversarial (opus)

---
type: review
feature: reading-mode
reviewer: adversarial
reviewed_at: 2026-11-23T00:00:00Z
verdict: FAIL
---

# Review — reading-mode

> Rules: review only the diff and the artifacts named below. Skip anything a gate or CI already enforces (types, lint, formatting, test pass/fail). Tag every finding with its pass and its severity. **Important** = would ship a defect, a vulnerability, or a divergence from spec/plan. **Nit** = everything else; **max 5 nits**, drop the rest.

## Diff scope

- Files changed: 16 (9 tracked, 7 new) · LOC: +317 / -16 (tracked +54 / -16, new files 263 lines)

## Pass 1 — Bugs

- [Important] `src/components/terminal/terminalService.ts:481` — A flag change during a pending spawn is lost. The refit at `src/components/terminal/TerminalManager.tsx:87` calls `fitActive`. `fitActive` reads `entries` (`terminalService.ts:534-540`). A spawning terminal enters `entries` only at `:481`, after spawn, resize and attach. So a flag change in that window refits nothing. Nothing refits after `entries.set`. The terminal stays at the old width until a pane resize or a session switch. Two real triggers exist. First, the spec's late config: `waitForConfig` gives up, the spawn starts at 180 columns, then `get_config` resolves during spawn. Second, `leader f` pressed while a new session's terminal starts. This breaks the scenario "A late config still applies". → After `entries.set(key, entry)`, call `fit(entry)` when `activeIds.get(region) === key`. The resize dedup makes it free when nothing changed.
- [Important] `src/components/terminal/terminalService.ts:904` — Reading mode turns the pane margins into dead zones. The wheel listener sits on the canvas, not the container. On a 1440 px pane the capped canvas is 800 px wide. So 640 px of the pane ignore the wheel. The margins share the terminal background, so the user sees one terminal. Scrolling over a margin does nothing in either screen. Drag selection past the canvas edge also stops tracking (`:767`), so dragging to a line end overshoots into the margin. This breaks the `terminal-wezterm` expectation that a wheel notch "over the terminal" scrolls 3 lines. Task 5.6 only walks clicks inside the canvas. → Register the wheel handler on `entry.container`. Clamp the cell in `mouseToCell` for drags that leave the canvas sideways.

## Pass 2 — Security

- No findings. The two new keys are booleans merged by the existing `save_config` path. They stay out of `BACKEND_OWNED_CONFIG_KEYS`, which is correct.

## Pass 3 — Compliance (vs specs/**, design.md, implementation.md, project CLAUDE.md conventions)

- [Important] `src/components/terminal/TerminalManager.tsx:86-89` — The scenario "A late config still applies" has no test and no walk. No vitest covers the subscription. Tasks group 5 walks a normal restart (5.5), not a config slower than the wait. The race in Pass 1 lives exactly here, unseen. → Add a walk step that delays `get_config`, or a unit test for the refit after registration.
- [Nit] `src/components/terminal/terminalService.ts:1176-1180` — `waitForConfig` counts 60 frames, not 1 second. At 144 Hz it waits 0.42 s, which widens the Pass 1 race. A throttled `requestAnimationFrame` can exceed the spec's "at most 1 second". If `get_config` never resolves, every center spawn pays the full wait. `leader f` then stays silent for the whole run, with no toast. → Bound the wait by `performance.now()`, and toast once when the config is missing.
- [Nit] `src/App.tsx:133-138` — The new default `leader f` can drop a user override. `migrateKeymapOverrides` runs on every start and drops overrides that collide with a default. A user who bound another action to `leader f` loses it. The toast then blames "the shortcuts restructure". → Accept, or name the new binding in that toast.
- [Nit] `openspec/changes/reading-mode/proposal.md` — The proposal promises the cap for "every markdown view of the right panel". The flag is global. It also caps the floating Linear and ClickUp detail (`src/components/layout/Workspace.tsx:445`, `:448`). That detail sets no focus zone of its own. So `leader f` inside it toggles whatever zone was last set, maybe the terminal. → Say "every markdown view" in the proposal, or route the detail panel to the markdown flag.
- [Nit] `src/components/plan/MarkdownView.tsx:182-183` — Tables use `w-full` with no overflow wrapper. In a 72ch column, a table with long paths or code cells overflows past the column edge. The same holds in `src/components/plan/AnnotatableMarkdownView.tsx:121-122`. Openspec tables hit this often. → Wrap tables in an `overflow-x-auto` block, as `pre` already does.

Acceptance criteria (spec `specs/reading-mode/spec.md`):

- [x] Wide pane is capped and centered — `src/components/terminal/readingCols.ts:18-19`, `terminalService.ts:1218-1219`; test `readingCols.test.ts` row 1; canvas offset by walk 5.3.
- [x] Narrow pane keeps its width — `readingCols.ts:21`; test row 2.
- [x] Flag off uses the full pane — `readingCols.ts:21`; test row 3.
- [x] Quake ignores the flag — `readingCols.ts:18`; test row 4; walk 5.5.
- [x] A hidden session adopts the flag when shown — `terminalService.ts:327-333` fits on show; walk 5.5.
- [x] Markdown flag on caps and centers — `src/components/plan/readingWidth.ts:2`, `MarkdownView.tsx:109`, `AnnotatableMarkdownView.tsx:617`; test `readingWidth.test.ts`; walk 5.4.
- [x] Markdown flag off keeps the full width — same anchors and test.
- [x] Terminal zone flips the terminal flag — `src/stores/readingMode.ts:32-41`, `src/stores/shortcuts.ts:459`; test `readingMode.test.ts`; refit by walk 5.3.
- [x] Panel zone flips the markdown flag — same anchors; test.
- [x] Sidebar zone changes nothing — `readingMode.ts:34`; test.
- [x] A toggle before the config loads changes nothing — `readingMode.ts:34`, `App.tsx:128`; test.
- [x] Turning the terminal flag off restores the full width — `TerminalManager.tsx:87`; walk 5.3.
- [x] Missing keys read as off — `src-tauri/src/config.rs:242-245`; test `config_reading_mode_tests.rs`, `readingMode.test.ts`.
- [x] A toggle saves only its key — `readingMode.ts:16`; test.
- [x] Flags survive a restart — `terminalService.ts:356`; walk 5.5.
- [.] A late config still applies — `TerminalManager.tsx:87` exists, but it misses a pending spawn (Pass 1). No test, no walk.

## Harness entropy check

- Speculative abstractions? No. WHAT-comments? No. The Rust doc at `config.rs:241` points at an existing WHY. New deps without user confirm? No.
- Checked and clean: no import cycle (`readingMode.ts` imports only the `FocusZone` type from `shortcuts.ts`). The `appStore.sub` refit fires only when the derived boolean changes, not on unrelated config writes. Settings → Apply sends the live `configAtom`, which already holds the toggled flags, so it neither drops nor reverts them. The theme and font restyle path refits through `fit`, so it keeps the cap. `waitForConfig` never delays quake shells.

## Verdict

FAIL — The feature works on the happy path, and its pure helpers are well tested. Two defects ship, though. A flag change during a terminal spawn is lost, which breaks the spec's late-config scenario. Capped margins also swallow wheel scrolling and edge drags. The late-config scenario has no test or walk, so neither defect would surface before release.


## Frontend (sonnet)

---
type: review
feature: reading-mode
reviewer: frontend
reviewed_at: 2026-11-23T00:00:00Z
verdict: PASS_WITH_NOTES
---

# Review — reading-mode (frontend)

> Code-level review only. Rendered, screenshot and keyboard audits are skipped: the user walks the installed .deb afterwards.

## Diff scope

- Files changed: 9 modified in `src/` plus 3 new source files · LOC: about +90 / -15 (excluding tests)

## Pass 1 — Bugs

- [Important] `src/components/terminal/terminalService.ts:349` — The margins around the centered canvas have no mouse handler. Mouse listeners live on the canvas only (line 736). A click in the empty side margin does not focus the terminal, though before the change the canvas filled the container. → Add a `mousedown` handler on `entry.container` that calls `focusCanvas(entry)` when `e.target === entry.container`.
- [Important] `src/components/plan/MarkdownView.tsx:183` — The table is `w-full` with no scroll wrapper. A wide table cannot shrink below its content width inside a 72ch column. It then spills past the right edge of the column and looks off-center. Code blocks already scroll (line 139), but tables do not. → Wrap the table in `<div className="my-2 overflow-x-auto">`, in `AnnotatableMarkdownView.tsx` too.
- [Nit] `src/components/terminal/terminalService.ts:390` — The scroll indicator is anchored `right:12px` to the container, not the canvas. With reading mode on it floats in the right margin, away from the text it describes. → Anchor it to the canvas edge, or accept it and note it in the walk.
- [Nit] `src/stores/readingMode.ts:36` — The `leader f` toggle on a panel that is not markdown (Git, Browser) still flips the markdown flag and toasts "markdown on". Nothing visible changes. → Name the surface only when a markdown view is mounted, or word the toast "Reading mode: panels on".
- [Nit] `src/components/terminal/terminalService.ts:1176` — `waitForConfig` blocks center terminal launch for up to 60 frames if `get_config` fails, because only the success path sets `configLoadedAtom` (`src/App.tsx:128`). → Set the flag in a `finally`, or lower the cap.

## Pass 2 — Security

- None. The change adds no input handling, no raw HTML and no new IPC beyond the existing `save_config`.

## Pass 3 — Compliance (vs spec, docs/design.md, docs/shortcuts.md, CLAUDE.md)

- [Nit] `src/stores/shortcuts.ts:459` — The keywords `focus` and `zen` overlap with `focus-terminal` and the Zen conflict view in the command palette. A search for "zen" would surface reading mode. → Drop `zen` and `focus`; keep `reading`, `width`, `narrow`, `column`.
- [Nit] `src/stores/readingMode.ts:38` — The toast reads "Reading mode: terminal on". Sibling info toasts are sentences ("No dev servers detected", `src/stores/shortcuts.ts:453`). → "Reading mode on for the terminal" reads closer to the rest. Keep it in English, like the other toasts.
- The registry entry is consistent with `docs/shortcuts.md`. `leader f` is a leader continuation, the project's default home for new features. Label, group `surfaces` and category `navigation` match the neighbouring toggles. No shortcut collision found in `shortcuts.ts` (`leader f` is unused).
- Acceptance criteria: [.] Line length at 72ch — static reasoning only, see the next section.

## Layout notes (static reasoning)

- `72ch` resolves against the root's 12px font, so the box is about 480 to 500px wide. Tailwind uses `border-box`, so `px-4` takes 32px from that. The text column is about 450 to 470px, around 65 to 75 prose characters. That is a sensible measure.
- Linear and ClickUp comment cards sit in a side panel that is often narrower than 500px. The cap is a no-op there and `mx-auto` changes nothing. The toast still says "on". Check this in the walk.
- Plan and Spec panels are the real beneficiaries. The Plan scroll container (`PlanPanel.tsx:366`) is `relative`, so the root centers inside it. Verify during the walk that the annotation toolbar and highlights still align with the centered column.
- Images use `max-w-full`, and headings with `border-b` span the column width. Both stay aligned inside the narrow column.
- Terminal margin color: the container background (`terminalService.ts:349`, refreshed at line 170) equals the canvas background. There is no visible seam.

## Harness entropy check

- Speculative abstractions: no. WHAT-comments: none added in the reviewed diff. New deps: none.

## Verdict

PASS_WITH_NOTES — Two Important findings: the dead click in the terminal margin and the table overflow in the narrow column. Neither blocks, but both are cheap to fix before the walk. The five nits are optional. Nothing is marked 🔴.


## Orchestrator rulings

- Frontend Important 1 (margin click does not focus the terminal) is a false positive: the host div's onMouseDown calls `terminalService.focusActive()` for clicks outside the canvas (`src/components/terminal/TerminalManager.tsx:121-127`).
- Frontend Important 2 and Adversarial Nit 4 are the same finding (tables without an overflow wrapper).
