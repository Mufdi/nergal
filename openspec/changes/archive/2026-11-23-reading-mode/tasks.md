## 1. Persisted reading flags

- [x] 1.1 Test (Red): Rust round-trip test in `src-tauri/src/config.rs` — a config without `reading_mode_terminal` and `reading_mode_markdown` deserializes both as `false`, and `true` values round-trip. Vitest `src/stores/readingMode.test.ts` — with a fresh store, both atoms read `false` from a config without the keys; setting `readingTerminalAtom` to `true` calls `save_config` with exactly `{ config: { reading_mode_terminal: true } }`; the markdown atom saves only `reading_mode_markdown`.
- [x] 1.2 Add both fields to `Config` in `src-tauri/src/config.rs` (`serde(default)`, `false` in `Default`), to `Config` in `src/lib/types.ts` and to the `configAtom` default in `src/stores/config.ts`. Add `configLoadedAtom` to `src/stores/config.ts` and set it in `App.tsx` right after `setConfig(cfg)`.
- [x] 1.3 Create `src/stores/readingMode.ts` with `readingTerminalAtom` and `readingMarkdownAtom` following `linearShowCompletedAtom`, importing only `type FocusZone` from `shortcuts.ts`.

## 2. Focus-routed toggle

- [x] 2.1 Test (Red): `readingModeTarget` maps `terminal` → `"terminal"`, `panel` → `"markdown"`, `sidebar` and `quake` → `null`. `toggleReadingMode(store, zone)` with `configLoadedAtom` true: zone `terminal` flips only the terminal flag and sets the toast `Reading mode: terminal on`; zone `panel` flips only the markdown flag with `Reading mode: markdown on`; a second flip reads `off`; zone `sidebar` flips nothing and sets no toast. With `configLoadedAtom` false, any zone flips nothing and `save_config` is not called. The registry holds `toggle-reading-mode` on `leader f` in group `surfaces`, and no other entry uses `leader f`.
- [x] 2.2 Implement `readingModeTarget` and `toggleReadingMode` in `src/stores/readingMode.ts`.
- [x] 2.3 Add the `toggle-reading-mode` entry to the registry in `src/stores/shortcuts.ts`, passing `store().get(focusZoneAtom)`.

## 3. Terminal column cap

- [x] 3.1 Test (Red): `src/components/terminal/readingCols.test.ts` — with 8×16 px cells: `readingLayout(1440, 800, m, "center", true)` is `{ cols: 100, rows: 50, centered: true }`; `readingLayout(640, 800, m, "center", true)` is `{ cols: 80, rows: 50, centered: false }`; `readingLayout(1440, 800, m, "center", false)` is `{ cols: 180, rows: 50, centered: false }`; `readingLayout(1440, 800, m, "quake", true)` is `{ cols: 180, rows: 50, centered: false }`; a 40 px-wide pane falls back to 80 columns before the cap. In `pixelGeometry.test.ts` (or a new file): `nextResize` with a last request at 180 columns and a next size of 100 returns a non-null payload with `cols: 100`.
- [x] 3.2 Create `src/components/terminal/readingCols.ts` with `READING_MAX_COLS` and `readingLayout`.
- [x] 3.3 In `terminalService.ts`, use `readingLayout` with `appStore.get(readingTerminalAtom)` at both call sites (`:408`, `:1215`), set `container.style.justifyContent` from `centered`, and make the first `center` spawn wait up to 1 s for `configLoadedAtom`.
- [x] 3.4 In `TerminalManager.tsx`, subscribe to `readingTerminalAtom` in a `useEffect` next to the `ResizeObserver` and refit the active `center` terminal on each change.

## 4. Markdown width cap

- [x] 4.1 Test (Red): `src/components/plan/readingWidth.test.ts` — `markdownWidthClass(true)` holds `max-w-[100ch]` and `mx-auto` and not `max-w-none`; `markdownWidthClass(false)` is `max-w-none`; `MARKDOWN_CONTAINER_CLASS` is `@container`; `MARKDOWN_TABLE_CLASS` holds `w-[calc(100cqw_-_2rem)]` and `ml-[calc(50%_-_50cqw_+_1rem)]` and not `w-full`. (The project has no DOM test environment, so the class choice is a pure helper the two components share.)
- [x] 4.2 In `src/components/plan/readingWidth.ts`, set the reading cap to `100ch` and export `MARKDOWN_CONTAINER_CLASS` and `MARKDOWN_TABLE_CLASS`. In `MarkdownView` and `AnnotatableMarkdownView`, wrap the root in a `<div className={MARKDOWN_CONTAINER_CLASS}>` and give the `table` element `MARKDOWN_TABLE_CLASS`.

## 5. Review follow-ups (B5, confirmed by the user 2026-11-23)

- [x] 5.1 Test (Red): `src/components/terminal/cellMapping.test.ts` — `clampedCell(x, y, cellW, cellH, cols, rows)` with 8×16 cells and a 100×50 grid maps (-40, 100) to `{col: 0, row: 6}`, (900, 100) to `{col: 99, row: 6}`, (400, -10) to `{col: 50, row: 0}`, (400, 900) to `{col: 50, row: 49}`, and (12, 20) to `{col: 1, row: 1}`. `src/components/terminal/waitUntil.test.ts` — `waitUntil(pred, timeoutMs, deps)` with an injected clock and frame function resolves as soon as `pred` is true, and resolves after 1000 ms of clock time when `pred` stays false, whatever the frame count; `refitNeeded(usedFlag, currentFlag)` is true only when they differ.
- [x] 5.2 Pending spawn: in `showTerminal`, after `entries.set(key, entry)`, compare the terminal flag used for the first layout with `appStore.get(readingTerminalAtom)` and call `fit(entry)` when `refitNeeded` says so.
- [x] 5.3 Margins: attach the `wheel` listener to `entry.container` instead of `entry.canvas`, and handle the drag-selection `mousemove` on the container with `clampedCell`, so the margins scroll and a drag past the canvas side keeps extending the selection to the edge column. Clicks outside the canvas still map to no cell.
- [x] 5.4 `waitForConfig` uses `waitUntil` with a 1000 ms bound measured by `performance.now()`, not a frame count.
- [x] 5.5 Tables fit the column: `th` and `td` in `MarkdownView.tsx` and `AnnotatableMarkdownView.tsx` get `[overflow-wrap:anywhere]`, so long unbroken tokens wrap and a table never grows past the markdown column. No horizontal scroll (user decision).

## 6. Full-width tables setting (user walk 2026-11-23)

- [x] 6.1 Test (Red): Rust test in `src-tauri/src/config_reading_mode_tests.rs` — a config without `reading_mode_full_width_tables` deserializes it as `true`, and `false` round-trips. `src/stores/readingMode.test.ts` — `readingFullWidthTablesAtom` reads `true` from a config without the key and `false` when the config holds `false`. `src/components/plan/readingWidth.test.ts` — `markdownTableClass(true)` holds `w-[calc(100cqw_-_2rem)]` and `ml-[calc(50%_-_50cqw_+_1rem)]` and not `w-full`; `markdownTableClass(false)` holds `w-full` and neither breakout class; both hold `border-collapse`.
- [x] 6.2 Add `reading_mode_full_width_tables: bool` to `Config` in `src-tauri/src/config.rs` (`serde(default = "default_true")`, `true` in `Default`), a non-optional `boolean` to `Config` in `src/lib/types.ts`, and `true` to the `configAtom` default.
- [x] 6.3 Add the read-only `readingFullWidthTablesAtom` to `src/stores/readingMode.ts`. Replace `MARKDOWN_TABLE_CLASS` with `markdownTableClass(fullWidth)` in `readingWidth.ts`, and render tables in both views through a shared `ReadingTable` component that reads the atom.
- [x] 6.4 Add the switch `Full-width tables in reading mode` to `AppearanceSection` in `src/components/settings/SettingsPanel.tsx`, after the existing switches, and add `reading table` to the appearance search keywords.

## 7. Verification

- [x] 7.1 `cd src-tauri && cargo clippy --all-targets -- -D warnings && cargo test && cargo fmt --check`, then `npx tsc --noEmit` and `pnpm test`.
- [x] 7.2 `~/.claude/skills/work/scripts/work-gates.sh main reading-mode` passes.
- [x] 7.3 Manual walk: with focus in the main terminal on a wide window, `leader f` shows the toast `Reading mode: terminal on` and narrows the terminal to 100 columns, centered; Claude Code rewraps at that width; `leader f` again restores the full width with nothing cut.
- [x] 7.4 Manual walk: with focus in the right panel, `leader f` shows the toast `Reading mode: markdown on` and centers the text in a column of at most 100 characters on a Plan, a Spec, an Obsidian note, a Linear issue (description and comments) and a ClickUp task (description and comments); a second `leader f` restores the full width; plan annotations stay on their text after a toggle.
- [x] 7.5 Manual walk: with both flags on, restart Nergal; the terminal opens capped with no full-width banner first and the markdown opens narrow. Another session's terminal, hidden during the toggle, shows capped when you switch to it. The quake terminal stays full width with the terminal flag on.
- [x] 7.6 Manual walk: with the terminal capped, click and drag-select text in the terminal; the selection matches the cells under the mouse.
- [x] 7.7 Manual walk: with the terminal capped, the mouse wheel over the empty margins scrolls the terminal, and a drag selection that leaves the canvas sideways keeps extending to the edge column. A markdown table with a long URL in a cell spans the full panel width, 1rem from each side, with the URL wrapped and no horizontal scroll, in reading mode on and off.
- [x] 7.8 Manual walk: in Settings > Appearance, turn `Full-width tables in reading mode` off and Save; with the markdown reading flag on, a table stays within the 100ch column; turn it back on and the table spans the panel again.
