## Context

The sidebar currently renders session status as a 6px colored dot (`size-1.5`) with a single CSS animation (`animate-dot-pulse`) for the "running" state. All other states are static dots with different colors. The data to derive granular state already exists: `modeMapAtom` tracks per-session mode (idle, active, tool names), `planReviewStatusMapAtom` tracks plan review state, and `askUserAtom` tracks pending questions. This data is unused in the sidebar — the dot only checks `session.status` (a coarser 4-value enum).

Current implementation:
- `SessionRow.tsx:88`: `<span className="size-1.5 rounded-full ${color} ${pulse}">`
- `Sidebar.tsx:165` (CollapsedSidebar): Same pattern
- `globals.css:211-218`: Single `dot-pulse` keyframe (scale 1→1.8 + opacity)
- Collapsed sidebar width: hardcoded at ~24px (fits a 6px dot + padding)

The `adaptive-layout` spec references sidebar "auto-collapsed to 40px" in the `tool-workspace` preset. The collapsed sidebar width increase to ~32-36px stays within that bound.

## Goals / Non-Goals

**Goals:**
- Each session state is visually distinct via geometry + color + animation
- User can scan the sidebar and know which sessions need attention, which are working, and which are idle — without clicking
- Animations are GPU-friendly (transform + opacity only, no layout thrash)
- Works at both expanded SessionRow size and collapsed sidebar size
- Collapsed sidebar grows just enough for legible animations

**Non-Goals:**
- Per-tool animations (e.g., different animation for Read vs Edit vs Bash)
- Sound or haptic feedback
- User-customizable shapes or colors
- Animation preferences (reduce-motion is respected via `prefers-reduced-motion` media query)

## Decisions

### 1. Shape morphing via CSS `border-radius` + `transform`

**Decision**: Use `border-radius` transitions to morph between shapes, and `transform: rotate()` for diamond/rotation effects. All shapes are a single `<span>` element — no SVG, no canvas.

| State | Shape | CSS |
|-------|-------|-----|
| Idle | Circle | `border-radius: 50%` |
| Thinking | Squircle (morphing) | `border-radius: 50%` ↔ `border-radius: 30%` animated |
| Working | Square (rotating) | `border-radius: 15%`, `transform: rotate()` animated |
| Attention | Diamond | `border-radius: 15%`, `transform: rotate(45deg)` + bounce |

**Why CSS-only**: All properties are GPU-compositable (`transform`, `opacity`, `border-radius`). No JS animation loop needed. CSS keyframes are declared once and reused. The `tw-animate-css` package is already in the project but isn't needed — vanilla `@keyframes` is simpler and more precise for custom shapes.

**Alternatives considered**:
- *SVG shapes*: More geometric precision but heavier DOM, harder to animate smoothly at 6-8px, and overkill for 4 shapes. Rejected.
- *Canvas/WebGL*: Maximum flexibility but requires JS render loop, breaks React rendering model. Rejected.
- *Framer Motion / react-spring*: JS-driven animations add bundle size (~15-30KB) for something CSS handles natively. Rejected.
- *Lottie animations*: Beautiful but requires external animation files, designer tooling, and a runtime library. Over-engineered for geometric shapes. Rejected.

### 2. State resolution hierarchy

**Decision**: A single function resolves the visual state from multiple atoms. Priority order (highest first):

```
1. planReviewStatus === "pending_review" → attention
2. askUser pending for this session       → attention
3. session.status === "completed"         → completed
4. modeMap === "idle"                     → idle
5. modeMap === "active"                   → thinking
6. modeMap === <tool name>                → working
```

**Why this order**: Attention states take priority because they require user action. Completed is checked before idle because a completed session's modeMap might also show "idle". The active/tool-name distinction captures the "between tools" vs "executing tool" difference.

**Alternatives considered**:
- *Derive from `session.status` only*: Only 4 values (idle/running/needs_attention/completed), loses the thinking vs working distinction. Rejected — that distinction is the core value of this feature.
- *Multiple atoms per indicator*: Each indicator subscribes to 3 atoms independently. Works but creates re-render churn when multiple atoms change on the same hook event. A single derived atom is cleaner.

### 3. New `SessionIndicator` component

**Decision**: Extract the indicator into `src/components/session/SessionIndicator.tsx`. Takes `sessionId` as prop, reads atoms internally, renders the appropriate shape + animation class.

**Why a separate component**: Currently the dot is inline JSX in both `SessionRow.tsx` and `CollapsedSidebar`. Duplicating the state resolution logic would be fragile. A shared component ensures consistency and simplifies prototyping variants.

**Interface**:
```tsx
interface SessionIndicatorProps {
  sessionId: string;
  size?: "sm" | "md";  // sm=6px (SessionRow), md=10px (CollapsedSidebar)
}
```

### 4. Collapsed sidebar width increase

**Decision**: Increase from ~24px to 32px. This accommodates a 10px indicator with 11px padding on each side.

**Why 32px**: The current 24px barely fits a 6px dot. At 10px indicator size with animation overshoot (scale up to 1.2x = 12px peak), 32px provides comfortable clearance without wasting space. The `adaptive-layout` spec allows up to 40px for collapsed state.

**Alternatives considered**:
- *Keep 24px, shrink animations*: Animations become invisible at 4-5px. The whole point is legibility. Rejected.
- *Go to 40px (spec maximum)*: Wastes horizontal space. 32px is the sweet spot. Can revisit if additional indicators are added later.

### 5. `prefers-reduced-motion` support

**Decision**: When `prefers-reduced-motion: reduce` is active, all animations are replaced with static shapes in their "rest" position. Colors still indicate state. Shapes still differ per state (circle vs square vs diamond) — only the motion is removed.

**Why**: Accessibility requirement. Users with vestibular disorders should still get state information from shape + color, just without movement.

## Risks / Trade-offs

**[Risk] Animations are too subtle at 6px in expanded SessionRow** → Mitigation: The `sm` size variant uses slightly exaggerated animation parameters (e.g., higher rotation speed) to compensate for small size. Prototyping phase will validate.

**[Risk] Multiple animating indicators cause visual noise** → Mitigation: Animation timing is slow (1.5-2.5s cycles) and uses `ease-in-out` easing. Maximum visual energy comes from "working" sessions, which are the ones the user cares about. Idle and completed are static.

**[Risk] Collapsed sidebar 32px breaks existing layout** → Mitigation: The `adaptive-layout` spec already allows 40px for collapsed panels. 32px is within bounds. Verify with the existing `terminal-focus` and `tool-workspace` presets.

**[Trade-off] CSS-only limits animation complexity** → Accepted. We can't do true "snake chasing tail" with a single `<span>` and CSS. But the shape-morphing approach achieves the same goal (visually distinct, non-trivial animations) within the constraint. If richer animations are needed later, SVG or canvas can replace individual states without changing the component interface.

## Open Questions

- Exact color values for thinking (blue/cyan) and working (green) — should these match the existing theme variables or use custom values?
- Should the indicator size increase slightly in expanded SessionRow too (from 6px to 8px) for better shape legibility?
