# cluihud — Interaction Patterns

Canonical interaction patterns: keyboard navigation, selection flows, and
reusable UI behaviors. Visual styling (colors, typography, surfaces, motion)
lives in [`design.md`](./design.md) — this file covers *how things behave*,
that one covers *how things look*. Read both before adding UI.

Every keybinding listed here must exist in `src/stores/shortcuts.ts` (the
registry) or be a documented component-local handler (§2). Shortcuts always
match on `event.code`, never `event.key` (WebKitGTK Linux bug). **Before
adding any binding, check `shortcuts.ts` for collisions** — they break flows
silently.

---

## 1. Shortcut taxonomy

### 1.1 The scoping principle

**The modifier a shortcut needs is decided by where focus lives.** The terminal
(the agent PTY) consumes bare keystrokes, so any shortcut that must fire *while
the terminal is focused* — i.e. a **global** — needs a modifier. A focus zone
that is NOT the terminal (panel, sidebar, quake, a modal, a floating module)
does not forward bare letters to the agent, so **bare letters are free there**
for contextual verbs. This is what lets us recycle the same letter (or even the
same combo) across surfaces.

That splits every binding into two families and three scoping levels:

| Level | Where it fires | Mechanism | Examples |
|-------|----------------|-----------|----------|
| **Global / transversal** | anywhere, incl. terminal focused | `shortcuts.ts` registry (always modified) | session switch, open panels, ship, push |
| **Modal capture** | while a `Dialog` is mounted | global dispatcher bails (`dialogOpen` guard in `useKeyboardShortcuts.ts`); the modal owns ALL keys | Ship, Merge, AgentPicker, AskUser, confirms |
| **Surface override** | while a panel / floating module owns focus | component-local handlers (§8) + selective combo recycling; true globals still pass through | ClickUp verbs, plan-review verbs, scratchpad tabs |

The difference between *modal capture* and *surface override*: a `Dialog` is a
**modal task you must finish or dismiss**, so it captures everything. A panel or
`FloatingPanel` is a **non-modal companion**, so it overrides only the keys it
claims and lets the few global/transversal shortcuts keep working.

### 1.2 Family A — global shortcuts (always modified)

Registered in `shortcuts.ts`. Pick the tier by intent:

| Tier | Keys | Meaning | Rule |
|------|------|---------|------|
| 0 | `Ctrl+{key}` | Global + OS-mirroring | if the OS / a browser / an editor already binds it, match them (`Ctrl+1..9` sessions, `Ctrl+B` sidebar, `Ctrl+K` palette, `Ctrl+,` settings, `Ctrl+S` save, `Ctrl+W` close, `Ctrl+N` new, `Ctrl+Enter` fullscreen, `Ctrl+Tab` cycle) |
| 1 | `Ctrl+Shift+{letter}` | **Open / toggle a PANEL** (nouns / surfaces) | default for any panel: plan, files, diff, spec, git, clickup, obsidian, activity, annotations drawer, file-picker |
| 2 | `Ctrl+Alt+{letter}` | **ACTION verb that mutates state** | push, ship, rename-branch, clear-tasks, complete-merge, quick-capture, vault-search |
| — | `Ctrl+Shift+{1-9}` · `Ctrl+Alt+{1-9}` | numeric variants | session-in-focused-workspace / jump-to-project |

`Ctrl+Shift+{letter}` is nearly saturated (`u` is reserved by IBus). When a
panel's natural letter collides, it falls to `Ctrl+Alt+` (browser `Ctrl+Alt+B`,
scratchpad `Ctrl+Alt+L`) — a documented exception, not a tier violation.

### 1.3 Family B — bare-letter verbs (surface-scoped)

Single letters, no modifier, that act on the focused surface's current item.
This is §8 — see there for the full guard contract. Used by ClickUp
(`S/W/P/B/R/C/O/T`), Conflicts (`O/T`), PrViewer (`A`), and plan/spec review
(`A` approve · `R` revise · `C` comment · `X` clear, swal-confirmed). Bare-letter
verbs are **NOT** registry entries; their affordance is the tooltip advertising
the letter (§6). **Entering** an engaged state keeps a modifier (annotation mode
is `Ctrl+Shift+H`) because it's triggered from *outside* the engaged surface;
once inside, the verbs go bare.

### 1.4 Navigation tiers

Three movement granularities, consistently bound:

| Tier | Keys | Mechanism |
|------|------|-----------|
| Between modules (sidebar ↔ terminal ↔ panel) | `Alt+←/→` | `shortcuts.ts`: `focus-left` / `focus-right` |
| Within a list (rows of the focused module) | `Alt+↑/↓` | `shortcuts.ts`: `nav-up` / `nav-down` |
| Between sibling views inside a panel (chips, tabs) | `Shift+←/→` | Component-local handlers (§2) |

`Ctrl+1..9` jumps to indexed sessions; number keys `1–9` jump to indexed
items inside decision modals and pickers.

### 1.5 Candidates for bare-letter migration

When you next enter these surfaces for another reason, evaluate moving their
modified actions to bare letters per §1.3: the **Git panel** (`S` stage, `P`
push, `X` discard — commit stays modified, it needs a message). Don't do it as
an isolated change.

### 1.6 User keymap overrides (Settings → Keymap)

Family A shortcuts are **remappable per user**. Defaults live in
`shortcutRegistryAtom`; user overrides live in `config.keymap_overrides`
(`{ shortcutId → keys }`, persisted in `config.json`). The merge happens once,
in `resolvedShortcutsAtom` — **every consumer reads the resolved atom, never the
raw registry**: the dispatcher (`useKeyboardShortcuts`) matches on it and the
command palette renders it. Add a new shortcut consumer the same way.

- **Locked ids** (`LOCKED_SHORTCUT_IDS` in `lib/keymap.ts`): `command-palette`,
  `focus-terminal`, `session-1..9`. Structural bindings; overrides are ignored
  even if hand-edited into `config.json`, and the editor shows a lock.
- **Capture + validation** (`lib/keymap.ts`): `eventToKeys` turns a live event
  into a registry keys string (`event.code`, not `key`); `validateCombo`
  enforces a Ctrl/Alt modifier (bare/Shift-only would swallow terminal typing),
  rejects OS-reserved combos (IBus `Ctrl+Shift+U`), and blocks collisions
  against the full effective keymap (locked included). Collision = warn + block,
  never silent reassign.
- **Capture guard**: while recording, `keymapCaptureActiveAtom` is set and all
  global keyboard consumers (the dispatcher + the Settings dialog nav handlers)
  bail so the keystroke reaches only the recorder. Set it the same way for any
  future press-to-record surface.
- **Special-cased shortcuts** keep working: `Ctrl+}` (quake) keeps its
  layout-dependent dual key/code match **only at its default** — once overridden,
  the dual-match is skipped and the registry loop resolves the new combo. The
  context-local terminal-emulator overrides (scratchpad/quake `Ctrl+Tab/W/T`,
  Tab routing) are **not** registry entries and are out of remap scope.

Out of scope for v1: bare-letter §8 verbs are not remappable.

## 2. Chip-strip navigation

A horizontal strip of pill chips switches sibling views inside one panel.
Canonical implementation: `src/components/git/chips/ChipStrip.tsx` (GitPanel:
Files / History / Stashes / PRs / Conflicts). Other instances:
`src/components/spec/SpecPanel.tsx:389` (spec tabs) and
`src/components/clickup/ClickUpPanel.tsx` (`VIEW_ORDER`: My tasks / Status /
List / Assignee — `Shift+←/→` cycles, handler at `:175`).

Behavior contract:

- `Shift+←/→` cycles chips, wrapping at both ends.
- The handler is **component-local** (window `keydown` listener mounted with
  the panel), not in the `shortcuts.ts` registry — the binding only makes
  sense while the panel is visible.
- The handler MUST ignore events originating in editable fields: `INPUT`,
  `TEXTAREA`, `.cm-editor` (CodeMirror), `[contenteditable]`. Copy the guard
  from `ChipStrip.tsx:30-36`.
- Chips show an inline count badge when their view has items; "hot" states
  (e.g. conflicts > 0) may override colors per `design.md` §1.2.
- Each chip's `title` advertises the binding: `` `${label} (Shift+←/→)` ``.

**Decision — `useChipStripNav` hook (2026-06-06, updated 2026-06-13)**: the
Shift+arrow handler is now duplicated in **three** places (ChipStrip, SpecPanel,
ClickUpPanel, ~15 lines each). The "extract on the third instance" trigger has
fired — **extract `useChipStripNav(items, active, onSelect)`** into `src/hooks/`
and migrate all three. Deferred as accepted tech-debt (logged here); pick it up
in a refactor pass, not mid-feature.

## 3. File picker

`Ctrl+Shift+K` toggles a panel-scoped fuzzy file picker
(`toggle-file-picker` in `src/stores/rightPanel.ts`, `filePickerOpenAtom`).
Popover-as-modal surface (`design.md` §3.9), `max-w-xs`, centered. Selecting
a file opens it as a document tab.

Use this pattern (not a native dialog) whenever the user picks a *file inside
the workspace* for an in-app action.

## 4. Placeholder keymap hints

Inputs whose primary affordance is a keyboard flow embed the keymap in the
placeholder, e.g. the command palette (`Type a command…  ↑↓ navigate · Enter
to run`) and decision-modal footers (`↑↓ navigate • Enter to select • Esc to
cancel`). Rules:

- Hints live in placeholder/footer text at `text-[10px] text-muted-foreground`
  — never as always-visible labels next to the input.
- Use the glyphs `↑↓ ← →` plus key names; `Kbd` chips are for buttons and
  tooltips, not placeholders.

## 5. Keyboard navigation primitives

cluihud is keyboard-first. Two systems collaborate (moved here from
`design.md` §5 — behavior, not styling):

### 5.1 Focus zones

The Workspace divides the UI into **zones** (sidebar, terminal, panel,
quake). Each zone is marked with `data-focus-zone="<name>"` on a focusable
container. Keystroke handlers in zones use
`e.currentTarget.querySelector("[data-nav-item]")` to locate items.

`zone-flash` (`design.md` §4) plays when the focus zone changes, giving
visual confirmation.

The `quake` zone (the drop-down shell overlay) is deliberately **excluded
from the `alt+left/right` cycle** — it's an overlay, reached only via its
own shortcut (`Ctrl+}`, which cycles hidden→open+focus →focus →hide). It
keeps the accent-border focus convention of the other zones.

### 5.2 Nav-item attributes

Inside any zone, list items participate in arrow-key navigation by setting:

| Attribute | Required? | Purpose |
|-----------|-----------|---------|
| `data-nav-item` | yes | Marks the element as navigable |
| `data-nav-selected` | runtime | Set/cleared by handler; styles via CSS rule in `globals.css` |
| `data-nav-expanded` | tree only | `"true"` / `"false"` for arrow-left/right collapse |
| `data-nav-chevron` | tree only | If present, arrow-left/right delegates to this child |

**The CSS rule that visualizes selection** is in `globals.css` (search for
`[data-nav-selected]`):
```
background: var(--secondary) !important;
color: var(--foreground) !important;
```

### 5.3 Modal-specific keyboard

Decision-list modals (Resume, ProjectPicker) bind their own `keydown` at
`window` level with **capture: true** — base-ui Dialog tries to intercept
Enter/Escape and we override that. They also focus a `tabIndex={0}` div with
`focus:ring-1 focus:ring-orange-500/50` so the user sees which list is active.

Every modal must respond to `Esc`; decision modals must respond to `↑↓+Enter`
and (where ≤9 options) `1–9`.

### 5.4 Terminal interception

The canvas terminal intercepts `keydown` at the React-pane layer **before**
forwarding input to wezterm-term. cluihud shortcuts swallow the event there;
anything unclaimed reaches the agent TUI. Adding a global shortcut therefore
changes what the terminal *doesn't* receive — another reason collisions in
`shortcuts.ts` must be checked first.

### 5.5 Index-cursor over heterogeneous controls

When a surface mixes *different kinds* of controls — a colored status chip, a
date trigger, list rows, a textarea — rather than uniform list rows, use an
**index cursor** instead of real per-element DOM focus. Canonical:
`src/components/session/AgentPickerModal.tsx` (the new-session modal: agent
cards + launch-option rows + env shells). Second instance:
`src/components/clickup/ClickUpTaskDetail.tsx` (status / due / subtasks /
checklist / comment); the closure dialog (`ClickUpClosureDialog.tsx`) uses the
same cursor over its status chips + comment.

Behavior contract:

- **One focus on the container** (`tabIndex={0}`, `outline-none`), never on the
  individual items. Putting DOM focus on each element shows per-element focus
  rings — the wrong convention here (and the card itself must carry
  `outline-none` so it shows no focus outline).
- **Highlight = background, not a ring.** Plain rows highlight via the
  `[data-nav-selected]` CSS rule (§5.2 — `bg-secondary`). Elements that carry
  their *own* background (a colored status chip) can't show a bg highlight, so
  use a subtle `ring-1 ring-foreground/50` — the same affordance as
  AgentPickerModal's selected agent card border. Never an accent/orange ring
  per element.
- **The cursor is state, not DOM focus** → the scroll container won't follow
  it. Manually `scrollIntoView({ block: "nearest" })` the selected element when
  the cursor moves.
- **DOM order is the source of truth.** Read the ordered `[data-nav-key]` (or
  `[data-nav-item]`) nodes from the container and **filter to visible**
  (`offsetParent !== null`) — `display:none` hover-only buttons (e.g. an
  assignee remove button) otherwise stall the cursor on an unfocusable node.
- **Mouse keeps the cursor live.** Clicking an item moves the cursor to it and
  refocuses the container (WebKitGTK doesn't focus buttons on click), so arrows
  resume — except when clicking into a textarea/input, which keeps its focus.
- A **textarea owns its own arrows**; `Esc` inside it backs out to the
  container. A nested popup (§7) owns the arrows entirely while open.

---

## 6. Immediate tooltips on action icons

Icon-only action buttons (panel toolbars, list-row actions, chip strips) use
the `Tooltip` component under a `<TooltipProvider delay={0}>` — instant on
hover, **not** the OS-delayed native `title`. Canonical: `TopBar.tsx`; reused
in `ClickUpPanel.tsx` (`RowAction`) and `ClickUpTaskDetail.tsx`
(`ToolbarAction`).

Contract:

- Wrap the action cluster in `<TooltipProvider delay={0}>`; each button is a
  `Tooltip` → `TooltipTrigger render={<button … />}` → `TooltipContent`.
- The tooltip text advertises the one-letter shortcut when one exists, e.g.
  `"Close out task (C) — mark done & unbind"`, `"Assigned to me (A)"`.
- Reserve native `title` for non-interactive affordances or surfaces where no
  `TooltipProvider` is mounted.

## 7. Nested popups own Escape

A `FloatingPanel` closes on `Esc` only when **no nested popup is open inside
it**. An in-panel dropdown/popover (status picker, date picker) must mark its
open surface with `data-floating-popup`; `FloatingPanel`'s Escape handler skips
the close when `card.querySelector("[data-floating-popup]")` is present, and the
popup's own capture-phase Escape handler closes itself. Without this, `Esc`
tears down the whole panel instead of the dropdown the user meant to dismiss.
The same attribute gates the click-to-refocus logic (§5.5) so clicking a
popup's ephemeral options doesn't move the index cursor.

The same marker also covers **in-card editing fields**: a description/comment
textarea sets `data-floating-popup` while editing/focused so `Esc` cancels the
edit (and hands the cursor back to its nav element) instead of closing the whole
panel — the FloatingPanel's window-capture Escape fires before the textarea's
own bubble handler otherwise, so the field can't intercept it without this.

## 8. Bare-letter verbs scoped to a focus zone

A focused panel/surface exposes its actions as **single bare letters** (no
modifier) that act on the current item, scoped to that zone so they don't fire
globally. Precedents: ConflictsPanel (`O`/`T`), PrViewer (`A`),
ClickUpPanel + ClickUpTaskDetail (`S`/`W`/`P`/`B`/`R`/`C`/`O`/`T` — send / spawn /
pin / bind / reinject / close-out / open / open-as-tab). These are
**component-local window handlers**, not entries in `shortcuts.ts` (the binding
only makes sense while the surface is engaged).

Contract:

- Guard hard before acting: bail on any modifier (`ctrl/alt/meta/shift`), bail
  inside editable fields (`INPUT`, `TEXTAREA`, `.cm-editor`, `[contenteditable]`),
  and bail unless the event target is inside the owning
  `data-focus-zone` (`target.closest("[data-focus-zone='…']")`). Copy the guard
  from `ClickUpTaskDetail.tsx`'s verb handler.
- Resolve the target item the same way the surface shows its cursor: the open
  floating detail's task wins; otherwise the panel's `data-nav-selected` row
  (`[data-focus-zone='…'] [data-nav-selected][data-task-id]`).
- **Only `preventDefault()` when the verb actually acts** — conditional verbs
  (reinject needs a live target, close-out needs the bound task) must let the
  key through when they don't apply, so they don't silently swallow it.
- Every verb's affordance advertises its letter in the tooltip (§6), e.g.
  `"Send as prompt (S)"`. Pick letters that don't collide within the zone
  (`event.code`, per the top-of-file rule) — when two zones overlap (the detail
  mounts inside the clickup zone), the panel's own letter toggles
  (e.g. show-closed) must vacate the letter the verb claims (show-closed moved
  `C → H` to free `C` for close-out).

**Where surface-override handlers live (the §1.1 "surface override" level):** the
canonical home is a **component-local window handler** in the surface itself —
ClickUp detail, ConflictsPanel, PlanPanel/SpecPanel all do this, so the keys
mount/unmount with the surface and the guard reads the live DOM. A non-modal
`FloatingPanel` follows the same rule: override only the keys it claims (the
scratchpad recycles `Ctrl+Tab`/`Ctrl+W`/`Ctrl+T` for its tabs) and let global
shortcuts pass. **Deferred tech-debt**: scratchpad and quake currently host their
combo overrides as hardcoded blocks *inside* `useKeyboardShortcuts.ts` rather
than component-local. It behaves correctly (non-modal, selective) but the
location is inconsistent — migrate to component-local when you next touch those
components, not as an isolated refactor (keyboard-routing changes carry
regression risk for zero behavior change). QuickCapture (single textarea) and
MentionPicker (presentation-only overlay; keys owned by the host field's hook)
need nothing — they already match the model.

## 9. Expandable index-cursor trees

A list whose items nest (ClickUp subtasks, recursive) renders each row with a
**leading chevron slot**: a chevron when the item has children, else a
same-width spacer so every row's content stays column-aligned. Indentation is
`paddingLeft: base + depth * 16`. Canonical: `ClickUpPanel.tsx` (`renderNode`).

- **Default state differs by surface**: the panel defaults **collapsed**
  (`expandedTaskIds`, absent = closed); the detail modal renders the tree
  **always expanded** (it doesn't share the collapse-set).
- **Space** toggles the keyboard-cursor row when it has a subtree
  (`data-has-subtree`); the chevron toggles on click (`stopPropagation`). An
  expand/collapse-**all** affordance flips the whole set (header button + `E`),
  refocusing the panel root afterwards so the bare-letter shortcuts keep firing.
- Children come from a **global parent→children map** over the full (unfiltered)
  pool, so a visible parent shows ALL its children even when a filter
  (assigned-to-me) would exclude them; a `seen` set guards malformed cycles.

## 10. Header-action row keyboard nav

A panel header with a focusable control (a `Select`) followed by a row of icon
buttons (sort / filters / toggles) is reachable without a mouse: from the
control, **→ enters the button row**; **←/→ move along it**; **← from the first
returns to the control**. Mark each button `data-header-action`; a capture-phase
window handler (beats the Select's own keys + the list-cursor nav) drives it and
**skips disabled buttons** (`:not([disabled])`). The list-nav handler
early-returns when focus sits on a `data-header-action` button so native
Enter/Space activate it. A button that disables itself on click (e.g. a
reset-to-default) must refocus the panel root so the zone keeps its shortcuts.
Canonical: `ClickUpPanel.tsx` + `LinearPanel.tsx`.

**Focus must stay visible during this nav.** The buttons are advanced by
programmatic `.focus()`, and WebKitGTK draws the native focus outline as a
**rectangle that ignores `border-radius`** — illegible on a 24px rounded icon
button, so the keyboard cursor looks "lost". The fix is global, not per-button:
`[data-header-action]:focus` in `globals.css` pins the same box-shadow ring as
`.cluihud-focus-ring`. Any new toolbar button gets it for free by carrying the
`data-header-action` marker — don't rely on the native outline.

### 10.1 Keyboard-navigable multi-select popover (label/tag filter)

A header-action button that opens a **multi-select** popover (Linear's label
filter) must be fully operable from the keyboard, mirroring the `Select`
mechanics (§3.8) the user already knows from the ClickUp status picker:

- The trigger is reachable via the §10 row; **↓ on the trigger opens** the
  popover, and an effect moves focus to the first option on open.
- The popup is `role="listbox"` (`aria-multiselectable`); options are
  `role="option"` `aria-selected` with **`tabIndex={-1}`** (roving focus driven
  by `.focus()`, not Tab order).
- A keydown handler on the listbox: **↑/↓** move the focused option (clamped at
  the ends), **Space/Enter** toggle it via the option's native `onClick` (no
  close — it's multi-select), **Esc** closes and returns focus to the trigger.
- Options highlight on `:focus` with `focus:bg-accent` (matches the `Select`
  popup's `data-highlighted`), NOT the rounded-button ring.
- The panel's list-nav and §10 ←/→ handlers already early-return when focus sits
  inside `[role='listbox']`, so they don't fight the popover.

Canonical: the label filter in `LinearPanel.tsx`.

## 11. View panels join the right-panel tab cycle

A standalone **"tool" view panel** (ClickUp / git / diff / browser — the view
*is* the content) shows as a **virtual tab** in the `TabBar` and takes a slot in
the `Ctrl+Tab` cycle after the document tabs. Landing on it clears the active
document tab (`activeTabId = null`) so `RightPanel` falls through to render
`activePanelView`. **"document" launcher panels** (spec / file / plan — lists
that *open* document tabs) are gated out (`PANEL_CATEGORY_MAP[view] === "tool"`):
no virtual tab, no cycle slot — otherwise the launcher list surfaces as a
confusing ghost slot. The virtual tab closes via its X, middle-click, or
`Ctrl+W`. Canonical: `TabBar.tsx` + `cyclePanelTab` / `closeCurrentTab` in
`shortcuts.ts`.

## 12. Copyable identifiers

A short identifier shown in a row (the ClickUp task id) is copyable two ways:
**click** the id (a `span role="button"` with `stopPropagation` so the row's own
click doesn't fire) and **`Ctrl+C`** while the keyboard cursor is on the row
(read from a `data-task-copy-id` attribute; handled before the modifier-bail,
skipped inside editable fields). Both route through one store action that writes
via the robust `terminal_clipboard_write` command (the plugin's Wayland backend
stalls async tasks) and raises a confirmation toast. In a detail surface the id
is also a first-class nav element (`data-nav-key`) that copies on Enter/click.

## 13. Detail content as both a floating modal and a document tab

A rich detail view (the ClickUp task) renders identically in two shells from one
**shared controller hook + a layout-parametrized body**, so neither shell
duplicates the fetch / optimistic-write / drill-in / cursor logic.
`useClickUpTaskController({taskId, setTaskId})` owns all of it; the consumer
supplies the controlled task: the **modal** passes the atom
(`clickupDetailTaskIdAtom`), the **tab** passes local state (so its drill-in
history is independent of the modal). `ClickUpTaskBody({c, layout})` renders the
same `data-nav-key` cursor elements in DOM order (properties lead so the cursor
starts at status/due) — two columns for `layout="modal"`, one column for
`layout="tab"`. Canonical: `ClickUpTaskView.tsx` (controller + body + shared
header pieces) + `ClickUpTaskTab.tsx` + `ClickUpTaskDetail.tsx`.

**The state-driven index cursor.** Actionable elements carry `data-nav-key`
(`status`/`due`/`sub:<id>`/`att:<id>`/`rel:<id>`/`copyid`/`open`/…) and highlight
via `data-nav-selected` (styled globally under any `[data-focus-zone]`). The
container (focused, `tabIndex={0}`) owns `onKeyDown` (↑/↓ walk `navKeysInOrder()`
= visible `[data-nav-key]` in DOM order; Enter/Space → `activateNav`) and
`onClick` (move cursor to the clicked element + refocus the container, since
WebKitGTK doesn't focus buttons on click). The cursor is **state, not DOM
focus** — buttons never steal focus during arrow-nav, so Enter doesn't
double-fire. `LinearTaskView.tsx` mirrors this exactly (read-only: nav keys are
`copyid`/`open`/`sub:`/`att:`/`rel:`, no status/due editing). To make a new
element reachable, give it a `data-nav-key` + `data-nav-selected` and a branch in
`activateNav` — nothing else.

Contract:

- **Surface-specific stays with each consumer.** The modal keeps FloatingPanel
  geometry, the close-focus restore, and its verb / `Ctrl+←/→` window listeners.
  The tab scopes its own verb + history listeners to its **wrapper subtree**
  (`wrapperRef.current.contains(target)`) so multiple open task tabs don't all
  fire.
- **The focusable container must persist across loading → loaded** (don't early-
  return a different element) or the index cursor loses its single focus. Render
  the placeholder *inside* the always-mounted container.
- **Convert-to-tab** is the `T` verb / a toolbar button. From the floating detail
  it closes the modal — and sets a `suppressCloseFocusRef` so the close-focus
  restore does NOT yank focus back to the panel; the new tab keeps it. From a
  panel row it just opens (nothing to close).
- **Dual-zone gotcha:** both the panel root *and* the floating detail carry
  `data-focus-zone="clickup"`, so "did this fire from the modal?" is
  `inClickupZone && !inPanelZone` (the floating detail mounts outside the right
  panel's `panel` zone). Don't key it on `inClickupZone` alone.
- The tab type is a **non-singleton document tab** (`clickup-task`, id
  `clickup-task:<taskId>`): multiple tasks coexist, reopening one reactivates it,
  in-memory like every tab (survives session-switch, not restart).
- **SWR detail cache** (module-level `Map` in the controller): revisiting a task
  renders the cached detail **immediately** (no loader, no reload flash) then
  revalidates silently; a loader only shows on a true cold load. A
  `detail → cache` effect keeps it fresh so optimistic writes persist across
  revisits.

## 14. Loading indicators: ProgressBar (regions) vs PulseDots (inline)

Two indeterminate indicators; **never a raw lucide `Loader2` spinner**.

- **Region / panel / list loaders** → `<ProgressBar>` (a slim indeterminate
  sliver) with the descriptive text, centered: `flex flex-col items-center
  justify-center gap-2 px-6` with the bar `className="max-w-32"`. Canonical:
  `ProgressBar.tsx`.
- **Inline action-in-progress** (a button mid-action, an icon slot, a status
  marker) → `<PulseDots>` (dots pulsing in a wave; `bg-current` so they inherit
  the surrounding text tone). Label form: `Posting <PulseDots/>`; icon-slot /
  status form: `<PulseDots count={1} dotClassName="size-…"/>`. Canonical:
  `PulseDots.tsx` + the `cluihud-dot-pulse` keyframes in `globals.css`.

Rule of thumb: if it fills a region while content loads → ProgressBar; if it
sits inline next to (or replaces) a glyph while an action runs → PulseDots.

---

## Maintenance

When you add an interaction pattern used in 2+ places, document it here with:
the canonical implementation path, the behavior contract (what MUST hold),
and any pending extraction decision. Visual tokens stay in `design.md`.
