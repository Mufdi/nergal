# conflict-resolution

Makes the onResolved "prior activity" gate per-session, so activity in one session never routes another.

## MODIFIED Requirements

### Requirement: ConflictsPanel onResolved callback

`ConflictsPanel` SHALL accept an optional `onResolved` callback that MUST fire 1.5s after both `files.length` and `pendingMerge` go falsy, gated on prior activity **observed under the currently displayed `sessionId`** and disabled in Zen mode. The activity gate SHALL reset whenever the panel's `sessionId` prop changes, so activity seen for one session never satisfies the gate for another. The chip wires this to switch to the `PRs` chip.

#### Scenario: Resolve drains, panel routes

- **WHEN** the panel saw activity for the displayed session and now has 0 conflicts and no pending merge
- **AND** is not in Zen mode
- **THEN** after 1500ms the `onResolved` callback fires; the chip translates this to a `chipMode = "prs"` for the workspace

#### Scenario: Activity from a previous session does not route the new one

- **GIVEN** the panel saw conflicts under session A
- **WHEN** the panel's `sessionId` prop switches to session B, which has 0 conflicts and no pending merge
- **THEN** `onResolved` does not fire for B until B itself first shows activity
