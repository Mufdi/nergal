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
    // A real team upsert clears the synthetic flag (a stub becomes real).
    conn.execute(
        "INSERT INTO linear_teams (id, name, key, synthetic, synced_at) VALUES (?1, ?2, ?3, 0, ?4) \
         ON CONFLICT(id) DO UPDATE SET name=?2, key=?3, synthetic=0, synced_at=?4",
        params![id, name, key, synced_at],
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
    pub priority: i64,
    pub state_id: Option<String>,
    pub state_name: Option<String>,
    pub state_type: Option<String>,
    pub state_color: Option<String>,
    pub assignee_id: Option<String>,
    pub assignee_name: Option<String>,
    pub project_id: Option<String>,
    pub project_name: Option<String>,
    pub cycle_id: Option<String>,
    pub parent_id: Option<String>,
    pub updated_at: Option<i64>,
    pub url: Option<String>,
    pub stale: bool,
    pub labels: Vec<LabelView>,
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
        "SELECT i.id, i.identifier, i.team_id, i.title, i.priority, \
                i.state_id, s.name, s.type, s.color, \
                i.assignee_id, COALESCE(u.display_name, u.name), \
                i.project_id, p.name, i.cycle_id, i.parent_id, i.updated_at, i.url, i.stale \
         FROM linear_issues i \
         LEFT JOIN linear_workflow_states s ON s.id = i.state_id \
         LEFT JOIN linear_users u ON u.id = i.assignee_id \
         LEFT JOIN linear_projects p ON p.id = i.project_id \
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
                priority: row.get(4)?,
                state_id: row.get(5)?,
                state_name: row.get(6)?,
                state_type: row.get(7)?,
                state_color: row.get(8)?,
                assignee_id: row.get(9)?,
                assignee_name: row.get(10)?,
                project_id: row.get(11)?,
                project_name: row.get(12)?,
                cycle_id: row.get(13)?,
                parent_id: row.get(14)?,
                updated_at: row.get(15)?,
                url: row.get(16)?,
                stale: row.get::<_, i64>(17)? != 0,
                labels: Vec::new(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::linear::model;

    fn mem_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        conn.execute_batch(include_str!("../../migrations/023_linear_mirror.sql"))
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
}
