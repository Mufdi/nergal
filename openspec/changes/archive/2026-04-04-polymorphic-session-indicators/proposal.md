## Why

The sidebar currently uses a single generic animation (pulsing dot) for all "running" sessions and static colored dots for other states. With multiple parallel sessions, the user must click into each session to understand what it's doing. Polymorphic shape-shifting indicators communicate session state at a glance — geometry, color, and motion each encode distinct information, making the sidebar a real-time dashboard without requiring interaction.

## What Changes

- Replace the current 6px status dot in SessionRow and CollapsedSidebar with a polymorphic indicator that shifts between geometries (circle, squircle, square, diamond) based on session state
- Map 4 granular states from existing data sources (`modeMapAtom`, `planReviewStatusMapAtom`, `askUserAtom`) to distinct shape + color + animation combinations
- Remove the existing `animate-dot-pulse` CSS animation, replace with 4 state-specific keyframe animations
- Widen collapsed sidebar from ~24px to ~32-36px to ensure animations remain legible at small size
- Extract indicator logic into a reusable component that resolves state → visual properties

## Capabilities

### New Capabilities
- `session-state-indicators`: Polymorphic shape-shifting micro-indicators for session status in the sidebar. Covers state-to-visual mapping, CSS keyframe animations, shape morphing mechanics, and collapsed sidebar sizing.

### Modified Capabilities
- `adaptive-layout`: Collapsed sidebar minimum width changes from ~24px to ~32-36px to accommodate legible animated indicators.

## Impact

- **Frontend components**: `SessionRow.tsx` (indicator replacement), `Sidebar.tsx` (CollapsedSidebar width + indicator), new `SessionIndicator.tsx` component
- **CSS**: `globals.css` — remove `dot-pulse`, add 4 new keyframe animations (idle, thinking, working, attention)
- **Stores**: Read-only consumption of `modeMapAtom`, `planReviewStatusMapAtom`, `askUserAtom` — no store changes needed
- **Layout**: Collapsed sidebar width increase may affect adjacent panel sizing
