## 1. CSS Animations

- [ ] 1.1 Remove existing `dot-pulse` keyframe and `.animate-dot-pulse` class from `src/styles/globals.css`
- [ ] 1.2 Add `@keyframes session-thinking` in `globals.css`: border-radius morph between 50% (circle) and 30% (squircle), 2.5s ease-in-out infinite
- [ ] 1.3 Add `@keyframes session-working` in `globals.css`: continuous rotation 0→360deg, 1.5s linear infinite, with border-radius at 15%
- [ ] 1.4 Add `@keyframes session-attention` in `globals.css`: gentle vertical bounce (translateY 0→-2px→0), 1s ease-in-out infinite, with 45deg base rotation
- [ ] 1.5 Add `@media (prefers-reduced-motion: reduce)` block that disables all `session-*` animations (animation: none)
- [ ] 1.6 Add CSS transition rule for indicator state changes: `transition: border-radius 250ms ease, transform 250ms ease, background-color 250ms ease`

## 2. SessionIndicator Component

- [ ] 2.1 Create `src/components/session/SessionIndicator.tsx` with props `{ sessionId: string, size?: "sm" | "md" }`. Default size: `"sm"`.
- [ ] 2.2 Implement state resolution function inside the component: read `modeMapAtom`, `planReviewStatusMapAtom`, and `askUserAtom` via Jotai `useAtomValue`. Return one of: `"idle"` | `"thinking"` | `"working"` | `"attention"` | `"completed"`. Priority: attention > completed > idle > thinking > working.
- [ ] 2.3 Map resolved state to CSS classes: shape class (border-radius + base transform), color class (bg-*), animation class (animate-session-*). Render a single `<span>` with computed classes.
- [ ] 2.4 Size variants: `sm` = `size-1.5` (6px), `md` = `size-2.5` (10px). Animation parameters same for both (CSS handles scaling).

## 3. Integration into Sidebar

- [ ] 3.1 Replace the inline status dot in `SessionRow.tsx:84-91` with `<SessionIndicator sessionId={session.id} size="sm" />`. Remove the `STATUS_DOT_COLORS` map and `animate-dot-pulse` class usage.
- [ ] 3.2 Replace the inline status dot in `Sidebar.tsx:165` (CollapsedSidebar) with `<SessionIndicator sessionId={s.id} size="md" />`. Remove the `STATUS_DOT` map.
- [ ] 3.3 Update CollapsedSidebar container width from current value to `w-8` (32px). Adjust internal padding/alignment for the larger indicator.
- [ ] 3.4 Verify layout in expanded sidebar: SessionIndicator at `sm` size aligns correctly with session name text and shortcut badge.

## 4. Prototyping & Visual Validation

- [ ] 4.1 Create a dev-only test scenario with 4 sessions in different states (idle, thinking, working, attention) visible simultaneously in the sidebar. Verify visual distinction.
- [ ] 4.2 Test collapsed sidebar at 32px: indicators legible, animations don't clip, tooltips still work.
- [ ] 4.3 Test state transitions: trigger mode changes (idle→working→thinking→idle) and verify smooth CSS transitions between shapes.
- [ ] 4.4 Test `prefers-reduced-motion`: enable the setting and verify all indicators show static shapes with correct colors.
- [ ] 4.5 Test with 5+ sessions: verify multiple simultaneous animations don't cause jank (check compositor-only rendering via DevTools Performance tab).
