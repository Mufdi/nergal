# linear-mirror

## ADDED Requirements

### Requirement: Personal API key stored in the OS keyring, auth layer extensible for OAuth

The system SHALL store the user's Linear Personal API key in the OS keyring (secret-service on Linux) under service `cluihud`, account `linear-token`. When the keyring is unavailable, the system SHALL fall back to `~/.config/cluihud/linear.toml` created **atomically with mode `0600`** (no write-then-chmod window) and SHALL surface to the UI that the key is stored on disk. The key SHALL be read only into the GraphQL client, never logged or included in any error string, and never returned to the frontend. The system SHALL expose commands to set, clear, and validate the key; validation SHALL run the `viewer { id name email }` query and return the resolved user, not the key. The authorization header SHALL be produced by an `AuthMode`-switchable builder: personal keys send `Authorization: <key>` (no `Bearer`); the builder SHALL leave an additive path for a future `Bearer <token>` OAuth mode without reworking the client or persistence.

#### Scenario: Key stored in keyring on the happy path

- **WHEN** the user sets an API key and the OS keyring is available
- **THEN** the key SHALL be written to the keyring under service `cluihud` / account `linear-token`
- **AND** no plaintext key SHALL be written to disk
- **AND** validation SHALL return the resolved Linear user via the `viewer` query

#### Scenario: Keyring-absent fallback is atomic and disclosed

- **WHEN** the user sets a key and the keyring is unavailable
- **THEN** the key file SHALL be created atomically with mode `0600`
- **AND** the UI SHALL indicate the key is stored on disk
- **AND** at no point SHALL the file exist readable at a wider mode

#### Scenario: Key never leaks to logs or errors

- **WHEN** a request fails or an error is logged
- **THEN** the error string and log output SHALL NOT contain the key

#### Scenario: Transient keyring failure is not a missing key

- **WHEN** reading the key fails with a transient keyring error (e.g. a D-Bus failure), as opposed to a definitive no-entry
- **THEN** the system SHALL surface a transient error state, NOT a "no key configured" state
- **AND** the poll loop SHALL retry on subsequent cycles instead of terminating

#### Scenario: Personal-key header carries no Bearer prefix

- **WHEN** the client builds the authorization header in personal `AuthMode`
- **THEN** the header SHALL be `Authorization: <key>` with no `Bearer` prefix
- **AND** the builder SHALL expose an additive branch for a future OAuth `Bearer` mode

### Requirement: GraphQL client with leaky-bucket rate handling and cursor pagination

The system SHALL provide a typed Linear GraphQL client over the shared `reqwest` client targeting `https://api.linear.app/graphql`, covering the read queries needed for the mirror (viewer, teams with workflow states and labels, paginated issues with nested relations, comments). Because Linear signals rate limiting as **HTTP 400 with a GraphQL error code `RATELIMITED`** (not `429`/`Retry-After`), the client SHALL parse the GraphQL body even on an HTTP 4xx (a 400 may be a rate-limit, not a hard failure) and, on `RATELIMITED`, back off until the reset of **the bucket that actually returned exhausted** (`X-RateLimit-Requests-Reset` or `X-RateLimit-Complexity-Reset`, UTC epoch ms — the one whose `*-Remaining` is `0`), with the wait clamped to a `[floor, cap]` range so neither a forward- nor a backward-skewed clock breaks it, then retry a bounded number of times. A query exceeding the per-query complexity cap is a **hard error distinct from `RATELIMITED`** and SHALL NOT be retried as a rate-limit. The client SHALL paginate connections by the Relay `pageInfo { hasNextPage, endCursor }` shape using `first`/`after`, ordering issues by `updatedAt` descending so the active window is fetched first, and SHALL keep page `first` sizes small enough that a single nested page stays under the per-query complexity cap.

#### Scenario: RATELIMITED on HTTP 400 is not a hard failure

- **WHEN** the API responds `HTTP 400` with a GraphQL error whose code is `RATELIMITED`
- **THEN** the client SHALL parse the body rather than treat the 4xx as a transport failure
- **AND** SHALL wait until the reset of the exhausted bucket (clamped to the `[floor, cap]` range), not the nearer of the two buckets
- **AND** SHALL give up after a bounded number of retries

#### Scenario: Hard complexity rejection is not retried as a rate-limit

- **WHEN** a query is rejected for exceeding the per-query complexity cap
- **THEN** the client SHALL surface it as a normal error
- **AND** SHALL NOT enter the rate-limit backoff/retry loop

#### Scenario: Connection paginates by hasNextPage, not row count

- **WHEN** an issues page returns fewer rows than `first` but `pageInfo.hasNextPage` is true
- **THEN** the client SHALL request the next page with `after: endCursor`
- **AND** SHALL stop only when `hasNextPage` is false

#### Scenario: GraphQL errors are surfaced without leaking the key

- **WHEN** a query returns a non-rate-limit GraphQL `errors` array
- **THEN** the client SHALL surface the error messages
- **AND** the surfaced error SHALL NOT contain the API key

### Requirement: Structure-agnostic local mirror of the Linear hierarchy

The system SHALL maintain a local SQLite mirror modeling `Team → Issue → Sub-issue` with per-team workflow states and labels, with real foreign keys, such that adding a workflow state, a label, a project, a cycle, or a team in Linear is absorbed as data without a code change or migration. Workflow states SHALL be stored per-team with Linear's native `type` (`triage`/`backlog`/`unstarted`/`started`/`completed`/`canceled`), not an enum baked in code. Labels SHALL be stored as definition rows plus an issue↔label join (labels are first-class, colored, and a group-by axis). Priority SHALL be stored as Linear's integer (`0`=none, `1`=urgent, `2`=high, `3`=medium, `4`=low) and mapped to the priority glyph at render. Projects and cycles SHALL be stored as minimal metadata rows referenced by issues (nullable). Sub-issues SHALL be issues with a non-null `parent_id`, and the sub-issue tree SHALL be built solely from `parent_id` populated from the issue's flat `parent` relation. The panel SHALL read its view-model exclusively from the mirror. Linear has no ClickUp-style custom fields or checklists; the mirror SHALL NOT model them.

#### Scenario: New per-team workflow state appears without code change

- **WHEN** a team gains a new workflow state in Linear
- **THEN** the next sync SHALL insert a new `linear_workflow_states` row for that team with its native `type`
- **AND** the panel SHALL render it (and its StatusIcon class) without a code or schema change

#### Scenario: New label appears without code change

- **WHEN** an issue gains a label that the mirror has not seen
- **THEN** the next sync SHALL upsert the label definition and the issue↔label join row
- **AND** the panel SHALL render and offer group-by for it without a migration

#### Scenario: Sub-issue tree from parent_id

- **WHEN** the issues fetch returns a sub-issue with a `parent`
- **THEN** the mirror SHALL set the sub-issue's `parent_id` from that `parent`
- **AND** SHALL build the sub-issue tree from `parent_id`

#### Scenario: Panel renders from the mirror, not live calls

- **WHEN** the panel displays issues
- **THEN** it SHALL read from the mirror
- **AND** SHALL NOT issue a live Linear GraphQL call per render

### Requirement: Poll scope is bounded; un-assignment is detectable

The system SHALL poll issues for the selected team(s) within a bounded scope — never every issue in the workspace, because a Linear workspace can hold tens of thousands of issues and an unscoped poll is infeasible against the complexity quota. One timestamp captured at cycle start SHALL drive both the server filter and the tombstone-candidate predicate. The scope SHALL be the union of three sets: (1) issues in the selected teams with `updatedAt` within a rolling window; (2) issues in the selected teams whose `assignee` is the viewer (a **team-constrained** query, never workspace-wide — it would drag in issues from unfetched teams); and (3) the **delta** of mirror issues currently flagged the viewer's that sets 1 and 2 did not already return, re-fetched by id. Set 3 SHALL make un-assignment detectable **independently of whether an assignee change advances `updatedAt`**, without re-fetching the whole assigned backlog (still-assigned issues are already in set 2 and excluded from set 3). The by-id query SHALL chunk its id list and cursor-paginate each chunk, and SHALL contribute its completeness to the cycle's `complete` flag. Because `issues(filter:{id:{in:[…]}})` does not error on a missing id (a deleted/archived/access-revoked issue is simply absent from the result), set-3 outcomes SHALL be: a returned issue no longer the viewer's clears its flag; a returned issue whose team is no longer selected is evicted (its workflow states are unfetched, so it cannot render); an issue **absent from a complete set-3** clears its flag and is evicted (the deleted-while-mine path); an issue **absent from an incomplete set-3 is retained unchanged** (its absence means "not reached", and evicting on it would delete the user's real issues). Assigned-to-me SHALL remain a local filter over the mirror. When the account has more than one team, the system SHALL let the user choose which team(s) to sync rather than silently syncing the first.

#### Scenario: Un-assignment is detectable regardless of updatedAt

- **WHEN** an issue currently shown as the viewer's is reassigned to someone else
- **THEN** because the mirror re-fetches every currently-mine issue by id each cycle (set 3), the system SHALL observe the new assignee even if the reassignment did not advance `updatedAt` or the issue left the window
- **AND** SHALL clear the mirror's "viewer's" flag rather than leaving it shown as assigned to the viewer

#### Scenario: Viewer-assigned union is constrained to selected teams

- **WHEN** the poll fetches the viewer-assigned set
- **THEN** it SHALL constrain that query to the selected teams
- **AND** SHALL NOT fetch viewer-assigned issues from unselected teams (whose workflow states were never fetched)

#### Scenario: Multiple teams prompt a choice

- **WHEN** the account has more than one team
- **THEN** the system SHALL let the user choose which team(s) to sync
- **AND** SHALL NOT silently sync only the first team

#### Scenario: Viewer-assigned issues outside the window are still synced

- **WHEN** an issue assigned to the viewer has not been updated within the rolling window
- **THEN** the poll SHALL still include it via sets 2 and 3
- **AND** the panel SHALL show it

#### Scenario: Token or team change reflects immediately

- **WHEN** the user sets a key or selects team(s)
- **THEN** the system SHALL emit a `syncing` status immediately, without waiting for the first network cycle
- **AND** deselecting a team SHALL tombstone that team's mirror contents

### Requirement: Atomic reconcile, completeness-gated tombstoning, age-out eviction

The system SHALL refresh the mirror on a configurable interval by fetching everything a cycle needs first, then committing in a single SQLite transaction with upsert order `teams → workflow-states → labels → projects → cycles → users → issues → issue-labels → comments`, so the panel never reads a torn mid-reconcile state and a mid-cycle network failure commits nothing. An issue referencing a non-null `state_id`/`team_id` unknown to the fetched hierarchy SHALL synthesize a placeholder row (flagged `synthetic`) and log it, never abort the poll on a foreign-key violation; a non-null `parent_id` whose parent is out of scope SHALL leave the issue as a tolerant tree root (the sub-issue tree is built in app from `parent_id`, not enforced by a self-foreign-key), never a foreign-key abort.

Tombstoning SHALL fire **only when every paginated branch for every selected team reached `hasNextPage == false`** (a provably complete fetch). If any branch was interrupted (rate-limit give-up, network error), the cycle SHALL commit upserts only and SHALL NOT tombstone. On a complete fetch, an issue SHALL be tombstoned (`stale = 1`) iff it belongs to a selected team, was in scope (`updated_at` within the window measured from the cycle-start timestamp), is absent from the cycle's global fetched-id set (across all selected teams and all branches, so an issue moved between two selected teams survives), is not `synthetic`, and is not currently flagged the viewer's. A reappearing issue SHALL be un-tombstoned (`stale = 0`). An issue that has **aged out** of the window and is not the viewer's SHALL be removed by a dedicated **eviction pass** (childless first), distinct from tombstone-GC, since absence then means "out of scope", not "deleted". Workspace and team **labels SHALL be upsert-only, never tombstoned by absence**; a label definition SHALL be GC'd only when no live issue references it. Tombstoned rows SHALL be garbage-collected only after a retention window and only when childless. After each committed reconcile the system SHALL emit `linear:changed`. The system SHALL NOT use webhooks.

#### Scenario: Interrupted fetch never tombstones

- **WHEN** a poll cycle's pagination is interrupted (rate-limit give-up or network error) before every branch reaches `hasNextPage == false`
- **THEN** the cycle SHALL commit upserts only
- **AND** SHALL tombstone zero issues (so issues on un-fetched pages are not falsely marked stale)

#### Scenario: Reconcile commits atomically

- **WHEN** a poll cycle reconciles the mirror
- **THEN** all upserts and tombstones SHALL commit in one transaction
- **AND** a concurrent panel read SHALL NOT observe states tombstoned while their issues are not yet updated

#### Scenario: In-scope removed issue is tombstoned; aged-out issue is evicted

- **WHEN** an issue in the window is absent from a complete fetch (deleted or completed-and-removed)
- **THEN** the system SHALL mark it `stale = 1` and record when it went stale, and un-tombstone it if it reappears
- **AND** **WHEN** instead an issue has aged past the window and is not the viewer's, the eviction pass SHALL remove it (childless first) rather than leave it indefinitely stale

#### Scenario: Issue moved between two selected teams survives

- **WHEN** an issue moves from selected team A to selected team B between polls
- **THEN** because the tombstone-candidate test uses the cycle's global fetched-id set, the issue (present in team B's fetch) SHALL NOT be tombstoned by team A's absence
- **AND** its `team_id` SHALL be updated to B

#### Scenario: Synthetic placeholder is not tombstoned

- **WHEN** a synthesized placeholder state/team row (never present in Linear's fetch) is reconciled
- **THEN** it SHALL be excluded from absence-tombstoning
- **AND** SHALL be re-resolved to the real row when that row arrives in a later fetch

### Requirement: Account swap is isolated by a key-generation epoch

The system SHALL guard an account swap with a monotonic `key_generation` counter, not a viewer-id comparison. Setting a key (the sole key-setter) SHALL atomically bump `key_generation`, wipe the mirror, clear the cached viewer id, clear the persisted team selection (stale ids from the old org would otherwise leave a permanently empty panel), and reset the baseline — immediately, not deferred to the next poll. Each poll cycle SHALL capture `key_generation` at start and, inside the reconcile transaction, SHALL discard its commit if the generation changed, so a cycle that fetched the prior account's data cannot re-populate the wiped mirror. Validating a key SHALL NOT participate in the reset (it does not persist a key). The cycle's viewer resolve SHALL only seed the cached viewer id for the assigned-to-me filter and SHALL NOT itself wipe; a failed or empty viewer resolve SHALL skip the cycle and never wipe. `baseline_done` SHALL be set to `1` only after a complete sync over a non-empty team selection.

#### Scenario: Key swapped mid-cycle does not re-populate the wiped mirror

- **WHEN** a poll cycle has fetched the prior account's issues and the user sets a new key before that cycle commits
- **THEN** `linear_set_key` SHALL bump `key_generation`, wipe the mirror, and clear the viewer id + team selection immediately
- **AND** the in-flight cycle SHALL detect the changed generation inside its transaction and discard its commit, leaving the prior-account data unwritten

#### Scenario: A failed viewer resolve never wipes the mirror

- **WHEN** the cycle-start `viewer` resolve fails or returns empty (rate-limit, network error, transient keyring)
- **THEN** the system SHALL skip the cycle and SHALL NOT wipe the mirror or reset the baseline

#### Scenario: Zero-team sync sets no baseline

- **WHEN** a sync runs over an empty team selection
- **THEN** the system SHALL NOT set `baseline_done = 1`

#### Scenario: Deleted-while-mine is cleaned only on a complete re-verify

- **WHEN** an issue the mirror flags as the viewer's is deleted (and so is absent from the by-id set-3 result)
- **THEN** if that set-3 was complete, the system SHALL clear the flag and evict the issue
- **AND** if that set-3 was incomplete, the system SHALL retain the issue unchanged rather than evict it on absence

#### Scenario: Unknown state id does not crash the poll

- **WHEN** a fetched issue references a `state_id` absent from the fetched workflow states
- **THEN** the system SHALL synthesize a placeholder state row (flagged `synthetic`) and log it
- **AND** SHALL NOT abort the poll on a foreign-key violation

### Requirement: First sync is silent; later assignments notify

The system SHALL seed the mirror on its first full sync **without** emitting notifications (the empty mirror would otherwise treat every already-assigned issue as new). The system SHALL record that a baseline is established and SHALL emit a desktop notification only for assignments observed after the baseline. A bulk re-assignment SHALL be coalesced into a single notification rather than one per issue. The same silent-baseline gate SHALL apply after a database reset.

#### Scenario: First sync of a pre-assigned workspace is silent

- **WHEN** the first full sync runs against a workspace where the viewer already has assigned issues
- **THEN** the system SHALL seed the mirror
- **AND** SHALL emit zero assignment notifications

#### Scenario: Post-baseline assignment notifies once

- **WHEN** after the baseline an issue is newly assigned to the viewer
- **THEN** the system SHALL emit one desktop notification

#### Scenario: Bulk assignment is coalesced

- **WHEN** many issues are assigned to the viewer between two polls
- **THEN** the system SHALL emit a single coalesced notification, not one per issue

### Requirement: Heavy sub-data fetched lazily

The system SHALL fetch comments lazily — on detail open or when an issue's `updatedAt` advanced — not on every poll, to stay within the complexity quota.

#### Scenario: Unchanged issue skips heavy refetch

- **WHEN** a poll observes an issue whose `updatedAt` is unchanged since last stored
- **THEN** the system SHALL NOT re-fetch that issue's comments on that poll
