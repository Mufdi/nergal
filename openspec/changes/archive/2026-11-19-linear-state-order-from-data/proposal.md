Linear: FRI-509

## Why

`src/lib/linearStateOrder.ts` ranks workflow states by a hard-coded name map (`RANK_BY_NAME`). Team RR renamed `Pending` to `Shipped` (FRI-485), so the map is stale: it still treats "pending" as In Review, and `Shipped` only works through the type fallback. Renaming or adding a state in Linear should not need a Nergal change. The mirror already stores each state's `type` and workflow `position`.

## What Changes

- The rank comes only from data: state type first (started → unstarted → backlog → triage → completed → duplicate → canceled), then `position`. Inside an open type, higher `position` ranks first (closest to done, as Linear sorts by status). Inside a closed type, lower `position` ranks first.
- Closed means a `completed`, `duplicate` or `canceled` type. The H control and the filter keep using that.
- No state names remain in the code. The legacy `Pending` rule is removed. Equal type and position break ties by name, so the order does not depend on which issue arrived first (two teams can share a state name).
- Verified in the mirror: both teams store Duplicate with type `duplicate`, so Duplicate ranks before Canceled by type.
- Rollback: one file and its tests; no schema or Rust change.
- Review of Linear's changelog (Sept–Nov 2026): no API change affects workflow state ordering. The candidate "teams report `retiredAt`" is an MCP field, not confirmed in the GraphQL API the mirror uses, so it stays out.

## Capabilities

### New Capabilities

### Modified Capabilities
- `linear-mirror`: "Faithful Linear status glyphs and group order" and "Completed, canceled, and duplicate issues visible by default".

## Impact

- `src/lib/linearStateOrder.ts` and its test. Callers (`LinearPanel.tsx`, `linearFold.ts`) keep the same API.
- With team RR's states the order becomes In Review, In Progress, Todo, Backlog, Triage, Done, Shipped, Duplicate, Canceled.
