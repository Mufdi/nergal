//! Mirror read/write helpers over the `linear_*` tables (migration 023).
//!
//! All helpers take a `&rusqlite::Connection` so the poller can run them inside
//! one transaction (`Transaction` derefs to `Connection`) and tests can use an
//! in-memory database. FK order matters with `PRAGMA foreign_keys=ON`:
//! teams → workflow-states → labels → projects → cycles → users → issues →
//! issue-labels → comments.
//!
//! Tombstoning/eviction applies to **issues only**. Teams, states, labels,
//! projects, and cycles are upsert-only and GC'd only when unreferenced by a
//! live issue — so a synthesized placeholder FK row can never oscillate. Every
//! issue/state upsert resets `stale=0`: reappearance un-tombstones the row.

use std::collections::HashSet;

use anyhow::{Result, anyhow};
use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;

use super::model::{self, iso8601_to_epoch};

// ── Hierarchy + vocabulary upserts ──

pub fn upsert_team(
    conn: &Connection,
    id: &str,
    name: &str,
    key: &str,
    synced_at: i64,
) -> Result<()> {
    upsert_team_with_estimation(conn, id, name, key, None, synced_at)
}

pub fn upsert_team_with_estimation(
    conn: &Connection,
    id: &str,
    name: &str,
    key: &str,
    estimation_type: Option<&str>,
    synced_at: i64,
) -> Result<()> {
    // A real team upsert clears the synthetic flag (a stub becomes real).
    conn.execute(
        "INSERT INTO linear_teams (id, name, key, synthetic, estimation_type, synced_at) VALUES (?1, ?2, ?3, 0, ?4, ?5) \
         ON CONFLICT(id) DO UPDATE SET name=?2, key=?3, synthetic=0, estimation_type=?4, synced_at=?5",
        params![id, name, key, estimation_type, synced_at],
    )?;
    Ok(())
}

/// Insert a stub team for an unknown FK target. `INSERT OR IGNORE` so a real
/// team upserted later (or already present) wins; never overwrites real data.
pub fn ensure_team_placeholder(conn: &Connection, id: &str, synced_at: i64) -> Result<()> {
    conn.execute(
        "INSERT OR IGNORE INTO linear_teams (id, name, key, synthetic, synced_at) \
         VALUES (?1, ?2, ?3, 1, ?4)",
        params![id, format!("(team {id})"), "?", synced_at],
    )?;
    Ok(())
}

pub fn upsert_workflow_state(
    conn: &Connection,
    s: &model::WorkflowState,
    team_id: &str,
) -> Result<()> {
    conn.execute(
        "INSERT INTO linear_workflow_states (id, team_id, name, type, color, position, synthetic) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0) \
         ON CONFLICT(id) DO UPDATE SET team_id=?2, name=?3, type=?4, color=?5, position=?6, synthetic=0",
        params![s.id, team_id, s.name, s.state_type, s.color, s.position],
    )?;
    Ok(())
}

/// Stub state for an unknown FK target (its team must already exist — caller
/// ensures the team placeholder first).
pub fn ensure_state_placeholder(conn: &Connection, id: &str, team_id: &str) -> Result<()> {
    conn.execute(
        "INSERT OR IGNORE INTO linear_workflow_states (id, team_id, name, type, color, position, synthetic) \
         VALUES (?1, ?2, ?3, 'backlog', NULL, NULL, 1)",
        params![id, team_id, format!("(state {id})")],
    )?;
    Ok(())
}

/// Upsert-only; labels are never absence-tombstoned. `team_id` is null for
/// workspace labels.
pub fn upsert_label(conn: &Connection, l: &model::Label) -> Result<()> {
    let team_id = l.team.as_ref().map(|t| t.id.as_str());
    conn.execute(
        "INSERT INTO linear_labels (id, team_id, name, color) VALUES (?1, ?2, ?3, ?4) \
         ON CONFLICT(id) DO UPDATE SET team_id=?2, name=?3, color=?4",
        params![l.id, team_id, l.name, l.color],
    )?;
    Ok(())
}

pub fn upsert_project(conn: &Connection, p: &model::Project) -> Result<()> {
    conn.execute(
        "INSERT INTO linear_projects (id, name, state) VALUES (?1, ?2, ?3) \
         ON CONFLICT(id) DO UPDATE SET name=?2, state=?3",
        params![p.id, p.name, p.state],
    )?;
    Ok(())
}

pub fn upsert_cycle(conn: &Connection, c: &model::Cycle, team_id: &str) -> Result<()> {
    let starts = c.starts_at.as_deref().and_then(iso8601_to_epoch);
    let ends = c.ends_at.as_deref().and_then(iso8601_to_epoch);
    conn.execute(
        "INSERT INTO linear_cycles (id, team_id, number, name, starts_at, ends_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6) \
         ON CONFLICT(id) DO UPDATE SET team_id=?2, number=?3, name=?4, starts_at=?5, ends_at=?6",
        params![c.id, team_id, c.number, c.name, starts, ends],
    )?;
    Ok(())
}

pub fn upsert_user(conn: &Connection, u: &model::User) -> Result<()> {
    conn.execute(
        "INSERT INTO linear_users (id, name, display_name, email, avatar_url) \
         VALUES (?1, ?2, ?3, ?4, ?5) \
         ON CONFLICT(id) DO UPDATE SET name=?2, display_name=?3, email=?4, avatar_url=?5",
        params![u.id, u.name, u.display_name, u.email, u.avatar_url],
    )?;
    Ok(())
}

/// Full issue upsert (resets `stale=0`). The caller has already upserted the
/// inline assignee/project/cycle and ensured state/team FK targets exist.
/// `was_viewer_assigned` is computed by the caller (assignee == viewer).
pub fn upsert_issue(conn: &Connection, i: &model::Issue, was_viewer_assigned: bool) -> Result<()> {
    let state_id = i.state.as_ref().map(|s| s.id.as_str());
    let assignee_id = i.assignee.as_ref().map(|a| a.id.as_str());
    let project_id = i.project.as_ref().map(|p| p.id.as_str());
    let cycle_id = i.cycle.as_ref().map(|c| c.id.as_str());
    let parent_id = i.parent.as_ref().map(|p| p.id.as_str());
    let created = i.created_at.as_deref().and_then(iso8601_to_epoch);
    let updated = i.updated_at.as_deref().and_then(iso8601_to_epoch);
    let completed = i.completed_at.as_deref().and_then(iso8601_to_epoch);
    let due = i.due_date.as_deref().and_then(iso8601_to_epoch);
    conn.execute(
        "INSERT INTO linear_issues \
         (id, identifier, team_id, title, description, state_id, priority, estimate, \
          assignee_id, project_id, cycle_id, parent_id, was_viewer_assigned, \
          due_date, created_at, updated_at, completed_at, url, stale, stale_since) \
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,0,NULL) \
         ON CONFLICT(id) DO UPDATE SET \
          identifier=?2, team_id=?3, title=?4, description=?5, state_id=?6, priority=?7, \
          estimate=?8, assignee_id=?9, project_id=?10, cycle_id=?11, parent_id=?12, \
          was_viewer_assigned=?13, due_date=?14, created_at=?15, updated_at=?16, \
          completed_at=?17, url=?18, stale=0, stale_since=NULL",
        params![
            i.id,
            i.identifier,
            i.team.id,
            i.title,
            i.description,
            state_id,
            i.priority.unwrap_or(0),
            i.estimate,
            assignee_id,
            project_id,
            cycle_id,
            parent_id,
            was_viewer_assigned as i64,
            due,
            created,
            updated,
            completed,
            i.url,
        ],
    )?;
    Ok(())
}

/// Reconcile an issue's labels: delete the join rows no longer present, insert
/// the present ones. Label defs are upserted by the caller before this.
pub fn reconcile_issue_labels(
    conn: &Connection,
    issue_id: &str,
    label_ids: &[String],
) -> Result<()> {
    if label_ids.is_empty() {
        conn.execute(
            "DELETE FROM linear_issue_labels WHERE issue_id=?1",
            params![issue_id],
        )?;
        return Ok(());
    }
    // Delete joins not in the new set.
    let placeholders = std::iter::repeat_n("?", label_ids.len())
        .collect::<Vec<_>>()
        .join(",");
    let mut sql = format!(
        "DELETE FROM linear_issue_labels WHERE issue_id=? AND label_id NOT IN ({placeholders})"
    );
    let mut binds: Vec<&dyn rusqlite::ToSql> = Vec::with_capacity(label_ids.len() + 1);
    binds.push(&issue_id);
    for id in label_ids {
        binds.push(id);
    }
    conn.execute(&sql, binds.as_slice())?;
    // Insert present joins.
    for id in label_ids {
        conn.execute(
            "INSERT OR IGNORE INTO linear_issue_labels (issue_id, label_id) VALUES (?1, ?2)",
            params![issue_id, id],
        )?;
    }
    sql.clear();
    Ok(())
}

// ── Reconcile: tombstone / evict / GC ──

/// Completeness-gated tombstoning. Given the cycle's global fetched id set and
/// the window start, tombstone in-scope issues for the selected teams that are
/// absent from the fetch. Caller MUST only invoke this on a provably-complete
/// cycle. Returns the count tombstoned.
pub fn tombstone_absent_issues(
    conn: &Connection,
    selected_team_ids: &[String],
    window_start: i64,
    fetched_ids: &HashSet<String>,
    cycle_now: i64,
) -> Result<usize> {
    if selected_team_ids.is_empty() {
        return Ok(0);
    }
    // Candidate = selected team, in-window (updated_at > window_start), not
    // already stale, not viewer's. Absence from the fetch (global set) makes it
    // a tombstone. Moved-between-selected-teams issues survive because the
    // fetched set is global across all teams + branches.
    let team_ph = std::iter::repeat_n("?", selected_team_ids.len())
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!(
        "SELECT id FROM linear_issues \
         WHERE team_id IN ({team_ph}) AND updated_at > ? AND stale = 0 AND was_viewer_assigned = 0"
    );
    let mut binds: Vec<&dyn rusqlite::ToSql> = Vec::with_capacity(selected_team_ids.len() + 1);
    for t in selected_team_ids {
        binds.push(t);
    }
    binds.push(&window_start);
    let mut stmt = conn.prepare(&sql)?;
    let ids: Vec<String> = stmt
        .query_map(binds.as_slice(), |row| row.get::<_, String>(0))?
        .collect::<std::result::Result<_, _>>()?;
    let mut n = 0;
    for id in ids {
        if !fetched_ids.contains(&id) {
            conn.execute(
                "UPDATE linear_issues SET stale=1, stale_since=?2 WHERE id=?1",
                params![id, cycle_now],
            )?;
            n += 1;
        }
    }
    Ok(n)
}

/// Age-out eviction: hard-delete issues out of the window and not the viewer's
/// (childless first — a parent with live children is retained until its subtree
/// also ages out). Safe regardless of fetch completeness (out-of-scope rows are
/// not the fetch's authority). Returns the count evicted across all passes.
pub fn evict_aged_out(conn: &Connection, window_start: i64) -> Result<usize> {
    let mut total = 0;
    // Iterate: each pass removes currently-childless aged-out rows, exposing
    // their parents on the next pass. Bounded by tree depth.
    loop {
        let n = conn.execute(
            "DELETE FROM linear_issues \
             WHERE (updated_at IS NULL OR updated_at <= ?1) \
               AND was_viewer_assigned = 0 \
               AND id NOT IN (SELECT parent_id FROM linear_issues WHERE parent_id IS NOT NULL)",
            params![window_start],
        )?;
        total += n;
        if n == 0 {
            break;
        }
    }
    Ok(total)
}

/// GC label defs no longer referenced by any live issue join row.
pub fn gc_unreferenced_labels(conn: &Connection) -> Result<usize> {
    let n = conn.execute(
        "DELETE FROM linear_labels \
         WHERE id NOT IN (SELECT label_id FROM linear_issue_labels)",
        [],
    )?;
    Ok(n)
}

/// GC synthetic FK-target rows (teams/states) once unreferenced by any issue.
pub fn gc_unreferenced_synthetic(conn: &Connection) -> Result<()> {
    conn.execute(
        "DELETE FROM linear_workflow_states \
         WHERE synthetic = 1 AND id NOT IN (SELECT state_id FROM linear_issues WHERE state_id IS NOT NULL)",
        [],
    )?;
    conn.execute(
        "DELETE FROM linear_teams \
         WHERE synthetic = 1 AND id NOT IN (SELECT team_id FROM linear_issues)",
        [],
    )?;
    Ok(())
}

pub fn clear_viewer_assigned(conn: &Connection, issue_id: &str) -> Result<()> {
    conn.execute(
        "UPDATE linear_issues SET was_viewer_assigned=0 WHERE id=?1",
        params![issue_id],
    )?;
    Ok(())
}

pub fn evict_issue(conn: &Connection, issue_id: &str) -> Result<()> {
    conn.execute("DELETE FROM linear_issues WHERE id=?1", params![issue_id])?;
    Ok(())
}

/// Ids the mirror currently believes are the viewer's (drives the set-3 delta).
pub fn mine_issue_ids(conn: &Connection) -> Result<Vec<String>> {
    let mut stmt = conn.prepare("SELECT id FROM linear_issues WHERE was_viewer_assigned=1")?;
    let ids = stmt
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<std::result::Result<_, _>>()?;
    Ok(ids)
}

// ── Sync-state (single row, id=1) ──

#[derive(Debug, Clone)]
pub struct SyncState {
    pub baseline_done: bool,
    pub last_full_sync: Option<i64>,
    pub viewer_id: Option<String>,
    pub selected_team_ids: Vec<String>,
    pub key_generation: i64,
}

pub fn get_sync_state(conn: &Connection) -> Result<SyncState> {
    let row = conn
        .query_row(
            "SELECT baseline_done, last_full_sync, viewer_id, selected_team_ids_json, key_generation \
             FROM linear_sync_state WHERE id=1",
            [],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, Option<i64>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, i64>(4)?,
                ))
            },
        )
        .optional()?;
    let (baseline_done, last_full_sync, viewer_id, teams_json, key_generation) =
        row.ok_or_else(|| anyhow!("linear_sync_state row missing"))?;
    let selected_team_ids: Vec<String> = serde_json::from_str(&teams_json).unwrap_or_default();
    Ok(SyncState {
        baseline_done: baseline_done != 0,
        last_full_sync,
        viewer_id,
        selected_team_ids,
        key_generation,
    })
}

pub fn set_viewer_id(conn: &Connection, viewer_id: &str) -> Result<()> {
    conn.execute(
        "UPDATE linear_sync_state SET viewer_id=?1 WHERE id=1",
        params![viewer_id],
    )?;
    Ok(())
}

pub fn set_baseline_done(conn: &Connection, done: bool) -> Result<()> {
    conn.execute(
        "UPDATE linear_sync_state SET baseline_done=?1 WHERE id=1",
        params![done as i64],
    )?;
    Ok(())
}

pub fn set_last_full_sync(conn: &Connection, at: i64) -> Result<()> {
    conn.execute(
        "UPDATE linear_sync_state SET last_full_sync=?1 WHERE id=1",
        params![at],
    )?;
    Ok(())
}

pub fn set_selected_teams(conn: &Connection, team_ids: &[String]) -> Result<()> {
    let json = serde_json::to_string(team_ids).unwrap_or_else(|_| "[]".to_string());
    conn.execute(
        "UPDATE linear_sync_state SET selected_team_ids_json=?1 WHERE id=1",
        params![json],
    )?;
    Ok(())
}

// ── Multi-workspace (linear-mirror-enhancements) ──

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceRow {
    pub org_id: String,
    pub name: String,
    pub url_key: Option<String>,
    pub active: bool,
}

/// Active workspace org id (the one the mirror reflects), or `None` when no
/// workspace is selected.
pub fn get_active_org(conn: &Connection) -> Result<Option<String>> {
    let v = conn.query_row(
        "SELECT active_org_id FROM linear_sync_state WHERE id=1",
        [],
        |row| row.get::<_, Option<String>>(0),
    )?;
    Ok(v)
}

pub fn set_active_org(conn: &Connection, org_id: Option<&str>) -> Result<()> {
    conn.execute(
        "UPDATE linear_sync_state SET active_org_id=?1 WHERE id=1",
        params![org_id],
    )?;
    Ok(())
}

/// List stored workspaces (non-secret metadata) with the active one flagged.
pub fn list_workspaces(conn: &Connection) -> Result<Vec<WorkspaceRow>> {
    let active = get_active_org(conn)?;
    let mut stmt =
        conn.prepare("SELECT org_id, name, url_key FROM linear_workspaces ORDER BY added_at")?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
            ))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows
        .into_iter()
        .map(|(org_id, name, url_key)| WorkspaceRow {
            active: active.as_deref() == Some(org_id.as_str()),
            org_id,
            name,
            url_key,
        })
        .collect())
}

pub fn upsert_workspace(
    conn: &Connection,
    org_id: &str,
    name: &str,
    url_key: Option<&str>,
    added_at: i64,
) -> Result<()> {
    conn.execute(
        "INSERT INTO linear_workspaces (org_id, name, url_key, added_at) VALUES (?1,?2,?3,?4) \
         ON CONFLICT(org_id) DO UPDATE SET name=excluded.name, url_key=excluded.url_key",
        params![org_id, name, url_key, added_at],
    )?;
    Ok(())
}

pub fn remove_workspace(conn: &Connection, org_id: &str) -> Result<()> {
    conn.execute(
        "DELETE FROM linear_workspaces WHERE org_id=?1",
        params![org_id],
    )?;
    Ok(())
}

pub fn workspace_count(conn: &Connection) -> Result<i64> {
    let n = conn.query_row("SELECT COUNT(*) FROM linear_workspaces", [], |r| r.get(0))?;
    Ok(n)
}

pub fn workspace_exists(conn: &Connection, org_id: &str) -> Result<bool> {
    let n: i64 = conn.query_row(
        "SELECT COUNT(*) FROM linear_workspaces WHERE org_id=?1",
        params![org_id],
        |r| r.get(0),
    )?;
    Ok(n > 0)
}

pub fn current_key_generation(conn: &Connection) -> Result<i64> {
    let g = conn.query_row(
        "SELECT key_generation FROM linear_sync_state WHERE id=1",
        [],
        |row| row.get::<_, i64>(0),
    )?;
    Ok(g)
}

/// `set_key` epoch bump: increments the generation, wipes the issue mirror,
/// clears the viewer id + team selection + baseline. Idempotent shape.
pub fn bump_generation_and_wipe(conn: &Connection) -> Result<()> {
    wipe_mirror(conn)?;
    conn.execute(
        "UPDATE linear_sync_state SET key_generation = key_generation + 1, \
         viewer_id=NULL, selected_team_ids_json='[]', baseline_done=0, last_full_sync=NULL \
         WHERE id=1",
        [],
    )?;
    Ok(())
}

/// Delete all mirrored content (issues cascade to joins/comments). Leaves the
/// single sync_state row.
pub fn wipe_mirror(conn: &Connection) -> Result<()> {
    conn.execute("DELETE FROM linear_issues", [])?;
    conn.execute("DELETE FROM linear_workflow_states", [])?;
    conn.execute("DELETE FROM linear_labels", [])?;
    conn.execute("DELETE FROM linear_projects", [])?;
    conn.execute("DELETE FROM linear_cycles", [])?;
    conn.execute("DELETE FROM linear_users", [])?;
    conn.execute("DELETE FROM linear_teams", [])?;
    Ok(())
}

// ── Panel read views ──

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LabelView {
    pub id: String,
    pub name: String,
    pub color: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IssueView {
    pub id: String,
    pub identifier: Option<String>,
    pub team_id: String,
    pub title: String,
    /// Full markdown body — the floating detail renders it (gated). Kept on the
    /// list view so the detail reads from the same atom; Linear bodies are modest.
    pub description: Option<String>,
    pub priority: i64,
    pub estimate: Option<f64>,
    pub state_id: Option<String>,
    pub state_name: Option<String>,
    pub state_type: Option<String>,
    pub state_color: Option<String>,
    /// Workflow position — orders state groups within their type.
    pub state_position: Option<f64>,
    pub assignee_id: Option<String>,
    pub assignee_name: Option<String>,
    pub assignee_avatar_url: Option<String>,
    pub project_id: Option<String>,
    pub project_name: Option<String>,
    pub cycle_id: Option<String>,
    pub cycle_name: Option<String>,
    pub parent_id: Option<String>,
    pub created_at: Option<i64>,
    pub updated_at: Option<i64>,
    pub due_date: Option<i64>,
    pub url: Option<String>,
    pub stale: bool,
    pub labels: Vec<LabelView>,
    /// Team estimation scheme: notUsed | exponential | fibonacci | linear | tShirt.
    /// Drives the estimate label in the detail (tShirt → XS/S/M/L/XL/XXL).
    pub estimation_type: Option<String>,
}

#[derive(Debug, Default)]
pub struct IssueFilter {
    pub team_id: Option<String>,
    pub include_stale: bool,
}

/// Read the panel view-model from the mirror (joined state/assignee/project +
/// labels). One query for issues, one for labels, stitched in Rust.
pub fn read_issues(conn: &Connection, filter: &IssueFilter) -> Result<Vec<IssueView>> {
    let mut sql = String::from(
        "SELECT i.id, i.identifier, i.team_id, i.title, i.description, i.estimate, i.priority, \
                i.state_id, s.name, s.type, s.color, s.position, \
                i.assignee_id, COALESCE(u.display_name, u.name), u.avatar_url, \
                i.project_id, p.name, i.cycle_id, c.name, i.parent_id, \
                i.created_at, i.updated_at, i.due_date, i.url, i.stale, \
                t.estimation_type \
         FROM linear_issues i \
         LEFT JOIN linear_workflow_states s ON s.id = i.state_id \
         LEFT JOIN linear_users u ON u.id = i.assignee_id \
         LEFT JOIN linear_projects p ON p.id = i.project_id \
         LEFT JOIN linear_cycles c ON c.id = i.cycle_id \
         LEFT JOIN linear_teams t ON t.id = i.team_id \
         WHERE 1=1",
    );
    let mut binds: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
    if !filter.include_stale {
        sql.push_str(" AND i.stale = 0");
    }
    if let Some(team) = &filter.team_id {
        sql.push_str(" AND i.team_id = ?");
        binds.push(Box::new(team.clone()));
    }
    sql.push_str(" ORDER BY i.updated_at DESC");

    let bind_refs: Vec<&dyn rusqlite::ToSql> = binds.iter().map(|b| b.as_ref()).collect();
    let mut stmt = conn.prepare(&sql)?;
    let mut views: Vec<IssueView> = stmt
        .query_map(bind_refs.as_slice(), |row| {
            Ok(IssueView {
                id: row.get(0)?,
                identifier: row.get(1)?,
                team_id: row.get(2)?,
                title: row.get(3)?,
                description: row.get(4)?,
                estimate: row.get(5)?,
                priority: row.get(6)?,
                state_id: row.get(7)?,
                state_name: row.get(8)?,
                state_type: row.get(9)?,
                state_color: row.get(10)?,
                state_position: row.get(11)?,
                assignee_id: row.get(12)?,
                assignee_name: row.get(13)?,
                assignee_avatar_url: row.get(14)?,
                project_id: row.get(15)?,
                project_name: row.get(16)?,
                cycle_id: row.get(17)?,
                cycle_name: row.get(18)?,
                parent_id: row.get(19)?,
                created_at: row.get(20)?,
                updated_at: row.get(21)?,
                due_date: row.get(22)?,
                url: row.get(23)?,
                stale: row.get::<_, i64>(24)? != 0,
                labels: Vec::new(),
                estimation_type: row.get(25)?,
            })
        })?
        .collect::<std::result::Result<_, _>>()?;

    // Labels for the visible set, stitched by issue id.
    let mut label_stmt = conn.prepare(
        "SELECT il.issue_id, l.id, l.name, l.color \
         FROM linear_issue_labels il JOIN linear_labels l ON l.id = il.label_id",
    )?;
    let mut by_issue: std::collections::HashMap<String, Vec<LabelView>> =
        std::collections::HashMap::new();
    let rows = label_stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            LabelView {
                id: row.get(1)?,
                name: row.get(2)?,
                color: row.get(3)?,
            },
        ))
    })?;
    for r in rows {
        let (issue_id, label) = r?;
        by_issue.entry(issue_id).or_default().push(label);
    }
    for v in &mut views {
        if let Some(labels) = by_issue.remove(&v.id) {
            v.labels = labels;
        }
    }
    Ok(views)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamView {
    pub id: String,
    pub name: String,
    pub key: String,
}

/// Teams for the panel selector (non-synthetic only).
pub fn read_teams(conn: &Connection) -> Result<Vec<TeamView>> {
    let mut stmt =
        conn.prepare("SELECT id, name, key FROM linear_teams WHERE synthetic = 0 ORDER BY name")?;
    let teams = stmt
        .query_map([], |row| {
            Ok(TeamView {
                id: row.get(0)?,
                name: row.get(1)?,
                key: row.get(2)?,
            })
        })?
        .collect::<std::result::Result<_, _>>()?;
    Ok(teams)
}

// ── Writeback reads (linear-writeback) ──

/// View returned by `read_team_states` — the state picker source.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowStateView {
    pub id: String,
    pub name: String,
    pub state_type: String,
    pub color: Option<String>,
    pub position: Option<f64>,
}

/// Non-synthetic workflow states for a team, ordered by position.
///
/// Used by the state picker and the server-side state-validation helper.
/// Synthetic placeholder rows (`synthetic=1`, inserted by `ensure_state_placeholder`
/// for FK resolution) are always excluded — a placeholder id offered to the
/// user or passed through validation could drive `issueUpdate` with a non-real
/// state id.
pub fn read_team_states(conn: &Connection, team_id: &str) -> Result<Vec<WorkflowStateView>> {
    let mut stmt = conn.prepare(
        "SELECT id, name, type, color, position \
         FROM linear_workflow_states \
         WHERE team_id = ?1 AND synthetic = 0 \
         ORDER BY position",
    )?;
    let states = stmt
        .query_map([team_id], |row| {
            Ok(WorkflowStateView {
                id: row.get(0)?,
                name: row.get(1)?,
                state_type: row.get(2)?,
                color: row.get(3)?,
                position: row.get(4)?,
            })
        })?
        .collect::<std::result::Result<_, _>>()?;
    Ok(states)
}

/// View returned by `read_team_cycles` — the cycle picker source.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CycleView {
    pub id: String,
    pub name: Option<String>,
    pub number: Option<i64>,
}

/// Cycles for a team, newest first (highest number). Feeds the cycle picker.
pub fn read_team_cycles(conn: &Connection, team_id: &str) -> Result<Vec<CycleView>> {
    let mut stmt = conn.prepare(
        "SELECT id, name, number FROM linear_cycles \
         WHERE team_id = ?1 \
         ORDER BY number DESC",
    )?;
    let cycles = stmt
        .query_map([team_id], |row| {
            Ok(CycleView {
                id: row.get(0)?,
                name: row.get(1)?,
                number: row.get(2)?,
            })
        })?
        .collect::<std::result::Result<_, _>>()?;
    Ok(cycles)
}

/// Validate that `state_id` belongs to the non-synthetic states of `team_id`.
/// Returns `Ok(())` when valid, `Err` when not found or synthetic.
pub fn validate_state_for_team(conn: &Connection, team_id: &str, state_id: &str) -> Result<()> {
    use rusqlite::OptionalExtension;
    let found: Option<i64> = conn
        .query_row(
            "SELECT 1 FROM linear_workflow_states \
             WHERE id = ?1 AND team_id = ?2 AND synthetic = 0",
            [state_id, team_id],
            |r| r.get(0),
        )
        .optional()?;
    if found.is_some() {
        Ok(())
    } else {
        anyhow::bail!("state {state_id:?} is not a valid non-synthetic state for team {team_id:?}")
    }
}

/// Retrieve the `team_id` for a given issue from the mirror.
///
/// Used by the state-validation helper when only the `issue_id` is available.
pub fn get_issue_team_id(conn: &Connection, issue_id: &str) -> Result<String> {
    use rusqlite::OptionalExtension;
    let team_id: Option<String> = conn
        .query_row(
            "SELECT team_id FROM linear_issues WHERE id = ?1",
            [issue_id],
            |r| r.get(0),
        )
        .optional()?;
    team_id.ok_or_else(|| anyhow::anyhow!("issue {issue_id:?} not found in the mirror"))
}

/// Retrieve the current `state_id` and `assignee_id` for an issue from the
/// post-reconcile mirror.  Returns `None` when the issue is absent (evicted /
/// tombstoned / out of window).
/// `(state_id, assignee_id, cycle_id)` for an issue — the writeback echo fields.
pub type IssueWriteFields = (Option<String>, Option<String>, Option<String>);

pub fn get_issue_write_fields(
    conn: &Connection,
    issue_id: &str,
) -> Result<Option<IssueWriteFields>> {
    use rusqlite::OptionalExtension;
    let row = conn
        .query_row(
            "SELECT state_id, assignee_id, cycle_id FROM linear_issues WHERE id = ?1",
            [issue_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?;
    Ok(row)
}

/// Insert a comment into the `linear_comments` mirror after a successful post.
///
/// `ON CONFLICT DO UPDATE` makes the upsert idempotent so a poll echo that
/// brings the same id never duplicates the row.
pub fn upsert_comment(
    conn: &Connection,
    issue_id: &str,
    comment_id: &str,
    author_id: Option<&str>,
    body: &str,
    created_at_secs: i64,
) -> Result<()> {
    let user_json = author_id.map(|id| format!(r#"{{"id":"{id}"}}"#));
    conn.execute(
        "INSERT INTO linear_comments (id, issue_id, user_json, body, created_at) \
         VALUES (?1, ?2, ?3, ?4, ?5) \
         ON CONFLICT(id) DO UPDATE SET \
          issue_id=?2, user_json=?3, body=?4, created_at=?5",
        rusqlite::params![comment_id, issue_id, user_json, body, created_at_secs],
    )?;
    Ok(())
}

// ── Worked & closed marker (linear-writeback) ──

/// Record that an issue was closed out from a session.
///
/// `ON CONFLICT DO UPDATE` so re-closing the same issue (e.g. after a crash
/// and reopen) is safe: it just refreshes the timestamp.
pub fn mark_closed_out(conn: &Connection, issue_id: &str, closed_at: i64) -> Result<()> {
    conn.execute(
        "INSERT INTO linear_closed_out (issue_id, closed_at) VALUES (?1, ?2) \
         ON CONFLICT(issue_id) DO UPDATE SET closed_at=?2",
        rusqlite::params![issue_id, closed_at],
    )?;
    Ok(())
}

/// All issue ids that were closed out from a session.
pub fn read_closed_out(conn: &Connection) -> Result<Vec<String>> {
    let mut stmt = conn.prepare("SELECT issue_id FROM linear_closed_out ORDER BY closed_at")?;
    let ids = stmt
        .query_map([], |r| r.get(0))?
        .collect::<std::result::Result<_, _>>()?;
    Ok(ids)
}

/// Remove the worked-closed marker for an issue, allowing it to be re-closed later.
pub fn unmark_closed_out(conn: &Connection, issue_id: &str) -> Result<()> {
    conn.execute(
        "DELETE FROM linear_closed_out WHERE issue_id = ?1",
        rusqlite::params![issue_id],
    )?;
    Ok(())
}

// ── Projects ──

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectView {
    pub id: String,
    pub name: String,
}

/// All projects in the mirror, ordered by name — feeds the panel project-select.
pub fn read_projects(conn: &Connection) -> Result<Vec<ProjectView>> {
    let mut stmt = conn.prepare("SELECT id, name FROM linear_projects ORDER BY name")?;
    let rows = stmt
        .query_map([], |r| {
            Ok(ProjectView {
                id: r.get(0)?,
                name: r.get(1)?,
            })
        })?
        .collect::<std::result::Result<_, _>>()?;
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::linear::model;

    fn mem_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        conn.execute_batch(include_str!("../../migrations/023_linear_mirror.sql"))
            .unwrap();
        conn.execute_batch(include_str!(
            "../../migrations/027_linear_estimation_type.sql"
        ))
        .unwrap();
        conn
    }

    fn issue(id: &str, team: &str, updated: &str) -> model::Issue {
        serde_json::from_value(serde_json::json!({
            "id": id, "identifier": "ENG-1", "title": "T",
            "priority": 2, "team": { "id": team }, "updatedAt": updated
        }))
        .unwrap()
    }

    fn seed_team(conn: &Connection, id: &str) {
        upsert_team(conn, id, "Eng", "ENG", 1).unwrap();
    }

    #[test]
    fn issue_round_trips_and_reads() {
        let conn = mem_db();
        seed_team(&conn, "t1");
        upsert_issue(&conn, &issue("i1", "t1", "2026-06-16T00:00:00Z"), true).unwrap();
        let views = read_issues(&conn, &IssueFilter::default()).unwrap();
        assert_eq!(views.len(), 1);
        assert_eq!(views[0].id, "i1");
        assert_eq!(views[0].priority, 2);
    }

    #[test]
    fn out_of_scope_parent_does_not_fk_abort() {
        // parent_id is a plain column: a child referencing an unfetched parent
        // inserts fine and renders as a tolerant root.
        let conn = mem_db();
        seed_team(&conn, "t1");
        let mut child = issue("child", "t1", "2026-06-16T00:00:00Z");
        child.parent = Some(model::IdRef {
            id: "ghost-parent".into(),
        });
        upsert_issue(&conn, &child, false).unwrap();
        let views = read_issues(&conn, &IssueFilter::default()).unwrap();
        assert_eq!(views[0].parent_id.as_deref(), Some("ghost-parent"));
    }

    #[test]
    fn deleting_issue_cascades_labels_and_comments() {
        let conn = mem_db();
        seed_team(&conn, "t1");
        upsert_issue(&conn, &issue("i1", "t1", "2026-06-16T00:00:00Z"), false).unwrap();
        upsert_label(
            &conn,
            &serde_json::from_value(serde_json::json!({"id":"l1","name":"bug"})).unwrap(),
        )
        .unwrap();
        reconcile_issue_labels(&conn, "i1", &["l1".to_string()]).unwrap();
        conn.execute(
            "INSERT INTO linear_comments (id, issue_id, body) VALUES ('c1','i1','hi')",
            [],
        )
        .unwrap();
        evict_issue(&conn, "i1").unwrap();
        let join_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM linear_issue_labels", [], |r| r.get(0))
            .unwrap();
        let comment_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM linear_comments", [], |r| r.get(0))
            .unwrap();
        assert_eq!(join_count, 0);
        assert_eq!(comment_count, 0);
    }

    #[test]
    fn gc_unreferenced_labels_drops_only_orphans() {
        let conn = mem_db();
        seed_team(&conn, "t1");
        upsert_issue(&conn, &issue("i1", "t1", "2026-06-16T00:00:00Z"), false).unwrap();
        for id in ["used", "orphan"] {
            upsert_label(
                &conn,
                &serde_json::from_value(serde_json::json!({"id":id,"name":id})).unwrap(),
            )
            .unwrap();
        }
        reconcile_issue_labels(&conn, "i1", &["used".to_string()]).unwrap();
        let removed = gc_unreferenced_labels(&conn).unwrap();
        assert_eq!(removed, 1);
        let remaining: i64 = conn
            .query_row("SELECT COUNT(*) FROM linear_labels", [], |r| r.get(0))
            .unwrap();
        assert_eq!(remaining, 1);
    }

    #[test]
    fn tombstone_only_absent_in_scope_non_mine() {
        let conn = mem_db();
        seed_team(&conn, "t1");
        // in-window, not mine, will be absent → tombstoned
        upsert_issue(&conn, &issue("gone", "t1", "2026-06-16T00:00:00Z"), false).unwrap();
        // in-window, mine → never tombstoned by absence
        upsert_issue(&conn, &issue("mine", "t1", "2026-06-16T00:00:00Z"), true).unwrap();
        // present → survives
        upsert_issue(&conn, &issue("here", "t1", "2026-06-16T00:00:00Z"), false).unwrap();
        let window_start = 1_000_000; // both updated_at are well above
        let mut fetched = HashSet::new();
        fetched.insert("here".to_string());
        let n = tombstone_absent_issues(
            &conn,
            &["t1".to_string()],
            window_start,
            &fetched,
            2_000_000_000,
        )
        .unwrap();
        assert_eq!(n, 1);
        let stale: i64 = conn
            .query_row("SELECT stale FROM linear_issues WHERE id='gone'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(stale, 1);
        let mine_stale: i64 = conn
            .query_row("SELECT stale FROM linear_issues WHERE id='mine'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(mine_stale, 0);
    }

    #[test]
    fn evict_aged_out_keeps_mine_and_children() {
        let conn = mem_db();
        seed_team(&conn, "t1");
        let window_start = 1_770_000_000; // ~2026; the 2020 rows are aged out
        // aged out, not mine → evicted
        upsert_issue(&conn, &issue("old", "t1", "2020-01-01T00:00:00Z"), false).unwrap();
        // aged out but mine → kept
        upsert_issue(&conn, &issue("oldmine", "t1", "2020-01-01T00:00:00Z"), true).unwrap();
        let n = evict_aged_out(&conn, window_start).unwrap();
        assert_eq!(n, 1);
        let remaining: i64 = conn
            .query_row("SELECT COUNT(*) FROM linear_issues", [], |r| r.get(0))
            .unwrap();
        assert_eq!(remaining, 1);
    }

    #[test]
    fn bump_generation_wipes_and_resets() {
        let conn = mem_db();
        seed_team(&conn, "t1");
        upsert_issue(&conn, &issue("i1", "t1", "2026-06-16T00:00:00Z"), true).unwrap();
        set_viewer_id(&conn, "viewer-old").unwrap();
        set_baseline_done(&conn, true).unwrap();
        set_selected_teams(&conn, &["t1".to_string()]).unwrap();
        let g0 = current_key_generation(&conn).unwrap();
        bump_generation_and_wipe(&conn).unwrap();
        let st = get_sync_state(&conn).unwrap();
        assert_eq!(st.key_generation, g0 + 1);
        assert!(st.viewer_id.is_none());
        assert!(!st.baseline_done);
        assert!(st.selected_team_ids.is_empty());
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM linear_issues", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn placeholder_state_lets_issue_insert_then_real_wins() {
        let conn = mem_db();
        seed_team(&conn, "t1");
        ensure_state_placeholder(&conn, "s1", "t1").unwrap();
        let mut iss = issue("i1", "t1", "2026-06-16T00:00:00Z");
        iss.state = Some(model::IdRef { id: "s1".into() });
        upsert_issue(&conn, &iss, false).unwrap();
        // real state arrives → synthetic cleared
        let real: model::WorkflowState = serde_json::from_value(serde_json::json!({
            "id":"s1","name":"In Progress","type":"started","color":"#fff"
        }))
        .unwrap();
        upsert_workflow_state(&conn, &real, "t1").unwrap();
        let (name, synth): (String, i64) = conn
            .query_row(
                "SELECT name, synthetic FROM linear_workflow_states WHERE id='s1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(name, "In Progress");
        assert_eq!(synth, 0);
    }

    fn mem_db_ws() -> Connection {
        let conn = mem_db();
        conn.execute_batch(include_str!("../../migrations/025_linear_workspaces.sql"))
            .unwrap();
        conn
    }

    #[test]
    fn workspaces_crud_and_active() {
        let conn = mem_db_ws();
        assert_eq!(workspace_count(&conn).unwrap(), 0);
        assert!(get_active_org(&conn).unwrap().is_none());

        upsert_workspace(&conn, "org-a", "Red Ribbon", Some("redribbon"), 1).unwrap();
        upsert_workspace(&conn, "org-b", "Other", None, 2).unwrap();
        set_active_org(&conn, Some("org-a")).unwrap();

        let list = list_workspaces(&conn).unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].org_id, "org-a"); // added_at order
        assert!(list[0].active);
        assert!(!list[1].active);
        assert_eq!(list[0].url_key.as_deref(), Some("redribbon"));

        // Upsert updates name, keeps the row.
        upsert_workspace(&conn, "org-a", "Red Ribbon 2", Some("redribbon"), 1).unwrap();
        assert_eq!(workspace_count(&conn).unwrap(), 2);
        assert_eq!(list_workspaces(&conn).unwrap()[0].name, "Red Ribbon 2");

        remove_workspace(&conn, "org-b").unwrap();
        assert_eq!(workspace_count(&conn).unwrap(), 1);
    }

    #[test]
    fn wipe_preserves_workspaces_and_sync_state() {
        let conn = mem_db_ws();
        seed_team(&conn, "t1");
        upsert_issue(&conn, &issue("i1", "t1", "2026-06-16T00:00:00Z"), true).unwrap();
        upsert_workspace(&conn, "org-a", "Red Ribbon", None, 1).unwrap();
        set_active_org(&conn, Some("org-a")).unwrap();

        // The switch path: bump epoch + wipe. Workspaces + sync_state survive.
        bump_generation_and_wipe(&conn).unwrap();

        assert_eq!(
            read_issues(&conn, &IssueFilter::default()).unwrap().len(),
            0
        );
        assert_eq!(workspace_count(&conn).unwrap(), 1);
        // active_org_id is independent of the epoch bump (which clears viewer/teams).
        assert_eq!(get_active_org(&conn).unwrap().as_deref(), Some("org-a"));
        assert_eq!(current_key_generation(&conn).unwrap(), 1);
    }

    fn mem_db_wb() -> Connection {
        let conn = mem_db();
        // 025 adds workspace tables (no FK to sessions); skip 024 which
        // ALTER TABLEs sessions (absent in the minimal in-memory schema).
        // 027 (estimation_type ALTER TABLE) is already applied by mem_db().
        conn.execute_batch(include_str!("../../migrations/025_linear_workspaces.sql"))
            .unwrap();
        conn.execute_batch(include_str!("../../migrations/026_linear_closed_out.sql"))
            .unwrap();
        conn
    }

    // 1.2 read_team_states excludes synthetic rows and is ordered by position
    #[test]
    fn read_team_states_excludes_synthetic_and_is_ordered() {
        let conn = mem_db();
        seed_team(&conn, "t1");
        // Real state at position 2.
        let real_a: model::WorkflowState = serde_json::from_value(serde_json::json!({
            "id": "s-a", "name": "In Progress", "type": "started", "color": "#0f0", "position": 2.0
        }))
        .unwrap();
        // Real state at position 1.
        let real_b: model::WorkflowState = serde_json::from_value(serde_json::json!({
            "id": "s-b", "name": "Backlog", "type": "backlog", "color": "#888", "position": 1.0
        }))
        .unwrap();
        upsert_workflow_state(&conn, &real_a, "t1").unwrap();
        upsert_workflow_state(&conn, &real_b, "t1").unwrap();
        // Synthetic placeholder — must be absent from the picker.
        ensure_state_placeholder(&conn, "s-synth", "t1").unwrap();

        let states = read_team_states(&conn, "t1").unwrap();
        assert_eq!(states.len(), 2, "synthetic must be excluded");
        assert_eq!(states[0].id, "s-b", "position order: s-b first");
        assert_eq!(states[1].id, "s-a");
    }

    // 1.3 validate_state_for_team rejects synthetic and unknown ids
    #[test]
    fn validate_state_rejects_synthetic_and_unknown() {
        let conn = mem_db();
        seed_team(&conn, "t1");
        let real: model::WorkflowState = serde_json::from_value(serde_json::json!({
            "id": "s-real", "name": "Done", "type": "completed", "color": "#0f0", "position": 1.0
        }))
        .unwrap();
        upsert_workflow_state(&conn, &real, "t1").unwrap();
        ensure_state_placeholder(&conn, "s-synth", "t1").unwrap();

        assert!(validate_state_for_team(&conn, "t1", "s-real").is_ok());
        assert!(
            validate_state_for_team(&conn, "t1", "s-synth").is_err(),
            "synthetic id must be rejected"
        );
        assert!(
            validate_state_for_team(&conn, "t1", "s-nonexistent").is_err(),
            "unknown id must be rejected"
        );
    }

    // 4.4 upsert_comment inserts once and is idempotent on echo
    #[test]
    fn upsert_comment_inserts_once_and_echo_is_idempotent() {
        let conn = mem_db();
        seed_team(&conn, "t1");
        upsert_issue(&conn, &issue("i1", "t1", "2026-06-16T00:00:00Z"), false).unwrap();

        upsert_comment(&conn, "i1", "c1", Some("author-uuid"), "hello", 1_000).unwrap();
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM linear_comments WHERE id='c1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);

        // Echo: upsert same id again — must remain at 1.
        upsert_comment(&conn, "i1", "c1", Some("author-uuid"), "hello", 1_000).unwrap();
        let count2: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM linear_comments WHERE id='c1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count2, 1, "echo upsert must not duplicate the row");
    }

    // 5.2 mark_closed_out and read_closed_out round-trip
    #[test]
    fn closed_out_round_trip() {
        let conn = mem_db_wb();
        assert!(read_closed_out(&conn).unwrap().is_empty());
        mark_closed_out(&conn, "issue-a", 1000).unwrap();
        mark_closed_out(&conn, "issue-b", 1001).unwrap();
        let mut ids = read_closed_out(&conn).unwrap();
        ids.sort();
        assert_eq!(ids, vec!["issue-a", "issue-b"]);

        // Re-closing the same issue updates the timestamp (idempotent).
        mark_closed_out(&conn, "issue-a", 2000).unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM linear_closed_out", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 2, "re-close must not duplicate the row");
    }

    // 8.3 unmark_closed_out removes the marker
    #[test]
    fn unmark_closed_out_removes_marker() {
        let conn = mem_db_wb();
        mark_closed_out(&conn, "issue-x", 5000).unwrap();
        let ids = read_closed_out(&conn).unwrap();
        assert_eq!(ids, vec!["issue-x"]);

        unmark_closed_out(&conn, "issue-x").unwrap();
        assert!(
            read_closed_out(&conn).unwrap().is_empty(),
            "unmark must delete the row"
        );

        // Unmarking a non-existent id is a no-op (not an error).
        unmark_closed_out(&conn, "no-such-issue").unwrap();
    }

    // 8.4 read_projects returns rows ordered by name
    #[test]
    fn read_projects_ordered_by_name() {
        let conn = mem_db_wb();
        conn.execute(
            "INSERT INTO linear_projects (id, name) VALUES ('p1','Zebra'),('p2','Alpha')",
            [],
        )
        .unwrap();

        let projects = read_projects(&conn).unwrap();
        assert_eq!(projects.len(), 2);
        assert_eq!(projects[0].id, "p2");
        assert_eq!(projects[0].name, "Alpha");
        assert_eq!(projects[1].id, "p1");
        assert_eq!(projects[1].name, "Zebra");
    }

    // 8.4 read_projects returns empty when no rows
    #[test]
    fn read_projects_empty() {
        let conn = mem_db_wb();
        assert!(read_projects(&conn).unwrap().is_empty());
    }

    // 8.2 estimation_type persisted on upsert_team_with_estimation + surfaced on IssueView
    #[test]
    fn estimation_type_persisted_and_surfaced() {
        let conn = mem_db_wb();
        upsert_team_with_estimation(&conn, "t-ts", "Tee", "TEE", Some("tShirt"), 1).unwrap();

        // Verify the column was written.
        let val: Option<String> = conn
            .query_row(
                "SELECT estimation_type FROM linear_teams WHERE id='t-ts'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(val.as_deref(), Some("tShirt"));

        // Seed a minimal issue and verify estimation_type flows through read_issues.
        upsert_issue(&conn, &issue("i-ts", "t-ts", "2026-06-18T00:00:00Z"), false).unwrap();
        let views = read_issues(
            &conn,
            &IssueFilter {
                team_id: Some("t-ts".into()),
                include_stale: false,
            },
        )
        .unwrap();
        assert_eq!(views.len(), 1);
        assert_eq!(views[0].estimation_type.as_deref(), Some("tShirt"));
    }

    // 8.2 upsert_team_with_estimation updates estimation_type on conflict
    #[test]
    fn estimation_type_updates_on_conflict() {
        let conn = mem_db_wb();
        upsert_team_with_estimation(&conn, "t-u", "Up", "UP", Some("fibonacci"), 1).unwrap();
        upsert_team_with_estimation(&conn, "t-u", "Up", "UP", Some("tShirt"), 2).unwrap();

        let val: Option<String> = conn
            .query_row(
                "SELECT estimation_type FROM linear_teams WHERE id='t-u'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(val.as_deref(), Some("tShirt"));
    }
}
