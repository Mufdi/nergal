# issue-tracker-adapter Specification

## Purpose
TBD - created by archiving change issue-tracker-adapter. Update Purpose after archive.
## Requirements
### Requirement: Tracker integrations share one adapter contract

Tracker integrations SHALL implement a shared `IssueTrackerAdapter` contract covering the mechanical layers — mirror reconcile lifecycle, poll cadence with tombstoning, own-echo writeback registry (provisional record before the API call, clear on failure, TTL), closure token flow, and keyring auth storage — with tracker-specific clients, models, and state vocabularies living behind the contract's associated types.

#### Scenario: cross-tracker fix lands once

- **WHEN** a defect is found in shared mechanics (e.g. echo-suppression ordering)
- **THEN** the fix is made once in the adapter layer and every tracker integration inherits it

#### Scenario: third tracker is a fill-in job

- **WHEN** a new tracker (e.g. GitHub Issues) is integrated
- **THEN** the work is implementing the adapter's tracker-specific surface, not copying and adapting an existing stack

### Requirement: Extraction is gated on the spike's go decision

The adapter extraction SHALL NOT begin until the spike deliverables exist (duplication inventory, trait draft, paper-migration of one tracker, go/no-go recommendation recorded in this change's design.md) and the go decision is made by the user.

#### Scenario: no-go is a recorded outcome

- **WHEN** the spike concludes the trackers are too divergent to extract profitably
- **THEN** the recommendation and the duplication inventory are recorded, and no refactor proceeds

