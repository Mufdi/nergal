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
Files / History / Stashes / PRs / Conflicts). Second instance:
`src/components/spec/SpecPanel.tsx:389` (spec tabs).

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

**Decision — `useChipStripNav` hook (2026-06-06)**: the Shift+arrow handler
is duplicated in ChipStrip and SpecPanel (~15 lines each). Accepted as-is:
two copies don't justify the indirection. **If a third chip-strip appears,
extract `useChipStripNav(items, active, onSelect)`** into `src/hooks/` and
migrate all three.

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

The Workspace divides the UI into **zones** (sidebar, terminal, panel). Each
zone is marked with `data-focus-zone="<name>"` on a focusable container.
Keystroke handlers in zones use
`e.currentTarget.querySelector("[data-nav-item]")` to locate items.

`zone-flash` (`design.md` §4) plays when the focus zone changes, giving
visual confirmation.

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

---

## Maintenance

When you add an interaction pattern used in 2+ places, document it here with:
the canonical implementation path, the behavior contract (what MUST hold),
and any pending extraction decision. Visual tokens stay in `design.md`.
