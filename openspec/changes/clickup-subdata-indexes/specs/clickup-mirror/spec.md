# clickup-mirror

Index-backed subdata reads for the ClickUp mirror's per-task checklist/attachment/comment queries.

## ADDED Requirements

### Requirement: Subdata reads are index-backed

The mirror schema SHALL provide indexes on `clickup_checklists(task_id)`, `clickup_attachments(task_id)`, and `clickup_comments(task_id)`, so per-task subdata lookups (panel count subqueries, detail comment reads) are index lookups rather than full-table scans.

#### Scenario: panel refresh uses the indexes

- **GIVEN** a mirror with N tasks and populated subdata tables
- **WHEN** `read_tasks` runs (panel mount or `clickup:changed` poll refresh)
- **THEN** `EXPLAIN QUERY PLAN` shows the checklist/attachment count subqueries using `idx_clickup_checklists_task` / `idx_clickup_attachments_task` (no `SCAN clickup_checklists` / `SCAN clickup_attachments`)

#### Scenario: migration is idempotent

- **WHEN** the migration runs on a database where the indexes already exist
- **THEN** it completes without error (`CREATE INDEX IF NOT EXISTS`)
