//! Mirror read/write helpers over the `clickup_*` tables (migration 015).
//!
//! All helpers take a `&rusqlite::Connection` so the poller can run them
//! inside one transaction (`Transaction` derefs to `Connection`) and tests
//! can use an in-memory database. FK order matters with
//! `PRAGMA foreign_keys=ON`: spaces → folders → lists → statuses → tasks →
//! subdata, and a subtask's parent row must exist before the subtask.
//!
//! Every upsert with a `stale` column resets it to 0 — reappearance in a
//! fetch un-tombstones the row.

use anyhow::{Context, Result, bail};
use rusqlite::{Connection, OptionalExtension, params};

use super::model;

// ── Hierarchy upserts ──

pub fn upsert_space(conn: &Connection, space: &model::Space, synced_at: i64) -> Result<()> {
    conn.execute(
        "INSERT INTO clickup_spaces (id, name, synced_at) VALUES (?1, ?2, ?3) \
         ON CONFLICT(id) DO UPDATE SET name=?2, synced_at=?3",
        params![space.id, space.name, synced_at],
    )?;
    Ok(())
}

/// Scalar args so both real `Folder` payloads and the synthetic `hidden`
/// folder ref carried by folderless lists feed the same row shape.
pub fn upsert_folder(
    conn: &Connection,
    id: &str,
    space_id: &str,
    name: &str,
    hidden: bool,
) -> Result<()> {
    conn.execute(
        "INSERT INTO clickup_folders (id, space_id, name, hidden, stale) \
         VALUES (?1, ?2, ?3, ?4, 0) \
         ON CONFLICT(id) DO UPDATE SET space_id=?2, name=?3, hidden=?4, stale=0",
        params![id, space_id, name, hidden as i64],
    )?;
    Ok(())
}

/// The folder row referenced by `list.folder` must already exist (FK).
pub fn upsert_list(conn: &Connection, list: &model::List, space_id: &str) -> Result<()> {
    let folder_id = list.folder.as_ref().map(|f| f.id.as_str());
    conn.execute(
        "INSERT INTO clickup_lists (id, folder_id, space_id, name, stale) \
         VALUES (?1, ?2, ?3, ?4, 0) \
         ON CONFLICT(id) DO UPDATE SET folder_id=?2, space_id=?3, name=?4, stale=0",
        params![list.id, folder_id, space_id, list.name],
    )?;
    Ok(())
}

/// Some payloads omit the status id; the synthesized `{list_id}:{status}`
/// key keeps the row stable across syncs.
pub fn upsert_status(conn: &Connection, list_id: &str, status: &model::Status) -> Result<()> {
    let id = status
        .id
        .clone()
        .unwrap_or_else(|| format!("{list_id}:{}", status.status));
    conn.execute(
        "INSERT INTO clickup_statuses (id, list_id, status, color, orderindex, type) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6) \
         ON CONFLICT(id) DO UPDATE SET list_id=?2, status=?3, color=?4, orderindex=?5, type=?6",
        params![
            id,
            list_id,
            status.status,
            status.color,
            status.orderindex,
            status.status_type
        ],
    )?;
    Ok(())
}

// ── Tasks ──

/// Upsert a task from the all-tasks fetch. `parent_id` comes from the flat
/// `parent` field — the sole tree source. Caller responsibilities: the
/// task's list row exists (or was synthesized as a placeholder), and parents
/// are upserted before their subtasks (FK on `parent_id`).
pub fn upsert_task(conn: &Connection, task: &model::Task) -> Result<()> {
    let Some(list) = task.list.as_ref() else {
        bail!("task {} has no list reference", task.id);
    };
    let assignees_json = serde_json::to_string(&task.assignees).context("serializing assignees")?;
    let tags_json = serde_json::to_string(&task.tags).context("serializing tags")?;
    conn.execute(
        "INSERT INTO clickup_tasks (id, list_id, parent_id, name, text_content, status_name, \
            status_color, priority, assignees_json, tags_json, due_date, start_date, \
            date_created, date_updated, url, archived, stale) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, 0) \
         ON CONFLICT(id) DO UPDATE SET list_id=?2, parent_id=?3, name=?4, text_content=?5, \
            status_name=?6, status_color=?7, priority=?8, assignees_json=?9, tags_json=?10, \
            due_date=?11, start_date=?12, date_created=?13, date_updated=?14, url=?15, \
            archived=?16, stale=0",
        params![
            task.id,
            list.id,
            task.parent,
            task.name,
            task.text_content,
            task.status.as_ref().map(|s| s.status.as_str()),
            task.status.as_ref().and_then(|s| s.color.as_deref()),
            task.priority.as_ref().and_then(|p| p.priority.as_deref()),
            assignees_json,
            tags_json,
            task.due_date,
            task.start_date,
            task.date_created,
            task.date_updated,
            task.url,
            task.archived.unwrap_or(false) as i64,
        ],
    )?;
    Ok(())
}

// ── Sub-data ──

/// Defs are derived from task payloads (Decision 2); `scope_*` stays NULL
/// best-effort — payloads don't carry the def's scope.
pub fn upsert_custom_field_def(conn: &Connection, field: &model::CustomField) -> Result<()> {
    let type_config_json = field
        .type_config
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .context("serializing type_config")?;
    conn.execute(
        "INSERT INTO clickup_custom_field_defs (id, scope_level, scope_id, name, type, type_config_json) \
         VALUES (?1, NULL, NULL, ?2, ?3, ?4) \
         ON CONFLICT(id) DO UPDATE SET name=?2, type=?3, type_config_json=?4",
        params![field.id, field.name, field.field_type, type_config_json],
    )?;
    Ok(())
}

pub fn upsert_task_custom_value(
    conn: &Connection,
    task_id: &str,
    field: &model::CustomField,
) -> Result<()> {
    let value_json = field
        .value
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .context("serializing custom field value")?;
    conn.execute(
        "INSERT INTO clickup_task_custom_values (task_id, field_id, value_json) \
         VALUES (?1, ?2, ?3) \
         ON CONFLICT(task_id, field_id) DO UPDATE SET value_json=?3",
        params![task_id, field.id, value_json],
    )?;
    Ok(())
}

pub fn upsert_checklist(
    conn: &Connection,
    task_id: &str,
    checklist: &model::Checklist,
) -> Result<()> {
    conn.execute(
        "INSERT INTO clickup_checklists (id, task_id, name, orderindex) \
         VALUES (?1, ?2, ?3, ?4) \
         ON CONFLICT(id) DO UPDATE SET task_id=?2, name=?3, orderindex=?4",
        params![checklist.id, task_id, checklist.name, checklist.orderindex],
    )?;
    for item in &checklist.items {
        conn.execute(
            "INSERT INTO clickup_checklist_items (id, checklist_id, name, resolved, orderindex) \
             VALUES (?1, ?2, ?3, ?4, ?5) \
             ON CONFLICT(id) DO UPDATE SET checklist_id=?2, name=?3, resolved=?4, orderindex=?5",
            params![
                item.id,
                checklist.id,
                item.name,
                item.resolved.unwrap_or(false) as i64,
                item.orderindex
            ],
        )?;
    }
    Ok(())
}

pub fn upsert_comment(conn: &Connection, task_id: &str, comment: &model::Comment) -> Result<()> {
    let user_json = comment
        .user
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .context("serializing comment user")?;
    conn.execute(
        "INSERT INTO clickup_comments (id, task_id, user_json, text, date, resolved, reply_count) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7) \
         ON CONFLICT(id) DO UPDATE SET task_id=?2, user_json=?3, text=?4, date=?5, resolved=?6, \
            reply_count=?7",
        params![
            comment.id,
            task_id,
            user_json,
            comment.comment_text,
            comment.date,
            comment.resolved.unwrap_or(false) as i64,
            comment.reply_count.unwrap_or(0)
        ],
    )?;
    Ok(())
}

pub fn upsert_attachment(
    conn: &Connection,
    task_id: &str,
    attachment: &model::Attachment,
) -> Result<()> {
    conn.execute(
        "INSERT INTO clickup_attachments (id, task_id, title, url, mimetype, size, thumbnail_url) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7) \
         ON CONFLICT(id) DO UPDATE SET task_id=?2, title=?3, url=?4, mimetype=?5, size=?6, \
            thumbnail_url=?7",
        params![
            attachment.id,
            task_id,
            attachment.title,
            attachment.url,
            attachment.mimetype,
            attachment.size,
            attachment.best_thumbnail()
        ],
    )?;
    Ok(())
}

// ── Sync state (silent-first-sync gate) ──

#[derive(Debug, Clone, Default)]
pub struct SyncState {
    pub baseline_done: bool,
    pub last_full_sync: Option<i64>,
}

pub fn get_sync_state(conn: &Connection, team_id: &str) -> Result<Option<SyncState>> {
    let row = conn
        .query_row(
            "SELECT baseline_done, last_full_sync FROM clickup_sync_state WHERE team_id = ?1",
            [team_id],
            |r| {
                Ok(SyncState {
                    baseline_done: r.get::<_, i64>(0)? != 0,
                    last_full_sync: r.get(1)?,
                })
            },
        )
        .optional()?;
    Ok(row)
}

/// Teams with a sync-state row — row presence is what marks a team as
/// selected for syncing (Decision 9: never silently sync `teams[0]`).
pub fn list_sync_state_teams(conn: &Connection) -> Result<Vec<String>> {
    let mut stmt = conn.prepare("SELECT team_id FROM clickup_sync_state ORDER BY team_id")?;
    let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

/// The token user's id, cached across restarts. Any row carries it — the
/// token is account-scoped, not team-scoped — and `upsert_sync_state`
/// deliberately leaves the column alone.
pub fn cached_user_id(conn: &Connection) -> Result<Option<i64>> {
    let row = conn
        .query_row(
            "SELECT user_id FROM clickup_sync_state WHERE user_id IS NOT NULL LIMIT 1",
            [],
            |r| r.get::<_, i64>(0),
        )
        .optional()?;
    Ok(row)
}

pub fn set_cached_user_id(conn: &Connection, user_id: Option<i64>) -> Result<()> {
    conn.execute(
        "UPDATE clickup_sync_state SET user_id = ?1",
        params![user_id],
    )?;
    Ok(())
}

pub fn upsert_sync_state(conn: &Connection, team_id: &str, state: &SyncState) -> Result<()> {
    conn.execute(
        "INSERT INTO clickup_sync_state (team_id, baseline_done, last_full_sync) \
         VALUES (?1, ?2, ?3) \
         ON CONFLICT(team_id) DO UPDATE SET baseline_done=?2, last_full_sync=?3",
        params![team_id, state.baseline_done as i64, state.last_full_sync],
    )?;
    Ok(())
}

// ── Panel read model ──

#[derive(Debug, Clone, serde::Serialize)]
pub struct SpaceView {
    pub id: String,
    pub name: String,
}

/// Spaces for the panel's persistent Space selector ("Todos" + each Space).
pub fn read_spaces(conn: &Connection) -> Result<Vec<SpaceView>> {
    let mut stmt =
        conn.prepare("SELECT id, name FROM clickup_spaces ORDER BY name COLLATE NOCASE")?;
    let rows = stmt.query_map([], |r| {
        Ok(SpaceView {
            id: r.get(0)?,
            name: r.get(1)?,
        })
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

#[derive(Debug, Clone, Default)]
pub struct TaskFilter {
    /// Restrict to one Space; `None` = "Todos".
    pub space_id: Option<String>,
    /// Local assigned-to-me filter (the token user's id).
    pub assignee_id: Option<i64>,
    /// Tombstoned rows are hidden unless the panel asks for them.
    pub include_stale: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct AssigneeView {
    pub id: Option<i64>,
    pub username: Option<String>,
    pub color: Option<String>,
    pub initials: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct TagView {
    pub name: String,
    pub tag_fg: Option<String>,
    pub tag_bg: Option<String>,
}

/// Panel view-model row: task joined with its list (name + space scope).
#[derive(Debug, Clone, serde::Serialize)]
pub struct TaskView {
    pub id: String,
    pub name: String,
    pub list_id: String,
    pub list_name: String,
    pub space_id: String,
    pub parent_id: Option<String>,
    pub status_name: Option<String>,
    pub status_color: Option<String>,
    pub priority: Option<String>,
    pub assignees: Vec<AssigneeView>,
    pub tags: Vec<TagView>,
    pub due_date: Option<i64>,
    pub start_date: Option<i64>,
    pub date_updated: Option<i64>,
    pub url: Option<String>,
    pub archived: bool,
    pub stale: bool,
}

/// The panel's only read path — mirror-only, never a live API call.
pub fn read_tasks(conn: &Connection, filter: &TaskFilter) -> Result<Vec<TaskView>> {
    let mut stmt = conn.prepare(
        "SELECT t.id, t.name, t.list_id, l.name, l.space_id, t.parent_id, t.status_name, \
                t.status_color, t.priority, t.assignees_json, t.tags_json, t.due_date, \
                t.start_date, t.date_updated, t.url, t.archived, t.stale \
         FROM clickup_tasks t \
         JOIN clickup_lists l ON l.id = t.list_id \
         WHERE (?1 IS NULL OR l.space_id = ?1) \
           AND (?2 OR t.stale = 0) \
         ORDER BY t.date_updated DESC, t.id",
    )?;
    let rows = stmt.query_map(params![filter.space_id, filter.include_stale], |r| {
        Ok(TaskView {
            id: r.get(0)?,
            name: r.get(1)?,
            list_id: r.get(2)?,
            list_name: r.get(3)?,
            space_id: r.get(4)?,
            parent_id: r.get(5)?,
            status_name: r.get(6)?,
            status_color: r.get(7)?,
            priority: r.get(8)?,
            assignees: parse_assignees(r.get::<_, String>(9)?),
            tags: parse_tags(r.get::<_, String>(10)?),
            due_date: r.get(11)?,
            start_date: r.get(12)?,
            date_updated: r.get(13)?,
            url: r.get(14)?,
            archived: r.get::<_, i64>(15)? != 0,
            stale: r.get::<_, i64>(16)? != 0,
        })
    })?;

    let mut out = Vec::new();
    for row in rows {
        let view = row?;
        if let Some(me) = filter.assignee_id
            && !view.assignees.iter().any(|a| a.id == Some(me))
        {
            continue;
        }
        out.push(view);
    }
    Ok(out)
}

/// Pure payload→view mapper for EPHEMERAL fetch results (the show-closed
/// on-demand fetch). Those rows must never touch the mirror — writing them
/// would un-tombstone closed tasks and fight the next poll's reconcile — so
/// this maps straight from the API payload, no SQLite round-trip.
pub fn task_to_view(task: &model::Task) -> Option<TaskView> {
    let list = task.list.as_ref()?;
    Some(TaskView {
        id: task.id.clone(),
        name: task.name.clone(),
        list_id: list.id.clone(),
        list_name: list.name.clone(),
        space_id: task
            .space
            .as_ref()
            .map(|s| s.id.clone())
            .unwrap_or_default(),
        parent_id: task.parent.clone(),
        status_name: task.status.as_ref().map(|s| s.status.clone()),
        status_color: task.status.as_ref().and_then(|s| s.color.clone()),
        priority: task.priority.as_ref().and_then(|p| p.priority.clone()),
        assignees: task
            .assignees
            .iter()
            .map(|u| AssigneeView {
                id: u.id,
                username: u.username.clone(),
                color: u.color.clone(),
                initials: u.initials.clone(),
            })
            .collect(),
        tags: task
            .tags
            .iter()
            .map(|t| TagView {
                name: t.name.clone(),
                tag_fg: t.tag_fg.clone(),
                tag_bg: t.tag_bg.clone(),
            })
            .collect(),
        due_date: task.due_date,
        start_date: task.start_date,
        date_updated: task.date_updated,
        url: task.url.clone(),
        archived: task.archived.unwrap_or(false),
        stale: false,
    })
}

fn parse_assignees(json: String) -> Vec<AssigneeView> {
    let users: Vec<model::User> = serde_json::from_str(&json).unwrap_or_default();
    users
        .into_iter()
        .map(|u| AssigneeView {
            id: u.id,
            username: u.username,
            color: u.color,
            initials: u.initials,
        })
        .collect()
}

fn parse_tags(json: String) -> Vec<TagView> {
    let tags: Vec<model::Tag> = serde_json::from_str(&json).unwrap_or_default();
    tags.into_iter()
        .map(|t| TagView {
            name: t.name,
            tag_fg: t.tag_fg,
            tag_bg: t.tag_bg,
        })
        .collect()
}

// ── Status read (writable surface + closure picker) ──

/// Ordered status list for a List's workflow. Consumed by the write commands'
/// server-side validation and the closure prompt's status picker.
#[derive(Debug, Clone, serde::Serialize)]
pub struct StatusView {
    pub name: String,
    pub color: Option<String>,
    pub orderindex: Option<i64>,
}

/// Replace the cached status rows for one list with a freshly-resolved set.
/// `GET /list/{id}` returns the list's true workflow (statuses inherited from
/// the Space/Folder are only resolved there — the folder/folderless poll
/// endpoints return an empty `statuses[]` for non-overriding lists). The
/// resolved status ids are Space/Folder-scoped and shared across every
/// inheriting list, so they are stripped and re-synthesized per-list
/// (`{list_id}:{status}`); keeping the shared id would collide on the PK and
/// reassign another list's rows.
pub fn replace_list_statuses(
    conn: &Connection,
    list_id: &str,
    statuses: &[model::Status],
) -> Result<()> {
    conn.execute("DELETE FROM clickup_statuses WHERE list_id = ?1", [list_id])?;
    for status in statuses {
        let mut scoped = status.clone();
        scoped.id = None;
        upsert_status(conn, list_id, &scoped)?;
    }
    Ok(())
}

/// Mirror read over `clickup_statuses WHERE list_id = ?` ordered by
/// `orderindex`. Returns an empty vec when the list is unknown (the caller
/// decides whether that is a validation error).
pub fn read_list_statuses(conn: &Connection, list_id: &str) -> Result<Vec<StatusView>> {
    let mut stmt = conn.prepare(
        "SELECT status, color, orderindex FROM clickup_statuses \
         WHERE list_id = ?1 ORDER BY orderindex",
    )?;
    let rows = stmt.query_map([list_id], |r| {
        Ok(StatusView {
            name: r.get(0)?,
            color: r.get(1)?,
            orderindex: r.get(2)?,
        })
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

// ── Detail read helpers (floating detail module reads the mirror only) ──

#[derive(Debug, Clone, serde::Serialize)]
pub struct CommentView {
    pub id: String,
    pub user: Option<AssigneeView>,
    pub text: Option<String>,
    pub date: Option<i64>,
    pub resolved: bool,
    pub reply_count: i64,
}

pub fn read_comments(conn: &Connection, task_id: &str) -> Result<Vec<CommentView>> {
    let mut stmt = conn.prepare(
        "SELECT id, user_json, text, date, resolved, reply_count \
         FROM clickup_comments WHERE task_id = ?1 ORDER BY date",
    )?;
    let rows = stmt.query_map([task_id], |r| {
        let user_json: Option<String> = r.get(1)?;
        Ok(CommentView {
            id: r.get(0)?,
            user: user_json.and_then(|j| {
                serde_json::from_str::<model::User>(&j)
                    .ok()
                    .map(|u| AssigneeView {
                        id: u.id,
                        username: u.username,
                        color: u.color,
                        initials: u.initials,
                    })
            }),
            text: r.get(2)?,
            date: r.get(3)?,
            resolved: r.get::<_, i64>(4)? != 0,
            reply_count: r.get(5)?,
        })
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ChecklistItemView {
    pub id: String,
    pub name: Option<String>,
    pub resolved: bool,
    pub orderindex: Option<i64>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ChecklistView {
    pub id: String,
    pub name: Option<String>,
    pub orderindex: Option<i64>,
    pub items: Vec<ChecklistItemView>,
}

pub fn read_checklists(conn: &Connection, task_id: &str) -> Result<Vec<ChecklistView>> {
    let mut stmt = conn.prepare(
        "SELECT id, name, orderindex FROM clickup_checklists \
         WHERE task_id = ?1 ORDER BY orderindex",
    )?;
    let lists: Vec<(String, Option<String>, Option<i64>)> = stmt
        .query_map([task_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<std::result::Result<_, _>>()?;

    let mut item_stmt = conn.prepare(
        "SELECT id, name, resolved, orderindex FROM clickup_checklist_items \
         WHERE checklist_id = ?1 ORDER BY orderindex",
    )?;
    let mut out = Vec::new();
    for (id, name, orderindex) in lists {
        let items = item_stmt
            .query_map([&id], |r| {
                Ok(ChecklistItemView {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    resolved: r.get::<_, i64>(2)? != 0,
                    orderindex: r.get(3)?,
                })
            })?
            .collect::<std::result::Result<_, _>>()?;
        out.push(ChecklistView {
            id,
            name,
            orderindex,
            items,
        });
    }
    Ok(out)
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct AttachmentView {
    pub id: String,
    pub title: Option<String>,
    pub url: Option<String>,
    pub mimetype: Option<String>,
    pub size: Option<i64>,
    pub thumbnail_url: Option<String>,
}

pub fn read_attachments(conn: &Connection, task_id: &str) -> Result<Vec<AttachmentView>> {
    let mut stmt = conn.prepare(
        "SELECT id, title, url, mimetype, size, thumbnail_url \
         FROM clickup_attachments WHERE task_id = ?1 ORDER BY id",
    )?;
    let rows = stmt.query_map([task_id], |r| {
        Ok(AttachmentView {
            id: r.get(0)?,
            title: r.get(1)?,
            url: r.get(2)?,
            mimetype: r.get(3)?,
            size: r.get(4)?,
            thumbnail_url: r.get(5)?,
        })
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct CustomValueView {
    pub field_id: String,
    pub name: String,
    pub field_type: String,
    pub type_config_json: Option<String>,
    pub value_json: Option<String>,
}

/// Per-task custom values joined with their defs, rendered by type.
pub fn read_custom_values(conn: &Connection, task_id: &str) -> Result<Vec<CustomValueView>> {
    let mut stmt = conn.prepare(
        "SELECT v.field_id, d.name, d.type, d.type_config_json, v.value_json \
         FROM clickup_task_custom_values v \
         JOIN clickup_custom_field_defs d ON d.id = v.field_id \
         WHERE v.task_id = ?1 ORDER BY d.name",
    )?;
    let rows = stmt.query_map([task_id], |r| {
        Ok(CustomValueView {
            field_id: r.get(0)?,
            name: r.get(1)?,
            field_type: r.get(2)?,
            type_config_json: r.get(3)?,
            value_json: r.get(4)?,
        })
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

// ── Worked & closed marker (migration 019) ──

/// Record that a task was closed out from a session. Local-only and separate
/// from the ClickUp status (the task keeps whatever status ClickUp holds).
pub fn mark_closed_out(conn: &Connection, task_id: &str, closed_at: i64) -> Result<()> {
    conn.execute(
        "INSERT INTO clickup_closed_out (task_id, closed_at) VALUES (?1, ?2) \
         ON CONFLICT(task_id) DO UPDATE SET closed_at=?2",
        params![task_id, closed_at],
    )?;
    Ok(())
}

/// All task ids that were closed out from a session (for the panel marker).
pub fn read_closed_out(conn: &Connection) -> Result<Vec<String>> {
    let mut stmt = conn.prepare("SELECT task_id FROM clickup_closed_out")?;
    let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clickup::model::TasksPage;

    /// Migration 015 is self-contained (FKs only among clickup_* tables), so
    /// a fresh connection plus the single migration is a valid test bed. The
    /// full-chain application is covered in db.rs tests.
    fn mirror_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        conn.execute_batch(include_str!("../../migrations/015_clickup_mirror.sql"))
            .unwrap();
        conn
    }

    /// Seed the hierarchy + both fixture tasks (parent before subtask, as
    /// the poller must).
    fn seed_from_fixtures(conn: &Connection) -> TasksPage {
        let spaces: crate::clickup::model::SpacesResponse =
            serde_json::from_str(include_str!("fixtures/spaces.json")).unwrap();
        let folders: crate::clickup::model::FoldersResponse =
            serde_json::from_str(include_str!("fixtures/folders.json")).unwrap();
        let folderless: crate::clickup::model::ListsResponse =
            serde_json::from_str(include_str!("fixtures/folderless_lists.json")).unwrap();
        let page: TasksPage =
            serde_json::from_str(include_str!("fixtures/tasks_page.json")).unwrap();

        for space in &spaces.spaces {
            upsert_space(conn, space, 1_717_000_000).unwrap();
        }
        let space_id = &spaces.spaces[0].id;
        for folder in &folders.folders {
            upsert_folder(conn, &folder.id, space_id, &folder.name, false).unwrap();
            for list in &folder.lists {
                // Hierarchy lists omit `folder`; bind to the parent payload.
                let mut list = list.clone();
                list.folder = Some(crate::clickup::model::NamedRef {
                    id: folder.id.clone(),
                    name: folder.name.clone(),
                    hidden: Some(false),
                    access: None,
                });
                upsert_list(conn, &list, space_id).unwrap();
                for status in &list.statuses {
                    upsert_status(conn, &list.id, status).unwrap();
                }
            }
        }
        for list in &folderless.lists {
            let folder = list.folder.as_ref().unwrap();
            assert_eq!(folder.hidden, Some(true));
            upsert_folder(conn, &folder.id, space_id, &folder.name, true).unwrap();
            upsert_list(conn, list, space_id).unwrap();
        }
        for task in &page.tasks {
            upsert_task(conn, task).unwrap();
            for field in &task.custom_fields {
                upsert_custom_field_def(conn, field).unwrap();
                upsert_task_custom_value(conn, &task.id, field).unwrap();
            }
            for checklist in &task.checklists {
                upsert_checklist(conn, &task.id, checklist).unwrap();
            }
            for attachment in &task.attachments {
                upsert_attachment(conn, &task.id, attachment).unwrap();
            }
        }
        page
    }

    #[test]
    fn upsert_and_read_tasks_round_trip() {
        let conn = mirror_conn();
        seed_from_fixtures(&conn);

        let all = read_tasks(&conn, &TaskFilter::default()).unwrap();
        assert_eq!(all.len(), 2);

        let parent = all.iter().find(|t| t.id == "86ahwtc67").unwrap();
        assert_eq!(parent.list_name, "Sprint 23");
        assert_eq!(parent.space_id, "901312445262");
        assert_eq!(
            parent.status_name.as_deref(),
            Some("en revisión - dev (pr)")
        );
        assert_eq!(parent.status_color.as_deref(), Some("#f9d900"));
        assert_eq!(parent.priority.as_deref(), Some("high"));
        assert_eq!(parent.assignees.len(), 1);
        assert_eq!(parent.assignees[0].id, Some(81234567));
        assert_eq!(parent.assignees[0].username.as_deref(), Some("Felipe"));
        assert_eq!(parent.tags[0].name, "backend");
        assert_eq!(parent.due_date, Some(1_717_500_000_000));
        assert_eq!(parent.date_updated, Some(1_717_090_000_000));

        // Subtask tree from the flat parent field.
        let sub = all.iter().find(|t| t.id == "86ahwtd99").unwrap();
        assert_eq!(sub.parent_id.as_deref(), Some("86ahwtc67"));
    }

    #[test]
    fn read_tasks_filters_by_space_and_assignee() {
        let conn = mirror_conn();
        seed_from_fixtures(&conn);

        // Space filter: tasks live in Producto; the Ops space has none.
        let ops = read_tasks(
            &conn,
            &TaskFilter {
                space_id: Some("901312990001".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(ops.is_empty());

        // Assigned-to-me is a local filter over the mirror.
        let mine = read_tasks(
            &conn,
            &TaskFilter {
                assignee_id: Some(81234567),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(mine.len(), 1);
        assert_eq!(mine[0].id, "86ahwtc67");

        let nobody = read_tasks(
            &conn,
            &TaskFilter {
                assignee_id: Some(424242),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(nobody.is_empty());
    }

    #[test]
    fn upsert_untombstones_on_reappearance() {
        let conn = mirror_conn();
        let page = seed_from_fixtures(&conn);

        conn.execute(
            "UPDATE clickup_tasks SET stale = 1 WHERE id = '86ahwtc67'",
            [],
        )
        .unwrap();
        let visible = read_tasks(&conn, &TaskFilter::default()).unwrap();
        assert_eq!(visible.len(), 1, "tombstoned task hidden by default");
        let with_stale = read_tasks(
            &conn,
            &TaskFilter {
                include_stale: true,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(with_stale.len(), 2);

        // Reappearance in a later fetch resets stale=0.
        let task = page.tasks.iter().find(|t| t.id == "86ahwtc67").unwrap();
        upsert_task(&conn, task).unwrap();
        let visible = read_tasks(&conn, &TaskFilter::default()).unwrap();
        assert_eq!(visible.len(), 2);
        assert!(visible.iter().all(|t| !t.stale));
    }

    #[test]
    fn folder_and_list_upserts_untombstone_too() {
        let conn = mirror_conn();
        seed_from_fixtures(&conn);

        conn.execute("UPDATE clickup_folders SET stale = 1", [])
            .unwrap();
        conn.execute("UPDATE clickup_lists SET stale = 1", [])
            .unwrap();

        let folders: crate::clickup::model::FoldersResponse =
            serde_json::from_str(include_str!("fixtures/folders.json")).unwrap();
        let folder = &folders.folders[0];
        upsert_folder(&conn, &folder.id, "901312445262", &folder.name, false).unwrap();
        let mut list = folder.lists[0].clone();
        list.folder = Some(crate::clickup::model::NamedRef {
            id: folder.id.clone(),
            name: folder.name.clone(),
            hidden: Some(false),
            access: None,
        });
        upsert_list(&conn, &list, "901312445262").unwrap();

        let folder_stale: i64 = conn
            .query_row(
                "SELECT stale FROM clickup_folders WHERE id = ?1",
                [&folder.id],
                |r| r.get(0),
            )
            .unwrap();
        let list_stale: i64 = conn
            .query_row(
                "SELECT stale FROM clickup_lists WHERE id = ?1",
                [&list.id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(folder_stale, 0);
        assert_eq!(list_stale, 0);
    }

    #[test]
    fn subdata_round_trips() {
        let conn = mirror_conn();
        seed_from_fixtures(&conn);

        let comments: crate::clickup::model::CommentsResponse =
            serde_json::from_str(include_str!("fixtures/comments.json")).unwrap();
        for comment in &comments.comments {
            upsert_comment(&conn, "86ahwtc67", comment).unwrap();
        }
        let read = read_comments(&conn, "86ahwtc67").unwrap();
        assert_eq!(read.len(), 2);
        assert_eq!(read[0].reply_count, 2, "string reply_count normalized");
        assert_eq!(
            read[0].user.as_ref().and_then(|u| u.username.as_deref()),
            Some("Felipe")
        );

        let checklists = read_checklists(&conn, "86ahwtc67").unwrap();
        assert_eq!(checklists.len(), 1);
        assert_eq!(checklists[0].items.len(), 2);
        assert!(checklists[0].items[0].resolved);
        assert!(!checklists[0].items[1].resolved);

        let attachments = read_attachments(&conn, "86ahwtc67").unwrap();
        assert_eq!(attachments.len(), 1);
        assert!(
            attachments[0]
                .thumbnail_url
                .as_deref()
                .unwrap()
                .contains("medium")
        );

        let values = read_custom_values(&conn, "86ahwtc67").unwrap();
        assert_eq!(values.len(), 2);
        let severity = values.iter().find(|v| v.name == "Severity").unwrap();
        assert_eq!(severity.field_type, "drop_down");
        assert_eq!(severity.value_json.as_deref(), Some("1"));
        assert!(
            severity
                .type_config_json
                .as_deref()
                .unwrap()
                .contains("High")
        );
    }

    #[test]
    fn status_without_id_gets_synthesized_key() {
        let conn = mirror_conn();
        seed_from_fixtures(&conn);

        let status = model::Status {
            id: None,
            status: "blocked".into(),
            color: Some("#ff0000".into()),
            orderindex: Some(5),
            status_type: Some("custom".into()),
        };
        upsert_status(&conn, "901317020124", &status).unwrap();
        // Idempotent: re-upsert hits the same synthesized row.
        upsert_status(&conn, "901317020124", &status).unwrap();
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM clickup_statuses WHERE status = 'blocked'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn replace_list_statuses_scopes_shared_ids_and_replaces() {
        let conn = mirror_conn();
        seed_from_fixtures(&conn);

        // Sprint 23 was seeded with the folder fixture's statuses; replace wipes
        // them and installs the freshly-resolved set.
        assert!(
            !read_list_statuses(&conn, "901317020124")
                .unwrap()
                .is_empty()
        );

        // A Space-level status carries one id shared across every inheriting
        // list — applying it to two lists must not reassign rows between them.
        let shared = model::Status {
            id: Some("p_space_abc".into()),
            status: "in progress".into(),
            color: Some("#0091ff".into()),
            orderindex: Some(2),
            status_type: Some("custom".into()),
        };
        replace_list_statuses(&conn, "901317020124", std::slice::from_ref(&shared)).unwrap();
        replace_list_statuses(&conn, "901317010101", std::slice::from_ref(&shared)).unwrap();

        assert_eq!(read_list_statuses(&conn, "901317020124").unwrap().len(), 1);
        assert_eq!(read_list_statuses(&conn, "901317010101").unwrap().len(), 1);
        let total: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM clickup_statuses WHERE status = 'in progress'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(total, 2, "shared id did not collide across lists");
    }

    #[test]
    fn closed_out_marker_round_trips_and_is_idempotent() {
        let conn = mirror_conn();
        conn.execute_batch(include_str!("../../migrations/019_clickup_closed_out.sql"))
            .unwrap();
        assert!(read_closed_out(&conn).unwrap().is_empty());
        mark_closed_out(&conn, "task-a", 1000).unwrap();
        mark_closed_out(&conn, "task-b", 1001).unwrap();
        // Re-mark updates closed_at without duplicating the row.
        mark_closed_out(&conn, "task-a", 2000).unwrap();
        let mut ids = read_closed_out(&conn).unwrap();
        ids.sort();
        assert_eq!(ids, vec!["task-a".to_string(), "task-b".to_string()]);
    }

    #[test]
    fn sync_state_round_trips() {
        let conn = mirror_conn();
        assert!(get_sync_state(&conn, "9013000000").unwrap().is_none());

        upsert_sync_state(
            &conn,
            "9013000000",
            &SyncState {
                baseline_done: false,
                last_full_sync: None,
            },
        )
        .unwrap();
        let state = get_sync_state(&conn, "9013000000").unwrap().unwrap();
        assert!(!state.baseline_done);

        upsert_sync_state(
            &conn,
            "9013000000",
            &SyncState {
                baseline_done: true,
                last_full_sync: Some(1_717_100_000),
            },
        )
        .unwrap();
        let state = get_sync_state(&conn, "9013000000").unwrap().unwrap();
        assert!(state.baseline_done);
        assert_eq!(state.last_full_sync, Some(1_717_100_000));
    }

    #[test]
    fn task_without_known_list_violates_fk() {
        let conn = mirror_conn();
        seed_from_fixtures(&conn);

        let mut page: TasksPage =
            serde_json::from_str(include_str!("fixtures/tasks_page.json")).unwrap();
        let mut task = page.tasks.remove(0);
        task.id = "orphan1".into();
        task.parent = None;
        task.list = Some(model::NamedRef {
            id: "no-such-list".into(),
            name: "ghost".into(),
            hidden: None,
            access: None,
        });
        // The poller must synthesize a placeholder list first; the raw
        // helper surfaces the FK error instead of panicking.
        assert!(upsert_task(&conn, &task).is_err());
    }

    #[test]
    fn task_to_view_maps_payload_without_db() {
        let page: TasksPage =
            serde_json::from_str(include_str!("fixtures/tasks_page.json")).unwrap();
        let task = &page.tasks[0];
        let view = task_to_view(task).unwrap();
        assert_eq!(view.id, task.id);
        assert_eq!(view.list_id, "901317020124");
        assert_eq!(view.list_name, "Sprint 23");
        assert_eq!(view.status_name.as_deref(), Some("en revisión - dev (pr)"));
        assert_eq!(view.priority.as_deref(), Some("high"));
        assert_eq!(view.assignees.len(), 1);
        assert!(!view.stale);

        let mut no_list = task.clone();
        no_list.list = None;
        assert!(task_to_view(&no_list).is_none());
    }
}
