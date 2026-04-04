---
status: archived
implemented: 2026-04-04
archived: 2026-04-04
files:
  - src/components/session/SessionIndicator.tsx
  - src/styles/globals.css
  - src/components/session/SessionRow.tsx
  - src/components/layout/Sidebar.tsx
  - src/components/layout/Workspace.tsx
  - src/components/layout/TopBar.tsx
  - src/components/layout/StatusBar.tsx
  - src/stores/hooks.ts
  - src-tauri/src/hooks/server.rs
---

## Purpose

Provide polymorphic session indicators that communicate session state at a glance via color and motion type, using a shared `SessionIndicator` component across sidebar, TopBar, and collapsed sidebar.

## Implementation Notes

Original spec proposed shape-shifting (circle→squircle→square→diamond). Implemented as circle-only with motion layers instead — more modern, legible at small sizes. Thinking state uses color-only (no animation) per user feedback. During implementation, discovered and fixed a critical session routing bug: all event payloads used Claude CLI session ID instead of cluihud session ID, causing cross-session state contamination.

## Requirements

### Requirement: Polymorphic indicator resolves visual state from session data
The system SHALL render a polymorphic indicator for each session that derives its visual state from the session's `modeMap` value, `planReviewStatus`, and `askUser` state. The resolution priority SHALL be: attention (plan review or ask user pending) > completed > idle > thinking (active between tools) > working (tool executing).

#### Scenario: Idle session shows static circle
- **WHEN** a session's mode is `"idle"` and it has no pending plan review or ask-user
- **THEN** the indicator renders as a static circle with muted gray color and no animation

#### Scenario: Thinking session shows blue circle
- **WHEN** a session's mode is `"active"` (between tool calls)
- **THEN** the indicator renders as a static circle with sky-blue color and no animation

#### Scenario: Working session shows circle with orbital ring
- **WHEN** a session's mode is a tool name (e.g., `"Read"`, `"Edit"`, `"Bash"`)
- **THEN** the indicator renders as a green circle with a rotating orbital ring (0.8s cycle)

#### Scenario: Attention session shows circle with ripple ping
- **WHEN** a session has `planReviewStatus === "pending_review"` OR has a pending `askUser` question
- **THEN** the indicator renders as an orange circle with expanding ripple/ping animation (1.2s cycle)
- **AND** the attention state takes priority over any mode-derived state

#### Scenario: Completed session shows check icon
- **WHEN** a session's status is `"completed"`
- **THEN** the indicator renders as a static check icon with dim color and no animation

### Requirement: Indicator renders in sidebar, TopBar, and collapsed sidebar
The system SHALL use the same `SessionIndicator` component in expanded `SessionRow` (xs: 6px), TopBar session tabs (xs: 6px), and `CollapsedSidebar` (md: 10px).

#### Scenario: Consistent behavior across locations
- **WHEN** a session is in working state
- **THEN** the orbital ring animation appears identically in all three locations at the appropriate size

### Requirement: CSS-only GPU-compositable animations
All indicator animations SHALL use only CSS `@keyframes` with GPU-compositable properties (`transform`, `opacity`). Orbital ring and ripple use `::after` pseudo-elements.

#### Scenario: No layout thrash during animation
- **WHEN** multiple sessions are animating simultaneously
- **THEN** no CSS properties that trigger layout recalculation are animated

### Requirement: Reduced motion accessibility
The system SHALL respect the `prefers-reduced-motion: reduce` media query. When active, all animations MUST be disabled. Color MUST still differ per state.

#### Scenario: Reduced motion preference active
- **WHEN** the user's system has `prefers-reduced-motion: reduce` enabled
- **THEN** all indicators display static circles with state-appropriate colors but no animation

### Requirement: Collapsed sidebar width accommodates indicators
The collapsed sidebar width SHALL be 32px to accommodate 10px indicators with animation overshoot clearance.

#### Scenario: Collapsed sidebar at 32px
- **WHEN** the sidebar is collapsed
- **THEN** it renders at 32px width with `SessionIndicator` at md size for each session

### Requirement: Event payloads use cluihud session ID
All Tauri event payloads (`plan:ready`, `cost:update`, `cwd:changed`, `file:changed`, `statusline:update`, `ask:user`, `permission:denied`) SHALL use `cluihud_session_id` (falling back to Claude CLI session ID) in the `session_id` field. Frontend listeners SHALL use `payload.session_id` to route state to the correct session.

#### Scenario: Plan ready routed to correct session
- **WHEN** session 4 generates a plan while session 3 is active in the UI
- **THEN** the plan state is associated with session 4, not session 3

#### Scenario: Session switch opens pending plan
- **WHEN** user switches to a session with `pending_review` plan status
- **THEN** the plan tab opens and the right panel expands automatically
