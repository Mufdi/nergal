# Implementation Plan: reading-mode

> Grounded in current codebase, symbols verified 2026-11-23. Behaviour (not just symbol existence) verified for the load-bearing claims below.

## Verified codebase facts (do not re-assume)

- `computeCols(container, metrics)` (`src/components/terminal/terminalService.ts:1154`) floors `rect.width / metrics.cssWidth` and falls back to 80×24 below 10×3. Its only callers are the first spawn in `showTerminal` (`terminalService.ts:408`) and `fit(entry)` (`terminalService.ts:1215`). Both need the cap, or a fresh terminal spawns at full width and only shrinks on the next fit.
- Each terminal container is created with `position:absolute;inset:0;display:flex;overflow:hidden` (`terminalService.ts:346-349`). `showTerminal` toggles `container.style.display` between `"flex"` and `"none"` (`terminalService.ts:324`), so `justify-content` survives show and hide.
- `sizeCanvas` sets the canvas CSS size to `cols × cssWidth` by `rows × cssHeight` (`terminalService.ts:1179-1190`). With a capped column count the canvas is narrower than the container, and `justify-content:center` centers it.
- Mouse mapping reads `entry.canvas.getBoundingClientRect()` (`terminalService.ts:997-998`), not the container rect, so a centered canvas keeps click and selection coordinates right.
- `Entry.region` is `"center" | "quake"` (`terminalService.ts:48`, `:55`). The cap applies only to `center`.
- `fitActive(region = "center")` (`terminalService.ts:529-536`) fits and repaints the active entry. `TerminalManager` calls it from a `ResizeObserver` (`src/components/terminal/TerminalManager.tsx:76-81`).
- `terminalService.ts` already imports `appStore` (`terminalService.ts:20`), so it can read an atom and subscribe to it outside React.
- `fit` returns early on a 0×0 container (`terminalService.ts:1213-1214`), and `sendResize` dedups through `nextResize` / `lastRequested` (`terminalService.ts:1192-1209`, from `pixelGeometry.ts`). A refit with an unchanged size sends nothing.
- Persisted boolean pattern: `linearShowCompletedAtom` (`src/stores/linear.ts:285-296`) reads `configAtom`, writes `configAtom`, and calls `invoke("save_config", { config: { <key>: next } })` with only the changed key. Rust side: `linear_show_completed: bool` with a serde default and a WHY doc (`src-tauri/src/config.rs:236-240`), its `Default` value (`config.rs:438`) and a round-trip test (`config.rs:567-578`). Frontend-owned keys must not enter `BACKEND_OWNED_CONFIG_KEYS` (`src-tauri/src/commands/config_misc.rs:21`).
- Frontend config shape: `Config` in `src/lib/types.ts:66-77` (optional fields) and the `configAtom` default in `src/stores/config.ts:12-32`.
- The config loads once at startup in `App.tsx:124-127` (`invoke<Config>("get_config")` then `setConfig`). A terminal can spawn before that, so the terminal must refit when the flag value arrives.
- Focus zone: `focusZoneAtom` (`src/stores/shortcuts.ts:64`), type `FocusZone = "sidebar" | "terminal" | "panel" | "quake"` (`shortcuts.ts:45`).
- `leader f` is unbound. Single-key leader entries live in the registry array of `src/stores/shortcuts.ts` (for example `toggle-notifications`, `leader h`, group `surfaces`, `shortcuts.ts:457`).
- Markdown roots: `MarkdownView` renders `<div className="prose-invert max-w-none px-4 py-3 text-[12px] text-text">` (`src/components/plan/MarkdownView.tsx:105`). `AnnotatableMarkdownView` renders a root with `annotatable-plan prose-invert max-w-none px-4 py-3 ...` (`src/components/plan/AnnotatableMarkdownView.tsx:614`). Every right-panel markdown surface uses one of the two: `LinearTaskView.tsx:1276,1331`, `ClickUpTaskView.tsx:1193,1282`, `SpecPanel.tsx:684,690`, `PlanPanel.tsx:367`, `ObsidianNoteView.tsx:161`.
- Tests: vitest (`pnpm test`), pure helpers sit next to their module (`src/components/terminal/pixelGeometry.test.ts`, `resizeTracker.test.ts`); store tests use a fresh Jotai store and mock `invoke` (`src/stores/linearPrefsAtoms.test.ts`).

## Execution order

1. Rust config fields and their test.
2. Frontend types, `configAtom` default, `configLoadedAtom`, and the new store `src/stores/readingMode.ts`.
3. Pure helpers: `readingLayout` (terminal), `readingModeTarget` and `toggleReadingMode` (routing), `markdownWidthClass` (markdown).
4. `terminalService.ts`: layout through `readingLayout` at both call sites, first spawn waits for the config.
5. `TerminalManager.tsx`: refit on flag change. `App.tsx`: mark the config loaded.
6. Markdown roots.
7. Shortcut entry.

## Plan

### Config (Rust)
- `src-tauri/src/config.rs`: add `reading_mode_terminal: bool` and `reading_mode_markdown: bool`, `#[serde(default)]`, one WHY doc line pointing at the same reason as `linear_hidden_states` (frontend-owned, config not localStorage). Add both to the `Default` impl as `false`. Add a round-trip test like `linear_show_completed_defaults_true_when_absent_and_round_trips`, asserting `false` when absent. `Config` has no `deny_unknown_fields` (`config.rs:119`), so a downgrade reads the file and only drops the two flags.

### Config load signal
- `src/stores/config.ts`: new `configLoadedAtom = atom(false)`. `App.tsx:125-127`: set it to `true` right after `setConfig(cfg)`. No signal like it exists today (verified: no hydration atom in `src/`).

### Store
- `src/lib/types.ts`: two optional `boolean` fields on `Config`. `src/stores/config.ts`: both `false` in the default.
- New `src/stores/readingMode.ts`: `readingTerminalAtom` and `readingMarkdownAtom`, each built like `linearShowCompletedAtom` (read `configAtom` with `?? false`, write `configAtom` and `save_config` with only its key). `readingModeTarget(zone)` maps terminal → `"terminal"`, panel → `"markdown"`, sidebar and quake → `null`. `toggleReadingMode(store, zone)` returns early when `configLoadedAtom` is false or the target is `null`; otherwise it flips the target atom and sets `toastsAtom` to `{ message: "Reading mode: <terminal|markdown> <on|off>", type: "info" }` (toast shape as in `shortcuts.ts:452`). The module imports only `type FocusZone` from `shortcuts.ts`: `shortcuts.ts:6` imports `terminalService`, and `terminalService` will import this module, so a value import would close the cycle `shortcuts → terminalService → readingMode → shortcuts`.

### Terminal
- New pure helper `src/components/terminal/readingCols.ts`: `READING_MAX_COLS = 100` and `readingLayout(width, height, metrics, region, enabled) → { cols, rows, centered }`. It absorbs `computeCols` (same floor and the same 80×24 fallback, applied before the cap), then caps the column count at 100 only for `center` with `enabled`; `centered` is true only when the cap reduced the count. `computeCols` becomes a thin wrapper over it or is replaced at both call sites (`terminalService.ts:408`, `:1215`). Both sites set `container.style.justifyContent` to `"center"` when `centered`, else `""`.
- First spawn: in `showTerminal`, before the `computeCols` call at `:408`, await the config with a bound of 1 second, like `waitForLayout` (`terminalService.ts:1171-1177`), only for `region === "center"`. Reading the flag: `appStore.get(readingTerminalAtom)` at both call sites.

### Refit on flag change
- `TerminalManager.tsx`: next to the `ResizeObserver` effect (`TerminalManager.tsx:73-81`), add `useEffect(() => appStore.sub(readingTerminalAtom, () => requestAnimationFrame(() => terminalService.fitActive())), [])`. The cleanup is the unsubscribe. `TerminalManager` mounts before `get_config` resolves, so the late-config case refits too. Keeping the subscription out of `terminalService.ts` keeps it out of the five store tests that import `shortcuts.ts` (vitest runs in node, with no `requestAnimationFrame`).

### Markdown
- New pure helper `src/components/plan/readingWidth.ts`: `markdownWidthClass(enabled)` returns `"mx-auto max-w-[100ch]"` or `"max-w-none"` (revised from 72ch, see Walk follow-ups). `MarkdownView` and `AnnotatableMarkdownView` read `readingMarkdownAtom` and put it in place of `max-w-none` on the root. The root keeps `px-4`, so the text column is a little under 100ch, within the spec's "at most 100ch". The project has no DOM test environment (`vitest.config.ts`, devDependencies hold only `vitest`), so the helper carries the test.

### Shortcut
- `src/stores/shortcuts.ts`: entry `{ id: "toggle-reading-mode", label: "Toggle Reading Mode", keys: "leader f", category: "navigation", group: "surfaces", keywords: [...], handler: () => toggleReadingMode(store(), store().get(focusZoneAtom)) }`. Do not touch `docs/shortcuts.md` (it holds an unrelated pending edit and lists no per-binding table).

## Per-phase risk

- [Claude Code caches its size at startup, so a spawn at full width then a resize to 100 redraws the banner] → the first center spawn waits up to 1 s for the config; only a config slower than that falls back to a refit.
- [A `leader f` before the config loads would be overwritten by `setConfig` while the disk keeps the new value] → the toggle is a no-op until `configLoadedAtom` is true.
- [Hidden center terminals keep their old width] → by spec they adopt the flag when shown: `showTerminal` calls `fit` when it shows an entry (`terminalService.ts:327-333`), and `fit` reads the flag at call time.
- [A flip without a visible change confuses the user] → the toast names the surface and the new state.
- [The scroll indicator sits at the container's right edge, outside a centered canvas] → accepted; it is an overlay hint, not content.
- [Reflow under existing annotation highlights in `AnnotatableMarkdownView`] → web-highlighter stores DOM ranges, and the annotation toolbar is `fixed` in viewport coordinates (`PlanAnnotationToolbar.tsx:202`); the walk checks highlights after a toggle.
- [The resize dedup drops the capped size] → `nextResize` gets a test: last request at 180 columns, next at 100, returns a payload.
- [Comment bodies in Linear and ClickUp center inside full-width cards while author and metadata stay left] → accepted as the per-view rule; the walk looks at it.

## Verification

- `cd src-tauri && cargo clippy --all-targets -- -D warnings && cargo test && cargo fmt --check`, then `npx tsc --noEmit` and `pnpm test`.
- `~/.claude/skills/work/scripts/work-gates.sh main reading-mode`.
- Manual walk on the installed `.deb` (tasks group 5).

## Review follow-ups (B5, tasks group 5)

- Pending spawn: `showTerminal` registers the entry at `terminalService.ts:481` (`entries.set(key, entry)`); `fitActive` only sees registered entries, so a flag change between the first `computeLayout` (`:412`) and `:481` is lost. Fix: remember the flag used for the first layout and refit after registration when it differs (`refitNeeded`).
- Margins: the `wheel` listener is on the canvas (`terminalService.ts:904`) and the drag `mousemove` too (`:767`); `mouseToCell` (`:1000-1009`) returns `null` outside the canvas. Fix: listen on `entry.container` for both; drag uses a pure `clampedCell` (new file `src/components/terminal/cellMapping.ts`), clicks keep `mouseToCell`. The window `mouseup` (`:145`, `:872`) already ends a drag released outside.
- Config wait: `waitForConfig` counts 60 frames; replace with a pure `waitUntil(pred, timeoutMs, deps)` (new file `src/components/terminal/waitUntil.ts`, `deps = { now, nextFrame }`) bounded by 1000 ms of `performance.now()`.
- Tables: `table` is `my-2 w-full border-collapse text-xs` in `MarkdownView.tsx:182-183` and `AnnotatableMarkdownView.tsx:121-122`. Auto table layout already shrinks columns to the container; only unbroken tokens overflow. Fix: `[overflow-wrap:anywhere]` on `th` and `td`. The user ruled out horizontal scroll.

## Walk follow-ups (user walk 2026-11-23)

- Width: at `text-[12px]` a 72ch column is about 430 px and reads too narrow. The cap becomes `100ch`, the terminal's column count (`READING_MAX_COLS`, `src/components/terminal/readingCols.ts`).
- Tables span the full view in both states. Both roots render the markdown elements as direct children (`MarkdownView.tsx:109`, `AnnotatableMarkdownView.tsx:615-619`, `PlanMarkdown` adds no wrapper at `:44-50`), and the `table` renderers are `MarkdownView.tsx:182-183` and `AnnotatableMarkdownView.tsx:121-122`. Fix: wrap each root in an outer `<div className="@container">` (Tailwind v4 `container-type: inline-size`), and give the table `w-[calc(100cqw_-_2rem)] ml-[calc(50%_-_50cqw_+_1rem)]` in place of `w-full`. With the column centered in the container and `px-4` on the root, the margin resolves to `1rem` from the container left edge whatever the column width, and to `0` when the flag is off, so one class works for both states.
- Rejected: capping direct children with `[&>:not(table)]:mx-auto` (it overrides the `ml-4` list indent at `MarkdownView.tsx:127,130` and `AnnotatableMarkdownView.tsx:75,78`, and `ch` resolves per child font, so headings get a wider cap); a grid full-bleed root (grid items do not collapse margins, so the vertical rhythm changes only in reading mode).
- The `containerRef` (`AnnotatableMarkdownView.tsx:616`) and the `annotatable-plan` class stay on the inner root, so highlight queries (`:547`) and the toolbar keep their element.
- Container queries: no prior use in `src/`. The host has WebKitGTK 2.50 (`libwebkit2gtk-4.1-0`), which supports `container-type` and `cqw`.
- Open as tab for a Linear issue and the plan panel `A` key are separate issues, not part of this change.

## Full-width tables setting (user walk 2026-11-23)

- The user asked for a switch for terminal and panel. The terminal half is not possible: the PTY has one column count, and Claude Code picks a grid or a vertical list for each table from it. The user chose a panel-only switch.
- Config: `reading_mode_full_width_tables`, `#[serde(default = "default_true")]` (`default_true` at `src-tauri/src/config.rs:6`), next to the reading flags (`config.rs:243-245`, `Default` at `:444-445`). Frontend-owned like them. Non-optional in `src/lib/types.ts` because `BooleanConfigKey` (`SettingsPanel.tsx:1363-1365`) only accepts keys whose type is exactly `boolean`; `sidebar_dot_grid` (`types.ts:49`) is the model.
- Settings: `AppearanceSection` (`SettingsPanel.tsx:1835`) edits a draft through `handleToggleChange` (`:3283-3285`), and Save sends the whole draft (`:3291-3292`). The switch copies the `sidebar_dot_grid` block (`:2051-2063`). Search keywords at `:2148`.
- `readingFullWidthTablesAtom` is read-only: Settings owns the writes.
- Rendering: react-markdown renderers are inline in a memoized `PlanMarkdown` (`AnnotatableMarkdownView.tsx:44-50`) and in `MarkdownView` (`MarkdownView.tsx:~185`). A hook inside an inline renderer would run in a new component type on each parent render, so both renderers return a shared `ReadingTable` component (new file `src/components/plan/ReadingTable.tsx`) that reads the atom and applies `markdownTableClass`.
