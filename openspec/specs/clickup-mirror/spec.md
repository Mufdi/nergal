# clickup-mirror Specification

## Purpose
TBD - created by archiving change clickup-sync. Update Purpose after archive.
## Requirements
### Requirement: Personal API token stored in the OS keyring

The system SHALL store the user's ClickUp Personal API token in the OS keyring (secret-service on Linux) under service `cluihud`, account `clickup-token`. When the keyring is unavailable, the system SHALL fall back to `~/.config/cluihud/clickup.toml` created **atomically with mode `0600`** (no write-then-chmod window) and SHALL surface to the UI that the token is stored on disk. The token SHALL be read only into the REST client, never logged or included in any error string, and never returned to the frontend. The system SHALL expose commands to set, clear, and validate the token; validation SHALL call `GET /user` (auth header `Authorization: <token>`, no `Bearer`) and return the resolved user, not the token.

#### Scenario: Token stored in keyring on the happy path

- **WHEN** the user sets a token and the OS keyring is available
- **THEN** the token SHALL be written to the keyring under service `cluihud` / account `clickup-token`
- **AND** no plaintext token SHALL be written to disk
- **AND** validation SHALL return the resolved ClickUp user

#### Scenario: Keyring-absent fallback is atomic and disclosed

- **WHEN** the user sets a token and the keyring is unavailable
- **THEN** the token file SHALL be created atomically with mode `0600`
- **AND** the UI SHALL indicate the token is stored on disk
- **AND** at no point SHALL the file exist readable at a wider mode

#### Scenario: Token never leaks to logs or errors

- **WHEN** a request fails or an error is logged
- **THEN** the error string and log output SHALL NOT contain the token

#### Scenario: Transient keyring failure is not a missing token

- **WHEN** reading the token fails with a transient keyring error (e.g. a D-Bus failure), as opposed to a definitive no-entry
- **THEN** the system SHALL surface a transient error state, NOT a "no token configured" state
- **AND** the poll loop SHALL retry on subsequent cycles instead of terminating

### Requirement: Structure-agnostic local mirror of the ClickUp hierarchy

The system SHALL maintain a local SQLite mirror modeling the generic `Space → Folder → List → Task → Subtask` tree with real foreign keys, such that adding a Folder, List, status, or custom field in ClickUp is absorbed as data without a code change or migration. Statuses SHALL be stored per-List (not an enum). Custom fields SHALL be stored as definitions plus per-task values keyed by field id, rendered by type; definitions SHALL be derived from task payloads with `scope_*` as nullable best-effort. Folderless Lists SHALL be represented under a folder row flagged `hidden`. Subtasks SHALL be tasks with a non-null `parent_id`, and the subtask tree SHALL be built solely from `parent_id` (populated from the flat `parent` field of the all-tasks fetch), not from the nested detail array. The panel SHALL read its view-model exclusively from the mirror.

#### Scenario: Folderless list mirrored under a hidden folder

- **WHEN** the mirror syncs a Space whose Lists are folderless
- **THEN** each List SHALL reference a folder row flagged `hidden = 1`
- **AND** a real Folder added later SHALL land in the same table flagged `hidden = 0` without a migration

#### Scenario: New per-list status appears without code change

- **WHEN** a List gains a new custom status in ClickUp
- **THEN** the next sync SHALL insert a new `clickup_statuses` row for that List
- **AND** the panel SHALL render it without a code or schema change

#### Scenario: Subtask tree from parent_id

- **WHEN** the all-tasks fetch returns subtasks flat with a `parent`
- **THEN** the mirror SHALL set each subtask's `parent_id` from that `parent`
- **AND** SHALL build the subtask tree from `parent_id`, not from the nested detail array

#### Scenario: Panel renders from the mirror, not live calls

- **WHEN** the panel displays tasks
- **THEN** it SHALL read from the mirror
- **AND** SHALL NOT issue a live ClickUp API call per render

### Requirement: REST client with rate-limit and last-page pagination

The system SHALL provide a typed ClickUp API v2 client over the shared `reqwest` client covering the read endpoints needed for the mirror. The client SHALL honor `429 Retry-After` with bounded backoff. The client SHALL paginate the filtered-tasks endpoint by the response `last_page` flag, NOT by row count — because the endpoint filters after the page slice, a page may return fewer than the page size while more pages exist.

#### Scenario: Rate-limit backoff

- **WHEN** the API responds `429` with a `Retry-After` header
- **THEN** the client SHALL wait the indicated interval before retrying
- **AND** SHALL give up after a bounded number of retries

#### Scenario: Short page is not the last page

- **WHEN** a task page returns fewer rows than the page size but `last_page` is false
- **THEN** the client SHALL request the next page
- **AND** SHALL stop only when `last_page` is true

### Requirement: Poll scope is all tasks per Space

The system SHALL poll **all** tasks per Space (no assignee filter), so that un-assignment is detectable and the panel can offer multiple local views. Assigned-to-me SHALL be a local filter over the mirror, not a server-side poll filter. When `get_teams` returns more than one team, the system SHALL let the user choose which team(s) to sync rather than silently syncing the first.

#### Scenario: Un-assignment is detectable

- **WHEN** a task the user was assigned is un-assigned in ClickUp
- **THEN** because the poll fetches all tasks, the system SHALL observe the assignee change
- **AND** SHALL update the mirror rather than leaving the task shown as assigned

#### Scenario: Multiple teams prompt a choice

- **WHEN** the account has more than one team
- **THEN** the system SHALL let the user choose which to sync
- **AND** SHALL NOT silently sync only the first team

#### Scenario: Token or team change reflects immediately

- **WHEN** the user sets a token or selects a team
- **THEN** the system SHALL emit a `syncing` status immediately, without waiting for the first network cycle (the persisted snapshot is stale until then)
- **AND** selecting a different team SHALL tombstone the previous team's mirror contents

### Requirement: Atomic reconcile with absent-task tombstoning

The system SHALL refresh the mirror on a configurable interval by fetching everything a cycle needs first, then committing in a single SQLite transaction with upsert order `spaces → folders → lists → statuses → tasks → subdata`, so the panel never reads a torn mid-reconcile state and a mid-cycle network failure commits nothing. A task whose `list_id` is unknown to the fetched hierarchy SHALL synthesize a placeholder list row and log it, never abort the poll on a foreign-key violation. A complete per-Space all-tasks fetch SHALL be authoritative: mirror tasks for that Space absent from the fetch SHALL be tombstoned (`stale = 1`); hierarchy rows absent from a fetch SHALL be tombstoned, not hard-deleted mid-iteration; any row reappearing in a later fetch SHALL be un-tombstoned (`stale = 0`). After each committed reconcile the system SHALL emit `clickup:changed`. The system SHALL NOT use webhooks.

#### Scenario: Reconcile commits atomically

- **WHEN** a poll cycle reconciles the mirror
- **THEN** all upserts and tombstones SHALL commit in one transaction
- **AND** a concurrent panel read SHALL NOT observe lists tombstoned while their tasks are not yet updated

#### Scenario: Closed or deleted task is tombstoned

- **WHEN** a task present in the mirror is absent from a complete Space fetch (closed, deleted, or moved away)
- **THEN** the system SHALL mark it `stale = 1` and record when it went stale
- **AND** SHALL un-tombstone it if it reappears in a later fetch

#### Scenario: Garbage collection only removes childless tombstones

- **WHEN** tombstoned rows are garbage-collected after their retention window
- **THEN** only rows with no live child rows SHALL be deleted (a cascade would take live children with them)
- **AND** a tombstoned row with live descendants SHALL be retained until its subtree is also stale

#### Scenario: Unknown list id does not crash the poll

- **WHEN** a fetched task references a `list_id` absent from the fetched hierarchy
- **THEN** the system SHALL synthesize a placeholder list row and log it
- **AND** SHALL NOT abort the poll on a foreign-key violation

### Requirement: First sync is silent; later assignments notify

The system SHALL seed the mirror on its first full sync **without** emitting notifications (the empty mirror would otherwise treat every already-assigned task as new). The system SHALL record that a baseline is established and SHALL emit a desktop notification only for assignments observed after the baseline. A bulk re-assignment SHALL be coalesced into a single notification rather than one per task. The same silent-baseline gate SHALL apply after a database reset.

#### Scenario: First sync of a pre-assigned workspace is silent

- **WHEN** the first full sync runs against a workspace where the user already has assigned tasks
- **THEN** the system SHALL seed the mirror
- **AND** SHALL emit zero assignment notifications

#### Scenario: Post-baseline assignment notifies once

- **WHEN** after the baseline a task is newly assigned to the token's user
- **THEN** the system SHALL emit one desktop notification

#### Scenario: Bulk assignment is coalesced

- **WHEN** many tasks are assigned to the user between two polls
- **THEN** the system SHALL emit a single coalesced notification, not one per task

### Requirement: Heavy sub-data fetched lazily

The system SHALL fetch comments and checklists lazily — on detail open or when a task's `date_updated` advanced — not on every poll.

#### Scenario: Unchanged task skips heavy refetch

- **WHEN** a poll observes a task whose `date_updated` is unchanged since last stored
- **THEN** the system SHALL NOT re-fetch that task's comments and checklists on that poll

