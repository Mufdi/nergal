# Implementation Plan: Polymorphic Session Indicators

## Execution Order

1. CSS keyframe animations in `globals.css`
2. `SessionIndicator` component
3. Integrate into `SessionRow.tsx`
4. Integrate into `Sidebar.tsx` (CollapsedSidebar) + width adjustment
5. Visual validation

## 1. CSS Keyframe Animations

### File to modify
- `src/styles/globals.css`

### Current state (lines 210-218)
```css
@keyframes dot-pulse {
  0%, 100% { opacity: 1; transform: scale(1); }
  50% { opacity: 0.4; transform: scale(1.8); }
}
.animate-dot-pulse {
  animation: dot-pulse 1.5s ease-in-out infinite;
}
```

### Replace with

Remove `dot-pulse` entirely. Add 4 new animations:

```css
/* ── Session state indicator animations ── */

/* Base transition for state changes */
.session-indicator {
  transition: border-radius 250ms ease, transform 250ms ease, background-color 250ms ease;
}

/* Thinking: circle ↔ squircle morph */
@keyframes session-thinking {
  0%, 100% { border-radius: 50%; }
  50% { border-radius: 30%; }
}
.animate-session-thinking {
  animation: session-thinking 2.5s ease-in-out infinite;
}

/* Working: continuous rotation with rounded square */
@keyframes session-working {
  from { transform: rotate(0deg); }
  to { transform: rotate(360deg); }
}
.animate-session-working {
  border-radius: 15%;
  animation: session-working 1.5s linear infinite;
}

/* Attention: diamond with gentle bounce */
@keyframes session-attention {
  0%, 100% { transform: rotate(45deg) translateY(0); }
  50% { transform: rotate(45deg) translateY(-2px); }
}
.animate-session-attention {
  border-radius: 15%;
  animation: session-attention 1s ease-in-out infinite;
}

/* Reduced motion: respect accessibility */
@media (prefers-reduced-motion: reduce) {
  .animate-session-thinking,
  .animate-session-working,
  .animate-session-attention {
    animation: none;
  }
  /* Keep static shapes for state differentiation */
  .animate-session-working {
    border-radius: 15%;
  }
  .animate-session-attention {
    border-radius: 15%;
    transform: rotate(45deg);
  }
}
```

### Color values
Using existing Tailwind theme tokens where possible:
- **Idle**: `bg-muted-foreground/40` (existing)
- **Thinking**: `bg-sky-400` (distinct from green, visible on dark bg)
- **Working**: `bg-green-500` (matches current "running" color — continuity)
- **Attention**: `bg-orange-500` (matches current "needs_attention" — continuity)
- **Completed**: n/a (uses Check icon, no dot)

## 2. SessionIndicator Component

### File to create
- `src/components/session/SessionIndicator.tsx`

### Data sources (read-only, no store changes)
- `modeMapAtom` from `src/stores/workspace.ts:53` — `Record<string, string>`
- `planReviewStatusMapAtom` from `src/stores/plan.ts` — `Record<string, string>`
- `askUserAtom` from `src/stores/askUser.ts:16` — `AskUserState | null`
- `session.status` from the `Session` interface in `src/stores/workspace.ts:4`

### Implementation

```tsx
import { useAtomValue } from "jotai";
import { Check } from "lucide-react";
import { modeMapAtom } from "@/stores/workspace";
import { planReviewStatusMapAtom } from "@/stores/plan";
import { askUserAtom } from "@/stores/askUser";

type IndicatorState = "idle" | "thinking" | "working" | "attention" | "completed";

interface SessionIndicatorProps {
  sessionId: string;
  sessionStatus: "idle" | "running" | "needs_attention" | "completed";
  size?: "sm" | "md";
}

const STATE_COLORS: Record<IndicatorState, string> = {
  idle: "bg-muted-foreground/40",
  thinking: "bg-sky-400",
  working: "bg-green-500",
  attention: "bg-orange-500",
  completed: "bg-muted-foreground/30",
};

const STATE_ANIMATIONS: Record<IndicatorState, string> = {
  idle: "",
  thinking: "animate-session-thinking",
  working: "animate-session-working",
  attention: "animate-session-attention",
  completed: "",
};

const SIZE_CLASSES = {
  sm: "size-2",     // 8px — slight increase from 6px for shape legibility
  md: "size-2.5",   // 10px
};

export function SessionIndicator({ sessionId, sessionStatus, size = "sm" }: SessionIndicatorProps) {
  const modeMap = useAtomValue(modeMapAtom);
  const planReviewMap = useAtomValue(planReviewStatusMapAtom);
  const askUser = useAtomValue(askUserAtom);

  const state = resolveState(sessionId, sessionStatus, modeMap, planReviewMap, askUser);

  if (state === "completed") {
    return <Check className="size-3 shrink-0 text-muted-foreground/50" />;
  }

  return (
    <span
      className={`shrink-0 session-indicator ${SIZE_CLASSES[size]} ${STATE_COLORS[state]} ${STATE_ANIMATIONS[state]}`}
      aria-hidden="true"
    />
  );
}

function resolveState(
  sessionId: string,
  sessionStatus: string,
  modeMap: Record<string, string>,
  planReviewMap: Record<string, string>,
  askUser: { sessionId: string } | null,
): IndicatorState {
  // Priority 1: Attention states
  if (planReviewMap[sessionId] === "pending_review") return "attention";
  if (askUser?.sessionId === sessionId) return "attention";

  // Priority 2: Completed
  if (sessionStatus === "completed") return "completed";

  // Priority 3: Mode-derived
  const mode = modeMap[sessionId] ?? "idle";
  if (mode === "idle") return "idle";
  if (mode === "active") return "thinking";
  return "working"; // tool name
}
```

### Note on askUser check
`askUserAtom` is global (single state, not per-session) but includes `sessionId`. The check `askUser?.sessionId === sessionId` correctly scopes it. Only one AskUser can be active at a time (Claude Code blocks until answered).

### Note on size increase
Changing from `size-1.5` (6px) to `size-2` (8px) for `sm` variant. At 6px, the difference between a circle and a squircle is ~1px of border-radius change — invisible. At 8px, it's noticeable. This is a 2px increase that doesn't affect SessionRow layout (the row height is set by text, not the indicator).

## 3. Integrate into SessionRow

### File to modify
- `src/components/session/SessionRow.tsx`

### Current code (lines 84-91)
```tsx
{isCompleted ? (
  <Check className="size-3 shrink-0 text-muted-foreground/50" />
) : (
  <span
    className={`size-1.5 shrink-0 rounded-full ${STATUS_DOT_COLORS[session.status]} ${session.status === "running" ? "animate-dot-pulse" : ""}`}
    aria-hidden="true"
  />
)}
```

### Replace with
```tsx
<SessionIndicator sessionId={session.id} sessionStatus={session.status} size="sm" />
```

### Also remove
- `STATUS_DOT_COLORS` constant (lines 18-23) — no longer needed
- `Check` import if only used here — but `SessionIndicator` handles it internally

## 4. Integrate into CollapsedSidebar + Width

### File to modify
- `src/components/layout/Sidebar.tsx`

### Current collapsed dot (line 165)
```tsx
<span className={`size-1.5 rounded-full ${STATUS_DOT[s.status] ?? "bg-muted-foreground"} ${s.status === "running" ? "animate-dot-pulse" : ""}`} />
```

### Replace with
```tsx
<SessionIndicator sessionId={s.id} sessionStatus={s.status} size="md" />
```

### Remove
- `STATUS_DOT` constant (lines 127-132) — no longer needed

### Width change

Current container (line 147):
```tsx
<div className="flex h-full w-full flex-col items-center gap-0.5 bg-card py-1">
```

The outer container width is set by the parent. Find where the collapsed sidebar width is defined. Based on the code, `CollapsedSidebar` is rendered inside a div with conditional width. Search for the sidebar width toggle logic.

The collapsed sidebar container's width likely comes from the parent `Sidebar` component or from a CSS/tailwind class. Look for `w-` class on the sidebar wrapper. The `CollapsedSidebar` function receives `onToggle` and renders `w-full` — so the width is set by the parent.

In `Sidebar.tsx`, search for the collapsed width class. The expand button (line 148) is `size-4`, and session dots are `size-4` hitbox. At 32px (`w-8`), this fits cleanly: 8px padding + 16px hitbox + 8px padding = 32px.

Change the parent container that wraps `<CollapsedSidebar>` from its current width to `w-8` (32px). If it's controlled by a CSS variable or a Jotai atom, update that value.

## 5. Visual Validation

### Test matrix

| State | Trigger | Expected visual |
|-------|---------|----------------|
| Idle | Session at prompt, no activity | Static circle, gray |
| Thinking | Claude responding between tool calls (`mode = "active"`) | Morphing circle↔squircle, sky blue, 2.5s |
| Working | Claude executing a tool (`mode = "Read"/"Edit"/"Bash"`) | Rotating square, green, 1.5s |
| Attention | Plan review pending OR AskUser dialog | Bouncing diamond, orange, 1s |
| Completed | Session marked completed | Check icon, dim |

### Transition test
1. Start a session (idle → circle)
2. Send a prompt (idle → thinking → working → thinking → working → ... → idle)
3. Trigger plan mode (working → attention)
4. Approve plan (attention → working/thinking)
5. Session stops (→ idle)

Verify smooth CSS transitions between each state change (border-radius + color + transform).

### Multi-session test
Open 4+ sessions in different states simultaneously. Verify:
- No visual noise (animations are slow enough)
- Collapsed sidebar indicators are legible at 10px
- No jank (check DevTools Performance — all animations should be compositor-only)

### Reduced motion test
Enable `prefers-reduced-motion: reduce` in DevTools → Rendering. Verify:
- All animations stop
- Shapes still differentiate states (circle vs square vs diamond)
- Colors still match states

## File Summary

| File | Action | Changes |
|------|--------|---------|
| `src/styles/globals.css` | Modify | Remove `dot-pulse`, add 4 new keyframes + `session-indicator` transition + reduced motion |
| `src/components/session/SessionIndicator.tsx` | Create | New component: resolves state, renders shape + animation |
| `src/components/session/SessionRow.tsx` | Modify | Replace inline dot with `<SessionIndicator>`, remove `STATUS_DOT_COLORS` |
| `src/components/layout/Sidebar.tsx` | Modify | Replace collapsed dot with `<SessionIndicator>`, remove `STATUS_DOT`, adjust width to 32px |
