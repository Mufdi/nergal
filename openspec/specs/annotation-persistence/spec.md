---
status: archived
implemented: 2026-03-29
archived: 2026-04-04
files:
  - src-tauri/migrations/003_annotations.sql
  - src-tauri/migrations/004_annotation_highlight_source.sql
  - src-tauri/src/annotations.rs
  - src/stores/annotations.ts
  - src/lib/highlighter.ts
---

## Purpose

Persist plan annotations in SQLite with session-scoped CRUD, bidirectional Jotai sync, and web-highlighter integration for DOM position tracking.

## Implementation Notes

All requirements implemented. Schema revised from integer positions to HighlightSource `start_meta`/`end_meta` JSON (migration 004). Uses web-highlighter for DOM-based annotation positioning with `fromStore()` restore.

## ADDED Requirements

### Requirement: Annotations persist in SQLite
The system SHALL store annotations in a SQLite table with fields: id, session_id, type, target, content, start_meta, end_meta, created_at. Annotations SHALL be scoped to the session that created them.

#### Scenario: Annotation saved to database
- **WHEN** user creates an annotation (any type) via the toolbar
- **THEN** the annotation SHALL be persisted to the `annotations` table with the active session_id

#### Scenario: Annotations loaded on plan open
- **WHEN** user opens a plan for a session that has persisted annotations
- **THEN** all annotations for that session SHALL be loaded from SQLite and rendered as visual markers in the plan view

#### Scenario: Annotation deleted from database
- **WHEN** user removes an annotation from the sidebar list or clears all
- **THEN** the annotation(s) SHALL be deleted from the `annotations` table

#### Scenario: Annotations cascade on session delete
- **WHEN** a session is deleted
- **THEN** all annotations belonging to that session SHALL be deleted via CASCADE

### Requirement: Backend CRUD commands
The system SHALL expose Tauri commands for annotation CRUD: `save_annotation`, `get_annotations` (by session_id), `delete_annotation` (by id), `clear_annotations` (by session_id).

#### Scenario: Save annotation command
- **WHEN** frontend invokes `save_annotation` with type, target, content, start_meta, end_meta, and session_id
- **THEN** backend SHALL insert a row in `annotations` and return the generated id

#### Scenario: Get annotations command
- **WHEN** frontend invokes `get_annotations` with a session_id
- **THEN** backend SHALL return all annotations for that session ordered by created_at

### Requirement: Bidirectional sync between Jotai and SQLite
The system SHALL keep Jotai atoms and SQLite in sync. Creating an annotation SHALL write to both stores. Loading annotations on plan open SHALL populate Jotai from SQLite.

#### Scenario: Create syncs to backend
- **WHEN** user creates an annotation via the toolbar
- **THEN** the annotation SHALL appear in Jotai state immediately AND be persisted to SQLite asynchronously

#### Scenario: Load populates Jotai
- **WHEN** plan view mounts for a session
- **THEN** the system SHALL invoke `get_annotations` and populate the Jotai annotation map for that session

### Requirement: Annotation data model
The `annotations` table SHALL use the following schema:

| Field | Type | Constraints |
|-------|------|-------------|
| id | TEXT | PRIMARY KEY |
| session_id | TEXT | NOT NULL, FK -> sessions(id) ON DELETE CASCADE |
| type | TEXT | NOT NULL, CHECK IN ('comment','replace','delete','insert') |
| target | TEXT | NOT NULL |
| content | TEXT | NOT NULL DEFAULT '' |
| start_meta | TEXT | NOT NULL DEFAULT '{}' |
| end_meta | TEXT | NOT NULL DEFAULT '{}' |
| created_at | TEXT | NOT NULL DEFAULT datetime('now') |

#### Scenario: Schema matches spec
- **WHEN** migration runs
- **THEN** the `annotations` table SHALL exist with all fields, types, and constraints as specified above
