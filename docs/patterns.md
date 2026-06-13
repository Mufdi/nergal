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

## 1. Navigation tiers

Three movement granularities, consistently bound:

| Tier | Keys | Mechanism |
|------|------|-----------|
| Between modules (sidebar ↔ terminal ↔ panel) | `Alt+←/→` | `shortcuts.ts`: `focus-left` / `focus-right` |
| Within a list (rows of the focused module) | `Alt+↑/↓` | `shortcuts.ts`: `nav-up` / `nav-down` |
| Between sibling views inside a panel (chips, tabs) | `Shift+←/→` | Component-local handlers (§2) |

`Ctrl+1..9` jumps to indexed sessions; number keys `1–9` jump to indexed
items inside decision modals and pickers.

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
ClickUpPanel + ClickUpTaskDetail (`S`/`W`/`P`/`B`/`R`/`C`/`O` — send / spawn /
pin / bind / reinject / close-out / open). These are **component-local window
handlers**, not entries in `shortcuts.ts` (the binding only makes sense while
the surface is engaged).

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
Canonical: `ClickUpPanel.tsx`.

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

---

## Maintenance

When you add an interaction pattern used in 2+ places, document it here with:
the canonical implementation path, the behavior contract (what MUST hold),
and any pending extraction decision. Visual tokens stay in `design.md`.
