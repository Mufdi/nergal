## ADDED Requirements

### Requirement: Polymorphic indicator resolves visual state from session data
The system SHALL render a polymorphic indicator for each session that derives its visual state from the session's `modeMap` value, `planReviewStatus`, and `askUser` state. The resolution priority SHALL be: attention (plan review or ask user pending) > completed > idle > thinking (active between tools) > working (tool executing).

#### Scenario: Idle session shows static circle
- **WHEN** a session's mode is `"idle"` and it has no pending plan review or ask-user
- **THEN** the indicator renders as a static circle with muted gray color and no animation

#### Scenario: Thinking session shows morphing squircle
- **WHEN** a session's mode is `"active"` (between tool calls)
- **THEN** the indicator renders as a circle that morphs to squircle and back with blue/cyan color on a ~2.5s cycle

#### Scenario: Working session shows rotating square
- **WHEN** a session's mode is a tool name (e.g., `"Read"`, `"Edit"`, `"Bash"`)
- **THEN** the indicator renders as a square with rounded corners that rotates continuously with green color on a ~1.5s cycle

#### Scenario: Attention session shows bouncing diamond
- **WHEN** a session has `planReviewStatus === "pending_review"` OR has a pending `askUser` question
- **THEN** the indicator renders as a diamond (45° rotated square) with gentle vertical bounce and orange/amber color
- **AND** the attention state takes priority over any mode-derived state

#### Scenario: Completed session shows check icon
- **WHEN** a session's status is `"completed"`
- **THEN** the indicator renders as a static check icon with dim color and no animation

### Requirement: Indicator renders in both expanded and collapsed sidebar
The system SHALL use the same `SessionIndicator` component in both the expanded `SessionRow` and the `CollapsedSidebar`. The component MUST accept a size variant: `sm` (6-8px, used in SessionRow) and `md` (10px, used in CollapsedSidebar).

#### Scenario: Expanded sidebar indicator
- **WHEN** the sidebar is expanded
- **THEN** the `SessionRow` renders the indicator at `sm` size in the same position as the current status dot

#### Scenario: Collapsed sidebar indicator
- **WHEN** the sidebar is collapsed
- **THEN** each session dot is replaced by the indicator at `md` size

### Requirement: CSS-only GPU-compositable animations
All indicator animations SHALL use only CSS `@keyframes` with GPU-compositable properties (`transform`, `opacity`, `border-radius`). No JavaScript animation loops or external animation libraries SHALL be used.

#### Scenario: No layout thrash during animation
- **WHEN** multiple sessions are animating simultaneously
- **THEN** no CSS properties that trigger layout recalculation (width, height, margin, padding) are animated
- **AND** the browser compositor handles all animation frames

### Requirement: Reduced motion accessibility
The system SHALL respect the `prefers-reduced-motion: reduce` media query. When active, all animations MUST be disabled. Shape and color MUST still differ per state to preserve information.

#### Scenario: Reduced motion preference active
- **WHEN** the user's system has `prefers-reduced-motion: reduce` enabled
- **THEN** all indicators display their rest-position shape (circle, square, diamond) with state-appropriate colors but no animation

### Requirement: State transition smoothness
The system SHALL animate transitions between indicator states (e.g., idle → thinking → working) using CSS transitions on `border-radius` and `transform` with a duration of 200-300ms.

#### Scenario: Transition from idle to working
- **WHEN** a session transitions from idle to working (tool use starts)
- **THEN** the indicator smoothly morphs from circle to rotating square over 200-300ms
- **AND** the color transitions from gray to green

#### Scenario: Transition from working to attention
- **WHEN** a working session receives a plan review request
- **THEN** the indicator smoothly transitions from rotating square to bouncing diamond
- **AND** the color transitions from green to orange
