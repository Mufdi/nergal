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

---

## Maintenance

When you add an interaction pattern used in 2+ places, document it here with:
the canonical implementation path, the behavior contract (what MUST hold),
and any pending extraction decision. Visual tokens stay in `design.md`.
