# cluihud — Design System

> **Identity**: Linear dark + factory.ai orange + JetBrains Air islands.
> A keyboard-first desktop wrapper for Claude Code. Dense, IDE-like, dark-only. Surfaces nest in three tiers; orange (`#f97316`) is reserved for affirmative state and the user's path of action.

This file is the source of truth for new components. Read it before adding UI.
The runtime tokens live in `src/styles/globals.css`; this document explains
*why* they exist and *when* to use them.

---

## 1. Foundations

### 1.1 Color tokens

All semantic colors are defined as CSS custom properties in `globals.css`.
**Reach for the semantic name first**; use Tailwind palette colors only for
status/encoding (see §1.2).

| Token | Value | Use |
|-------|-------|-----|
| `--background` | `#141415` | Root canvas. Workspace flex container. |
| `--card` | `#0a0a0b` | **Islands**: panels, sidebar cards, top bar, status bar, modal surface, dropdowns, terminal background. |
| `--popover` | `#1c1c1e` | Reserved (currently aliased to secondary; tooltip uses raw `#1c1c1e`). |
| `--secondary` / `--muted` | `#1c1c1e` | Hover/active fills inside islands; segmented-tab background. |
| `--secondary-foreground` | `#a0a0a3` | Secondary text on secondary surface. |
| `--muted-foreground` | `#5c5c5f` | Tertiary text, hints, meta. |
| `--foreground` | `#ededef` | Primary text. |
| `--primary` / `--accent` / `--ring` | `#f97316` | Affirmative accent. **One purpose**: signal "this is your active path / chosen option / focus target". |
| `--primary-foreground` | `#0a0a0b` | Text on saturated orange. |
| `--destructive` | `#ef4444` | Errors, destructive actions. **Subtle by default** (`bg-destructive/10`). |
| `--border` | `rgba(255,255,255,0.08)` | Card outlines, hairline separators. |
| `--input` | `rgba(255,255,255,0.12)` | Form-control border (slightly more visible than `--border`). |
| `--radius` | `0.375rem` | Base unit; all `--radius-*` derive from it. |

### 1.2 Encoded colors (hardcoded Tailwind)

These are **semantically meaningful** beyond "active/passive" and shouldn't be
swapped for `--primary`. Keep encoding consistent.

| Color | Encoding | Examples |
|-------|----------|----------|
| `orange-500` (= primary) | The user's chosen option, attention, dirty state | RadioCard selected, dirty branch dot, attention indicator |
| `green-500` / `green-400` | Success, additions, working state | Lines added, working spinner, ship button, replace/insert annotation |
| `red-500` / `red-400` / `red-300` | Conflicts, deletions, close, errors | Conflict badge, lines removed, close button, delete annotation |
| `yellow-500` / `yellow-400` (`#eab308` / `#facc15`) | Warnings, pending annotations, file-modified | Highlighter mark, pinpoint outline, rate-limit threshold |
| `sky-400` | "Thinking" / active not-yet-busy | Session in active mode |
| `blue-400` (`#60a5fa`) | Comment annotation, tool-use activity | Annotation type "comment" |
| `amber-400` | Buddy "shiny" variant, minimize button | Cosmetic accent |

**Rule**: Don't introduce a new encoded color without adding it here. If the
state is just "user's choice" or "current focus", use `--primary` instead.

### 1.3 Typography

Variable Geist is loaded via `@fontsource-variable/geist` and aliased to
`--font-sans`. Mono only inside `Kbd` and the `BuddyWidget` sprite.

| Class | Px | Use |
|-------|----|-----|
| `text-base` | 16 | Modal title, dialog headings, default body in some forms. |
| `text-sm` | 14 | Default body, form inputs, primary buttons, palette input, panel placeholders. |
| `text-xs` | 12 | Task subjects, palette commands, activity entries, icon-button tooltips. |
| `text-[11px]` | 11 | Sidebar rows, status bar text, session tabs, dense list items. |
| `text-[10px]` | 10 | Hints, meta info, footer help text, sidebar shortcut numbers. |
| `text-[9px]` | 9 | Kbd glyphs, BuddyWidget sprite, ultra-dense meta. |

**Section caps** (used everywhere for "heading-as-divider"):
```
text-[10px] font-medium uppercase tracking-wider text-muted-foreground
```
Used in: Workspaces header, Tasks header, palette categories, AskUserModal
question headers.

**Numbers** that align in columns (counts, line diffs, ages, shortcut numbers):
add `tabular-nums`.

### 1.4 Radius scale

```
--radius-sm  = 0.225rem   (3.6px)  — kbd, palette key chips
--radius-md  = 0.300rem   (4.8px)  — small buttons (xs/sm), session indicator dots
--radius-lg  = 0.375rem   (6px)    — default: buttons, inputs, cards
--radius-xl  = 0.525rem   (8.4px)  — modal content (rounded-xl)
--radius-4xl = 0.975rem  (15.6px)  — Badge (pill)
```

**Rule**: Use `rounded` (default lg) for islands and panels. Use `rounded-xl`
only for modal surfaces. Use `rounded-4xl` only for badges/pills.

### 1.5 Spacing & sizing primitives

| Token | Use |
|-------|-----|
| `gap-0.5` | Stacked icon buttons (collapsed sidebar, right panel) |
| `gap-1` | **Inter-island gap** (panels, drawers stacked vertically) |
| `gap-1.5` | Tight inline groups (icon + label) |
| `gap-2` | Standard horizontal grouping |
| `gap-3` | Form field stacks, modal content blocks |
| `p-1` | Outer Workspace padding — creates the 1-px gutter between islands |
| `px-2.5 py-1` | Default input/button padding |
| `px-3 py-1.5` | List rows, palette items |
| `px-3 py-2` | Modal content blocks |

**Icon-button size tiers** (always include `aria-label`):
- `size-4` — collapsed-rail trigger (sidebar/right panel)
- `size-5` — inline utility in section headers
- `size-7` — default top-bar buttons, panel actions
- `size-8` — default Button primitive
- `size-9` — lg Button primitive

**Icon (svg) size tiers** inside buttons:
- `size-2.5` (10px) — inline hover-revealed actions inside list rows
- `size-3` (12px) — status bar icons, sidebar panel icons
- `size-3.5` (14px) — close button in drawer header
- `size-4` (16px) — default Button icon

---

## 2. Surface system

cluihud has **three** background tiers. New surfaces must fit into one of them.

```
┌─ bg-background  #141415 ──────────────────────────────────┐
│  Root canvas. Visible only as the 1-px gutter between     │
│  islands (because of p-1 on the panel group).             │
│                                                            │
│  ┌─ bg-card  #0a0a0b ─────────────────────────────────┐   │
│  │  ISLAND. Panels, sidebar cards, top bar, status   │   │
│  │  bar, modal surface, dropdowns, terminal.          │   │
│  │                                                     │   │
│  │  ┌─ bg-secondary  #1c1c1e ────────────────────┐   │   │
│  │  │  Hover/active fill inside islands.         │   │   │
│  │  │  Segmented-tab background.                 │   │   │
│  │  └─────────────────────────────────────────────┘   │   │
│  └─────────────────────────────────────────────────────┘   │
└────────────────────────────────────────────────────────────┘
```

### 2.1 Island anatomy

```tsx
<div className="flex flex-1 flex-col overflow-hidden rounded bg-card">
  {/* Header (optional) */}
  <div className="flex h-8 shrink-0 items-center px-3 border-b border-border/50">
    <span className="text-[11px] font-medium text-foreground/80">{title}</span>
  </div>
  {/* Body */}
  <div className="flex-1 overflow-y-auto">…</div>
</div>
```

**Rules**:
- Always `rounded bg-card overflow-hidden` for the outer.
- Header height is `h-8` or `h-9`. Use `h-9` only when the island is a
  primary container (Workspaces card).
- Header dividers are always `border-b border-border/50` — never `border-b border-border`
  (full-strength border is too heavy on `bg-card`).
- Stacked islands use `gap-1` between them.

### 2.2 Layout grid

Master layout (`Workspace.tsx`) is a 3-panel `ResizablePanelGroup` with
**`p-1` outer padding** — this is what produces the JetBrains Air "floating
islands" feel. **Do not remove the `p-1`**.

| Panel | Default | Min | Max | Collapsed |
|-------|---------|-----|-----|-----------|
| Sidebar | 15% | 6% | 22% | 32 px |
| Center (Terminal + ActivityDrawer) | 42% | 25% | — | — |
| Right (TabBar + content) | 43% | 15% | 65% | 28 px |

The center column nests vertically with `gap-1` (terminal island +
ActivityDrawer island).

---

## 3. Component patterns

### 3.1 Buttons (`@/components/ui/button.tsx`)

Built on `@base-ui/react/button` + `cva`. Six variants × eight sizes. Always
prefer the primitive over hand-rolling.

**Variants**:
- `default` — orange primary, for the **single** affirmative action in a flow.
- `outline` — bordered, neutral; for cancel / secondary actions.
- `secondary` — `bg-secondary` filled; for non-primary toolbar buttons.
- `ghost` — transparent; for icon-only buttons in dense bars.
- `destructive` — `bg-destructive/10` (subtle, never solid); deletes/discards.
- `link` — text-only, primary color, underline-on-hover.

**Sizes**: `xs` (h-6) · `sm` (h-7) · `default` (h-8) · `lg` (h-9) ·
`icon` / `icon-xs` / `icon-sm` / `icon-lg`.

**Focus ring is universal**: `focus-visible:border-ring focus-visible:ring-3
focus-visible:ring-ring/50`. Don't override it.

**`aria-expanded:bg-muted`** is built into `outline`, `secondary`, and `ghost`
variants — when the button opens a popover/dropdown, it stays visually
"pressed".

### 3.2 Hand-rolled icon buttons (when not using `Button`)

Three places use raw `<button>` for icon-only chrome (TopBar, status bar,
panel headers). Stick to this template:

```tsx
<button
  onClick={…}
  className="flex size-7 items-center justify-center rounded
             text-muted-foreground hover:bg-card/50 hover:text-foreground
             transition-colors"
  aria-label={…}
>
  <Icon size={14} />
</button>
```

Active state: `text-foreground bg-card` (or `bg-secondary` if the parent is
already `bg-card`).

### 3.3 Inputs (`@/components/ui/input.tsx`, `textarea.tsx`)

Built on `@base-ui/react/input`. `h-8`, `bg-transparent`, `border-input`,
`rounded-lg`. Same focus ring as Button. Textarea uses `field-sizing-content`
to auto-grow.

**Inline-edit input** (used in SessionRow rename, sidebar new-session):
```
h-5 bg-transparent border-b border-border text-[11px] outline-none
```
This is the **typeform-style** edit input — only a bottom border, no rounded
box. Use it when an existing label transitions to editable in place.

### 3.4 Kbd (`@/components/ui/kbd.tsx`)

OS-aware (Mac glyphs `⌃⇧⌥⌘` vs `Ctrl+Shift+Alt+Super`). Two tones:
- `subtle` (default) — `border-border/40 bg-background/40 text-muted-foreground/70`
- `onPrimary` — for chips inside a saturated orange button.

`h-4 px-1 font-mono text-[9px]`. Always pair with the actual keybinding string
from `stores/shortcuts.ts` so the registry is the single source of truth.

The `CommandPalette` renders its own KeyBadges inline — they should migrate
to `Kbd` (open follow-up).

### 3.5 Badges (`@/components/ui/badge.tsx`)

Pill-shaped (`rounded-4xl`) `h-5 px-2 text-xs`. Six variants matching Button.
Use for:
- Mode chip in StatusBar (`secondary`, h-4 customization)
- Inline status tags

Don't use for actions (use Button instead) or for status dots (use a
`size-1.5 rounded-full` span — see §3.10).

### 3.6 Tabs (`@/components/ui/tabs.tsx`)

Built on `@base-ui/react/tabs`. Two visual variants:
- `default` — segmented control with `bg-muted` track.
- `line` — bottom-underline accent (`after:bottom-[-5px] h-0.5 bg-foreground`).

Use `default` for in-panel section switching. Use `line` for primary
navigation that must feel attached to its content.

The custom **`TabBar`** (`src/components/ui/TabBar.tsx`) is for the right
panel's open documents — **not** a substitute for `Tabs`. It supports
drag-reorder, conflict pinning, dirty indicator (`size-1 rounded-full
bg-foreground` replaces close button until hover), and middle-click close.

### 3.7 Tooltips (`@/components/ui/tooltip.tsx`)

`@base-ui/react/tooltip` with `delay={1000}`. Surface uses **raw**
`bg-[#1c1c1e] text-[#ededef] border border-white/10` instead of semantic
tokens — this is intentional (the popup needs to stay legible regardless of
parent surface). Animations: `fade-in-0 zoom-in-95` open/close.

**Rule**: Every icon-only button gets a tooltip. The tooltip should be the
source of truth for the action label + shortcut: `Commit (Ctrl+Shift+C)`.

### 3.8 Modal / Dialog (`@/components/ui/dialog.tsx`)

Built on `@base-ui/react/dialog`. The canonical surface:

| Layer | Class |
|-------|-------|
| Overlay | `fixed inset-0 isolate z-50 bg-black/10 supports-backdrop-filter:backdrop-blur-xs` |
| Surface | `rounded-xl bg-background p-4 ring-1 ring-foreground/10 max-w-[calc(100%-2rem)] sm:max-w-sm` |
| Title | `text-base leading-none font-medium` |
| Description | `text-sm text-muted-foreground` |
| Footer | `-mx-4 -mb-4 bg-muted/50 border-t p-4 (col-reverse → row sm)` |
| Close | `XIcon` inside `Button variant="ghost" size="icon-sm"` top-right |

**Motion**: `fade-in-0 zoom-in-95` open / `fade-out-0 zoom-out-95` close, 100 ms.

**Two modal flavors are in use**:

1. **Form modal** (CommitModal, SettingsPanel): Title + content + Footer with
   `Cancel` (outline) and primary action (default). Footer is **always**
   right-aligned on `sm:` and column-reverse on mobile.
2. **Decision-list modal** (ResumeModal, ProjectPickerModal): No footer.
   Each option is a card-button (see §3.13). Footer text gives keyboard hints:
   `↑↓ navigate • Enter to select • Esc to cancel`.

`AskUserModal` is a hybrid — uses a footer for `Send` but the questions
are radio-cards (§3.13).

**Sizing**: Default `sm:max-w-sm`. AskUserModal inherits default (questions
scroll inside `max-h-[60vh] overflow-y-auto`). SettingsPanel uses `sm:max-w-md`.

### 3.9 Command palette / file picker (popover-as-modal)

The `CommandPalette` and `FilePickerOverlay` aren't built on `Dialog`. Their
own pattern:

| Layer | Class |
|-------|-------|
| Overlay | `fixed inset-0 z-50 backdrop-blur-sm bg-black/30` (palette) or `absolute inset-0 z-30 backdrop-blur-sm bg-black/20` (panel-scoped picker) |
| Surface | `rounded-lg border border-border bg-card shadow-2xl max-h-[70vh]` |
| Width | `max-w-lg` (palette) / `max-w-xs` (file picker) |
| Position | `pt-[20vh]` (palette pinned high) / centered (picker) |

The blur is **stronger** than modals (`backdrop-blur-sm`, `bg-black/30`)
because the palette appears on top of an active context — the user is jumping
*through* the UI, not pausing it.

### 3.10 Status dots & progress

**Solid dot** (binary state) — `size-1.5 shrink-0 rounded-full bg-{color}`.
Use for: dirty branch (orange-500), conflict (red-500 + animate-pulse),
session attention.

**Animated dot** — three custom animations live in `globals.css`:

| Class | Use | Animation |
|-------|-----|-----------|
| `animate-session-thinking` | Active mode (sky-400) | Opacity pulse, 1.6 s |
| `animate-session-working` | Tool running (green-500) | Orbital ring, 0.8 s |
| `animate-session-attention` | Awaiting user (orange-500) | Ripple ping, 1.2 s |
| `animate-pulse` (Tailwind) | Conflict urgency (red-500) | Standard pulse |

Always wrap inside `SessionIndicator` rather than recreating animations inline.
All three respect `prefers-reduced-motion`.

**Progress bar** (StatusBar context window):
`h-2 w-12 rounded-full bg-muted` track + `h-full rounded-full transition-all`
fill. Color is **threshold-encoded**: ≥90% red, ≥70% yellow, else primary.

### 3.11 Drawers (ActivityDrawer, AnnotationsDrawer)

Drawer ≠ modal. Drawer = a horizontal island stacked with `gap-1` below the
main panel. Same `rounded bg-card`, but with `style={{ maxHeight: "30vh" }}`
to bound it.

Header: `h-8` with `border-b border-border` (full strength here, not /50,
because the drawer needs more visual separation from its content). Right side
holds inline actions: `Open as Tab` (text + ExternalLink icon), close (X).

### 3.12 Lists & nav rows

**Sidebar/sessions row pattern**:
```tsx
<div
  data-nav-item
  data-nav-expanded={…}
  className={`group flex w-full items-center gap-1.5 px-3 py-1
              hover:bg-secondary/40 transition-colors ${
                isActive
                  ? "bg-secondary/60 text-foreground
                     shadow-[inset_2px_0_0_0_var(--color-primary)]"
                  : "text-foreground/70"
              }`}
>
```

**Active-row treatment** is the cluihud signature: an **inset 2-px orange
left bar** via `box-shadow`, not a full background highlight. This is the
"current row in a list of peers" pattern.

**Hover-revealed actions**: wrap actions in `hidden group-hover:flex` and
hide the meta info on hover with `group-hover:hidden`. Used in SessionRow.

### 3.13 Choice cards (radio-as-button)

When a modal asks the user to pick one of N discrete options (Resume mode,
project picker, AskUser options, commit language):

```tsx
<button
  className={`rounded-md border px-3 py-1.5 text-[11px] transition-colors ${
    selected
      ? "border-orange-500 bg-orange-500/10 text-foreground"
      : "border-border bg-card text-muted-foreground hover:text-foreground"
  }`}
>
```

**Rule**: Use **hardcoded `orange-500`** here (not `--primary`) because the
state needs maximum contrast against the modal background.

This is the **second selection pattern** in the system. The first is the
inset-bar active row (§3.12). They serve different cases:
- **Inset bar** → "this is the current/displayed item in a permanent list"
- **Orange-500 outlined card** → "this is the option you've picked but haven't
  committed yet"

### 3.14 Annotation marks (`globals.css`)

The plan-editor highlighter uses semantic colors via `mark.annotation-{type}`:

| Type | Color | When |
|------|-------|------|
| pending (default) | yellow `#facc15` | Selection-in-progress |
| comment | blue-400 `#60a5fa` | Inline comment |
| replace | green-400 `#34d399` | Suggested replacement |
| insert | green-400 `#4ade80` | Suggested addition |
| delete | red-500 `#ef4444` + line-through | Deletion |

Pinpoint hover/active states are dashed outlines — `outline: 1px dashed
var(--primary)` / `outline: 2px dashed #eab308`.

### 3.15 Window controls (TopBar)

Three traffic-light buttons, **color-encoded by destructiveness**:
- Minimize → `text-amber-500/60 hover:bg-amber-500/10 hover:text-amber-400`
- Maximize → `text-green-500/60 hover:bg-green-500/10 hover:text-green-400`
- Close → `text-red-500/60 hover:bg-red-500/10 hover:text-red-400`

All `size-7`. Don't extend this pattern to other buttons — it's reserved for
window chrome.

---

## 4. Motion vocabulary

| Class / pattern | Speed | Purpose |
|-----------------|-------|---------|
| `transition-colors` | default | Most hover/active state changes |
| `transition-all` | default | Buttons (broader: bg + ring + opacity) |
| `transition-opacity` | default | Hover-revealed actions |
| `transition-transform` | default | Chevron rotation |
| Dialog open/close | 100 ms | `fade + zoom-95` |
| Tooltip open/close | 150 ms (base-ui default) | `fade + zoom-95` |
| `session-thinking` | 1.6 s ∞ | Opacity pulse |
| `session-working` | 0.8 s ∞ | Orbital ring |
| `session-attention` | 1.2 s ∞ | Ripple |
| `zone-flash` | 0.8 s 1× | Inset ring on focus-zone change |
| `animate-spin` (Tailwind) | 1 s ∞ | Loader2 only |
| `animate-pulse` (Tailwind) | 2 s ∞ | Conflict urgency |

**Rules**:
- Never use `transition-all` with non-color properties without testing — it
  catches `transform` and `box-shadow` and can stutter on resize.
- All custom keyframe animations must include a
  `@media (prefers-reduced-motion: reduce)` opt-out.
- Don't add new infinite animations without a clear semantic role; the four
  above already encode "thinking / working / attention / urgency".

---

## 5. Keyboard navigation primitives

cluihud is keyboard-first. Two systems collaborate:

### 5.1 Focus zones

The Workspace divides the UI into **zones** (sidebar, terminal, panel). Each
zone is marked with `data-focus-zone="<name>"` on a focusable container.
Keystroke handlers in zones use `e.currentTarget.querySelector("[data-nav-item]")`
to locate items.

`zone-flash` (§4) plays when the focus zone changes, giving visual confirmation.

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

Number-key shortcuts (`1`–`9`) jump to indexed items in pickers and sidebar.

---

## 6. Decision rules (when to use what)

### 6.1 Surface choice

| Need | Use |
|------|-----|
| New top-level region of the workspace | new `ResizablePanel` + `bg-card` island |
| Container inside an existing island | no new surface — just spacing & dividers |
| Modal that takes a decision | `Dialog` from `@/components/ui/dialog` |
| Floating tool that doesn't pause work | popover-as-modal pattern (§3.9) |
| Drawer attached to a panel | sibling island with `gap-1`, `maxHeight` style |

### 6.2 Selection / active state

| Case | Pattern | Color |
|------|---------|-------|
| Currently displayed row in a permanent list | Inset 2px left bar | `var(--color-primary)` |
| Currently active panel button in a row of equals | `text-foreground bg-card` (subtle elevation) | — |
| User is picking from N discrete options (modal) | RadioCard | `border-orange-500 bg-orange-500/10` |
| Form input focused | Focus ring | `ring-ring/50` |
| Menu item hovered/highlighted | `bg-secondary` or `bg-secondary/50` | — |

### 6.3 Color choice

1. Need a **state encoded with semantic meaning** (success, error, warning,
   pending, working)? Use the encoded Tailwind color from §1.2.
2. Need to mark "the user's path" (chosen option, attention, dirty, focus
   target)? Use `--primary` (or `orange-500` if you need higher contrast on a
   modal surface).
3. Need a fill/border for chrome that has no semantic state? Use semantic
   tokens (`muted`, `secondary`, `border`).

### 6.4 Density & sizing

- A new **panel header** is `h-8` or `h-9`, `px-3`, body in `text-[11px]`.
- A new **modal** uses default sizing (`sm:max-w-sm`); only widen if the
  content genuinely needs columns.
- A new **button in a dense bar** is `size-7` icon, `text-muted-foreground`
  default, `text-foreground` active.
- A new **list row** is `px-3 py-1` (sidebar) or `px-3 py-1.5` (palette/menu).

---

## 7. Open decisions & gaps

These are unresolved or inconsistent and must be addressed *as a system* the
next time the relevant component is added or revised. Don't add ad-hoc fixes.

### 7.1 No `Select` component

`SettingsPanel.tsx:108` and `git/ShipDialog.tsx:692` use the native HTML
`<select>`. There's no shadcn-style primitive yet.

**Recommended path** (when next select is needed): build
`src/components/ui/select.tsx` on `@base-ui/react/select`, mirroring the
`dialog.tsx` structure (`data-slot`, base-ui primitive + Tailwind tokens).
Surface should use the popover pattern (§3.9): `rounded-md border border-border
bg-card shadow-lg`. Item active state: `bg-secondary text-foreground`.

### 7.2 No shared `DropdownMenu`

Two ad-hoc dropdowns exist:
- TopBar editor picker (`absolute right-0 top-full mt-1 w-44 rounded border bg-card py-1 shadow-lg`)
- TabBar overflow menu (`w-48 rounded-md border bg-card py-1 shadow-lg`)

Both use a `<div className="fixed inset-0 z-40">` to capture outside clicks.
Promote to `src/components/ui/dropdown-menu.tsx` on `@base-ui/react/menu` next
time a third dropdown is needed.

### 7.3 No shared `Toast` system

Currently using `sileo` (`Toaster` + `toastsAtom`) — it works but isn't part
of the documented system. Either codify `sileo` as the official toast pattern
in this file (with surface tokens listed) or migrate to a custom wrapper. Pick
one before adding the next toast use-case.

### 7.4 Empty-state size inconsistency

`text-xs`, `text-sm`, and `text-[11px]` are all used for "No X" empty states
across the codebase (TaskPanel, RightPanel, Sidebar, ActivityDrawer).

**Standard** (apply going forward): `text-xs text-muted-foreground` centered
in a `flex h-full items-center justify-center` parent. Update existing
inconsistencies opportunistically, not as a refactor PR.

### 7.5 Modal vs popover surface inconsistency

- `Dialog`: `bg-background ring-1 ring-foreground/10 rounded-xl`
- `CommandPalette`, `FilePickerOverlay`, dropdowns: `bg-card border border-border rounded-{md,lg}`

These are intentionally different (`Dialog` is a true modal pause; popovers
are navigational), but the ring vs border distinction is weak. **Rule going
forward**: paused-state surface uses `ring-1 ring-foreground/10`; navigational
surface uses `border border-border`.

### 7.6 `KeyBadges` in CommandPalette duplicates `Kbd`

`CommandPalette.tsx:192` reimplements OS-aware keyboard chips. Migrate to
`Kbd` next time the palette is touched, so OS detection lives in one place.

### 7.7 Status-dot color inconsistencies

Active session dot uses `sky-400`, but tool-use activity uses `bg-blue-500`.
Both are "blue states". Pick one (`sky-400` recommended, since it ties to the
canonical `session-thinking` state) and update the activity timeline.

---

## 8. Anti-patterns

Avoid these. They're things the codebase already gets right and we want to
keep right.

- **No new background colors.** Three tiers is the system.
- **No solid `bg-destructive`.** Always use `bg-destructive/10`/`/20`.
- **No emoji in UI text** (the codebase has none today). Status uses
  iconography + color, not emoji.
- **No icons larger than `size-4` in chrome.** If you need bigger,
  reconsider — chrome should be dense.
- **No `border-radius` larger than `rounded-xl`** except for badges
  (`rounded-4xl` pill).
- **No new `transition-all`** on layout-affecting properties (transform,
  height, width). It causes resize stutter; use targeted transitions.
- **No hardcoded primary color.** Use `var(--color-primary)` or `text-primary`
  / `bg-primary` — except in modal RadioCards where contrast-on-dialog
  requires `orange-500` (§3.13).
- **No new infinite animations** without a semantic role (§4).
- **No modals without keyboard navigation.** Every modal must respond to
  Esc; decision modals must respond to ↑↓+Enter and (where ≤9 options) 1–9.
- **No icon-only buttons without `aria-label`.** Tooltip is recommended too,
  but the aria-label is the floor.

---

## 9. Stack reference

Component primitives and their role in the system:

| Library | Used for | File |
|---------|----------|------|
| `@base-ui/react` | All headless interactive primitives | dialog/tabs/tooltip/scroll-area/separator/input/button/badge |
| `tailwindcss@4` + `tw-animate-css` | Styling, animations | `globals.css` |
| `class-variance-authority` | Variant systems for Button/Badge/Tabs | inside each `ui/*.tsx` |
| `react-resizable-panels` | Workspace 3-panel layout | `ui/resizable.tsx` |
| `lucide-react` | All iconography | inline imports |
| `@fontsource-variable/geist` | Typeface | `globals.css` |
| `sileo` | Toasts (see §7.3) | `Workspace.tsx` |
| `web-highlighter` | Plan annotations | `plan/AnnotatableMarkdownView.tsx` |

When adding a new primitive type, prefer `@base-ui/react` first — every
existing primitive is built on it, and consistency in headless behavior
(focus management, portal rendering, data-state attributes) matters more than
a marginally better API in another library.

---

## Maintenance

When you add or significantly change a component:
1. If it follows an existing pattern in §3 — no DESIGN.md change needed.
2. If it introduces a new primitive (e.g. fills the §7.1 Select gap) — move
   the entry from §7 (Open) to §3 (Component patterns) with the actual tokens.
3. If it introduces a new color, animation, or surface tier — update §1, §4,
   or §2 *and* note why the new addition was necessary.

Keep this file under ~700 lines. If a section needs more depth, link to a
focused sub-doc rather than expanding here.
