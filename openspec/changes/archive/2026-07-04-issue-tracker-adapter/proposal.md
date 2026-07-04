## Why

ClickUp and Linear each ship a full, independently-maintained stack with no shared abstraction — verified byte-for-byte parallel module lists (`src-tauri/src/clickup/` and `src-tauri/src/linear/`, each: `auth`, `client`, `closure`, `integration`, `mirror`, `model`, `poller`, `writeback`, `mod`), ~8-10 frontend components apiece (`src/components/{clickup,linear}/`), and four openspec capabilities apiece. Bug fixes land twice (the pending `clickup-writeback-echo-ordering` change exists precisely because ClickUp missed a pattern Linear already had). A third tracker (GitHub Issues, Jira, Height) today means a third full copy — after which extracting a shared shape becomes 3× harder. **This is a direction finding**: the deliverable is a design + spike, not a build.

## What Changes

- **Design change only** — produce the `IssueTrackerAdapter` design: the shared contract (mirror lifecycle, poller cadence + own-echo registry, writeback command surface, closure flow, auth storage, panel data shape) vs. the per-tracker surface (API client, field vocabulary, state machines like Linear cycles vs ClickUp lists).
- **Spike deliverable**: a written migration path for ONE existing tracker (recommendation to be validated in the spike: ClickUp, since Linear's patterns are the canon the shared shape would adopt) behind the adapter, with an honest cost/risk assessment and a go/no-go recommendation for the extraction.
- **Explicit non-goal**: no production refactor, no third-tracker build, no behavior change lands from this change. Its outputs are `design.md` (in this change), the spike findings appended to it, and — if "go" — follow-up build changes seeded from it.

## Capabilities

### New Capabilities

- `issue-tracker-adapter` (design-only delta): the proposed contract that tracker integrations implement a shared adapter interface so cross-tracker behavior (echo suppression, mirror reconcile, closure) is written once. Marked as a direction capability — requirements below bind only when the extraction is greenlit.

### Modified Capabilities

_None._

## Impact

- **Artifacts only**: this change's `design.md` + spike notes; zero source edits.
- **Follow-up shape (if go)**: an extraction change per layer (backend adapter trait + one tracker migrated; panel component sharing is a separate later question — the UI copies are less mechanical).
- **Risk**: the design could conclude "don't extract" (trackers too divergent) — that is a valid, valuable outcome and gets recorded as a decision.
- **Relation to pending work**: `clickup-writeback-echo-ordering` and `mcp-expose-tracker-tasks` proceed independently; their landing states feed the spike's duplication inventory.
