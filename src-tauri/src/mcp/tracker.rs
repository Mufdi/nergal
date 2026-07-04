//! Read-only tracker/PR/git MCP tools (`mcp-tracker-tools`, building on the
//! archived `mcp-expose-tracker-tasks` design).
//!
//! D1: `list_tracker_tasks`/`get_tracker_task` unify ClickUp + Linear behind a
//! `source` field + shared columns + a `fields` bag for tracker-specifics.
//! D2: mirror-only reads; every tracker response carries `mirror_updated_at`
//! (per tracker) instead of triggering a live poll.
//! D4: `list_tracker_tasks` returns capped summary rows ordered by
//! `date_updated DESC`; `get_tracker_task` returns detail + subdata counts +
//! comments trimmed to a budget.
//!
//! `get_pr_status`/`get_git_status` reuse the exact read fns the ship-flow
//! commands (`commands::ship_pr`, `commands::git`) and status bar already
//! call. Those shell out to local `git`/`gh` on every read — there is no
//! persisted PR/git cache in this codebase to read instead (see the
//! divergence note in the change's task report). This is still read-only (no
//! mutation) and matches "the panels' read path" literally.

use anyhow::{Context, Result};
use rusqlite::{Connection, OptionalExtension};
use serde_json::{Value, json};

use super::DaemonContext;
use crate::clickup::mirror as clickup_mirror;
use crate::linear::mirror as linear_mirror;

const DEFAULT_LIST_LIMIT: u32 = 50;
const MAX_LIST_LIMIT: u32 = 200;
/// D4 "trimmed to a budget": cap comment count and per-comment text length so
/// a chatty task/issue can't blow out the tool response.
const COMMENT_BUDGET: usize = 10;
const COMMENT_TEXT_BUDGET: usize = 1000;

fn trim_text(s: &str) -> String {
    if s.chars().count() <= COMMENT_TEXT_BUDGET {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(COMMENT_TEXT_BUDGET).collect();
        format!("{truncated}…")
    }
}

#[derive(Debug, Clone, Default)]
pub struct ListTrackerTasksFilter {
    /// Restrict to one tracker; `None` = both, unified.
    pub source: Option<String>,
    pub limit: Option<u32>,
}

pub fn parse_list_filter(args: &Value) -> ListTrackerTasksFilter {
    ListTrackerTasksFilter {
        source: args
            .get("source")
            .and_then(|v| v.as_str())
            .map(String::from),
        limit: args.get("limit").and_then(|v| v.as_u64()).map(|v| v as u32),
    }
}

fn clickup_mirror_updated_at(conn: &Connection) -> Option<i64> {
    conn.query_row(
        "SELECT MAX(last_full_sync) FROM clickup_sync_state",
        [],
        |r| r.get::<_, Option<i64>>(0),
    )
    .ok()
    .flatten()
}

fn linear_mirror_updated_at(conn: &Connection) -> Option<i64> {
    linear_mirror::get_sync_state(conn)
        .ok()
        .and_then(|s| s.last_full_sync)
}

fn clickup_priority_view(p: &str) -> String {
    p.to_string()
}

fn linear_priority_label(p: i64) -> Option<String> {
    match p {
        1 => Some("urgent".into()),
        2 => Some("high".into()),
        3 => Some("medium".into()),
        4 => Some("low".into()),
        _ => None,
    }
}

fn clickup_row_to_unified(t: clickup_mirror::TaskView) -> Value {
    json!({
        "source": "clickup",
        "id": t.id,
        "title": t.name,
        "url": t.url,
        "status": t.status_name,
        "priority": t.priority.as_deref().map(clickup_priority_view),
        "assignees": t.assignees.iter().filter_map(|a| a.username.clone()).collect::<Vec<_>>(),
        "date_created": t.date_created,
        "date_updated": t.date_updated,
        "fields": {
            "list_name": t.list_name,
            "space_id": t.space_id,
            "custom_id": t.custom_id,
            "status_color": t.status_color,
            "status_type": t.status_type,
            "due_date": t.due_date,
            "start_date": t.start_date,
            "tags": t.tags.iter().map(|tag| tag.name.clone()).collect::<Vec<_>>(),
            "has_description": t.has_description,
            "subtask_count": t.subtask_count,
            "checklist_count": t.checklist_count,
            "attachment_count": t.attachment_count,
        },
    })
}

fn linear_row_to_unified(i: linear_mirror::IssueView) -> Value {
    json!({
        "source": "linear",
        "id": i.id,
        "title": i.title,
        "url": i.url,
        "status": i.state_name,
        "priority": linear_priority_label(i.priority),
        "assignees": i.assignee_name.iter().cloned().collect::<Vec<_>>(),
        "date_created": i.created_at,
        "date_updated": i.updated_at,
        "fields": {
            "identifier": i.identifier,
            "team_id": i.team_id,
            "project_name": i.project_name,
            "cycle_name": i.cycle_name,
            "due_date": i.due_date,
            "labels": i.labels.iter().map(|l| l.name.clone()).collect::<Vec<_>>(),
            "estimate": i.estimate,
        },
    })
}

/// `list_tracker_tasks(filter?)` (D1/D4): unified summary rows over the
/// ClickUp + Linear mirrors, capped, ordered `date_updated DESC`. Mirror-only
/// — no live tracker API call reachable from this path (uses
/// `clickup_mirror::read_tasks` / `linear_mirror::read_issues`, the same pure
/// reads the panels use, never `client::*`).
pub fn list_tracker_tasks(ctx: &DaemonContext, filter: &ListTrackerTasksFilter) -> Result<Value> {
    let limit = filter
        .limit
        .unwrap_or(DEFAULT_LIST_LIMIT)
        .clamp(1, MAX_LIST_LIMIT) as usize;
    let want_clickup = filter.source.as_deref() != Some("linear");
    let want_linear = filter.source.as_deref() != Some("clickup");

    ctx.with_db(|db| {
        let conn = db.conn();
        let mut rows: Vec<(Option<i64>, Value)> = Vec::new();

        if want_clickup {
            let tasks = clickup_mirror::read_tasks(conn, &clickup_mirror::TaskFilter::default())?;
            rows.extend(
                tasks
                    .into_iter()
                    .map(|t| (t.date_updated, clickup_row_to_unified(t))),
            );
        }
        if want_linear {
            let issues = linear_mirror::read_issues(conn, &linear_mirror::IssueFilter::default())?;
            rows.extend(
                issues
                    .into_iter()
                    .map(|i| (i.updated_at, linear_row_to_unified(i))),
            );
        }

        rows.sort_by_key(|r| std::cmp::Reverse(r.0));
        let tasks: Vec<Value> = rows.into_iter().take(limit).map(|(_, v)| v).collect();

        Ok(json!({
            "tasks": tasks,
            "mirror_updated_at": {
                "clickup": clickup_mirror_updated_at(conn),
                "linear": linear_mirror_updated_at(conn),
            },
        }))
    })
}

fn clickup_task_detail(conn: &Connection, id: &str) -> Result<Option<Value>> {
    let task = clickup_mirror::read_tasks(
        conn,
        &clickup_mirror::TaskFilter {
            include_stale: true,
            ..Default::default()
        },
    )?
    .into_iter()
    .find(|t| t.id == id);
    let Some(task) = task else {
        return Ok(None);
    };

    let description: Option<String> = conn
        .query_row(
            "SELECT text_content FROM clickup_tasks WHERE id = ?1",
            [id],
            |r| r.get(0),
        )
        .optional()?
        .flatten();

    let comments = clickup_mirror::read_comments(conn, id)?;
    let checklists = clickup_mirror::read_checklists(conn, id)?;
    let attachments = clickup_mirror::read_attachments(conn, id)?;

    let comments_trimmed: Vec<Value> = comments
        .iter()
        .rev() // newest first
        .take(COMMENT_BUDGET)
        .map(|c| {
            json!({
                "id": c.id,
                "author": c.user.as_ref().and_then(|u| u.username.clone()),
                "text": c.text.as_deref().map(trim_text),
                "date": c.date,
            })
        })
        .collect();

    let mut row = clickup_row_to_unified(task);
    row["description"] = json!(description.as_deref().map(trim_text));
    row["subdata_counts"] = json!({
        "checklists": checklists.len(),
        "attachments": attachments.len(),
        "comments": comments.len(),
    });
    row["comments"] = json!(comments_trimmed);
    row["mirror_updated_at"] = json!(clickup_mirror_updated_at(conn));
    Ok(Some(row))
}

fn linear_issue_row(conn: &Connection, id: &str) -> Result<Option<linear_mirror::IssueView>> {
    // `read_issues` filters by team, not id — the caller wants a single
    // known-id lookup, so scope the (already mirror-only) query directly
    // rather than pulling every issue to find one.
    let team_id: Option<String> = conn
        .query_row(
            "SELECT team_id FROM linear_issues WHERE id = ?1",
            [id],
            |r| r.get(0),
        )
        .optional()?;
    let Some(team_id) = team_id else {
        return Ok(None);
    };
    let issue = linear_mirror::read_issues(
        conn,
        &linear_mirror::IssueFilter {
            team_id: Some(team_id),
            include_stale: true,
        },
    )?
    .into_iter()
    .find(|i| i.id == id);
    Ok(issue)
}

fn linear_issue_detail(conn: &Connection, id: &str) -> Result<Option<Value>> {
    let Some(issue) = linear_issue_row(conn, id)? else {
        return Ok(None);
    };

    let subissue_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM linear_issues WHERE parent_id = ?1",
        [id],
        |r| r.get(0),
    )?;
    let comment_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM linear_comments WHERE issue_id = ?1",
        [id],
        |r| r.get(0),
    )?;

    let mut stmt = conn.prepare(
        "SELECT id, user_json, body, created_at FROM linear_comments \
         WHERE issue_id = ?1 ORDER BY created_at DESC LIMIT ?2",
    )?;
    let comments_trimmed: Vec<Value> = stmt
        .query_map(rusqlite::params![id, COMMENT_BUDGET as i64], |r| {
            let user_json: Option<String> = r.get(1)?;
            let body: Option<String> = r.get(2)?;
            Ok(json!({
                "id": r.get::<_, String>(0)?,
                "author": user_json.and_then(|j| {
                    serde_json::from_str::<serde_json::Value>(&j)
                        .ok()
                        .and_then(|v| v.get("id").and_then(|id| id.as_str()).map(String::from))
                }),
                "text": body.as_deref().map(trim_text),
                "date": r.get::<_, Option<i64>>(3)?,
            }))
        })?
        .collect::<std::result::Result<_, _>>()?;

    let description = issue.description.clone();
    let mut row = linear_row_to_unified(issue);
    row["description"] = json!(description.as_deref().map(trim_text));
    row["subdata_counts"] = json!({
        "sub_issues": subissue_count,
        "comments": comment_count,
    });
    row["comments"] = json!(comments_trimmed);
    row["mirror_updated_at"] = json!(linear_mirror_updated_at(conn));
    Ok(Some(row))
}

/// `get_tracker_task(id)` (D4): detail + subdata counts + budget-trimmed
/// comments, mirror-only. ClickUp and Linear ids don't collide (different
/// generators), so a plain id lookup tries both mirrors and returns whichever
/// hits; `None` when neither mirror has it.
pub fn get_tracker_task(ctx: &DaemonContext, id: &str) -> Result<Option<Value>> {
    ctx.with_db(|db| {
        let conn = db.conn();
        if let Some(v) = clickup_task_detail(conn, id)? {
            return Ok(Some(v));
        }
        linear_issue_detail(conn, id)
    })
}

/// `get_pr_status(session_id?)` — the PR + checks rollup (D2 open question
/// resolved: rollup-only for v1), reusing `worktree::pr_status`/`pr_checks`
/// exactly as `commands::ship_pr::poll_pr_checks` does. `session_id`, when
/// given, must equal the caller's asserted identity — a cooperative same-user
/// check (the socket is same-uid gated and the identity hint is client-
/// asserted, not secret-bound), not a cryptographic isolation boundary.
pub fn get_pr_status(ctx: &DaemonContext, caller: &str, session_id: Option<&str>) -> Result<Value> {
    let session_id = session_id.unwrap_or(caller);
    if session_id != caller {
        anyhow::bail!("workspace-scoped: a session may only read its own PR status");
    }
    let (cwd, branch) = ctx.with_db(|db| {
        let cwd = crate::commands::shared::resolve_session_cwd(db, session_id)
            .map_err(|e| anyhow::anyhow!(e))?;
        let branch = crate::commands::shared::resolve_session_branch(db, session_id)
            .map_err(|e| anyhow::anyhow!(e))?;
        Ok((cwd, branch))
    })?;

    let pr = crate::worktree::pr_status(&cwd, &branch)?;
    let Some(pr) = pr else {
        return Ok(json!({ "session_id": session_id, "has_pr": false }));
    };
    let checks = if pr.state == "OPEN" {
        crate::worktree::pr_checks(&cwd, pr.number).ok()
    } else {
        None
    };
    Ok(json!({
        "session_id": session_id,
        "has_pr": true,
        "number": pr.number,
        "title": pr.title,
        "state": pr.state,
        "url": pr.url,
        "checks": checks,
    }))
}

/// `get_git_status(session_id)` — branch/dirty/ahead, the status-bar data,
/// reusing `worktree::current_branch`/`is_worktree_dirty`/`commits_ahead_count`
/// exactly as `commands::git::get_session_git_info` does. `session_id` must
/// equal the caller's asserted identity — a cooperative same-user check (see
/// `get_pr_status`), not a cryptographic isolation boundary.
pub fn get_git_status(ctx: &DaemonContext, caller: &str, session_id: &str) -> Result<Value> {
    if session_id != caller {
        anyhow::bail!("workspace-scoped: a session may only read its own git status");
    }
    let (cwd, repo_path) = ctx.with_db(|db| {
        let cwd = crate::commands::shared::resolve_session_cwd(db, session_id)
            .map_err(|e| anyhow::anyhow!(e))?;
        let session = db.find_session(session_id)?.context("session not found")?;
        let repo_path = db
            .workspace_repo_path(&session.workspace_id)?
            .context("workspace not found")?;
        Ok((cwd, repo_path))
    })?;

    let branch = crate::worktree::current_branch(&cwd).unwrap_or_else(|_| "unknown".into());
    let dirty = crate::worktree::is_worktree_dirty(&cwd).unwrap_or(false);
    let branches = crate::worktree::list_branches(&repo_path).unwrap_or_default();
    let main_branch = if branches.iter().any(|b| b == "main") {
        Some("main")
    } else if branches.iter().any(|b| b == "master") {
        Some("master")
    } else {
        None
    };
    let ahead = main_branch
        .and_then(|m| crate::worktree::commits_ahead_count(&cwd, m).ok())
        .unwrap_or(0);

    Ok(json!({
        "session_id": session_id,
        "branch": branch,
        "dirty": dirty,
        "ahead": ahead,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::state::AgentRuntimeState;
    use crate::db::Database;
    use std::sync::{Arc, Mutex};

    fn test_ctx() -> DaemonContext {
        let db = Arc::new(Mutex::new(Database::open_in_memory().unwrap()));
        let agents = AgentRuntimeState::bootstrap().unwrap();
        DaemonContext {
            db,
            agents,
            delivery: Arc::new(crate::mcp::delivery::NoopDelivery),
            worktree_gate: Default::default(),
        }
    }

    fn seed_clickup(conn: &Connection) {
        // `Database::open_in_memory()` already applies every migration
        // (including the clickup ones), so only seed rows here.
        clickup_mirror::upsert_space(
            conn,
            &crate::clickup::model::Space {
                id: "space1".into(),
                name: "Space".into(),
                ..Default::default()
            },
            1_000,
        )
        .unwrap();
        clickup_mirror::upsert_folder(conn, "folder1", "space1", "Folder", true).unwrap();
        clickup_mirror::upsert_list(
            conn,
            &crate::clickup::model::List {
                id: "list1".into(),
                name: "List".into(),
                folder: Some(crate::clickup::model::NamedRef {
                    id: "folder1".into(),
                    name: "Folder".into(),
                    hidden: Some(true),
                    ..Default::default()
                }),
                ..Default::default()
            },
            "space1",
        )
        .unwrap();
        let task: crate::clickup::model::Task = serde_json::from_value(json!({
            "id": "cu1",
            "name": "ClickUp task",
            "text_content": "cu description",
            "list": { "id": "list1", "name": "List" },
            "date_created": "1000",
            "date_updated": "2000",
            "assignees": [],
            "tags": [],
            "custom_fields": [],
            "checklists": [],
            "attachments": []
        }))
        .unwrap();
        clickup_mirror::upsert_task(conn, &task).unwrap();
        conn.execute(
            "INSERT INTO clickup_sync_state (team_id, baseline_done, last_full_sync) \
             VALUES ('team1', 1, 4242)",
            [],
        )
        .unwrap();
    }

    fn seed_linear(conn: &Connection) {
        // Migration already applied by `Database::open_in_memory()`.
        linear_mirror::upsert_team(conn, "team1", "Team", "T", 1_000).unwrap();
        let issue: crate::linear::model::Issue = serde_json::from_value(json!({
            "id": "li1",
            "identifier": "T-1",
            "title": "Linear issue",
            "description": "li description",
            "priority": 2,
            "team": { "id": "team1" },
            "createdAt": "2026-01-01T00:00:00Z",
            "updatedAt": "2026-01-02T00:00:00Z"
        }))
        .unwrap();
        linear_mirror::upsert_issue(conn, &issue, false).unwrap();
        conn.execute(
            "UPDATE linear_sync_state SET last_full_sync = 5252 WHERE id = 1",
            [],
        )
        .unwrap();
    }

    #[test]
    fn list_tracker_tasks_unifies_and_orders_by_date_updated_desc() {
        let ctx = test_ctx();
        {
            let guard = ctx.db.lock().unwrap();
            seed_clickup(guard.conn());
            seed_linear(guard.conn());
        }
        let result = list_tracker_tasks(&ctx, &ListTrackerTasksFilter::default()).unwrap();
        let tasks = result["tasks"].as_array().unwrap();
        assert_eq!(tasks.len(), 2, "both trackers unified");
        // Linear's updated_at (2026-01-02) postdates ClickUp's (epoch ms 2000).
        assert_eq!(tasks[0]["source"], "linear");
        assert_eq!(tasks[1]["source"], "clickup");
        assert_eq!(result["mirror_updated_at"]["clickup"], 4242);
        assert_eq!(result["mirror_updated_at"]["linear"], 5252);
    }

    #[test]
    fn list_tracker_tasks_source_filter_and_limit() {
        let ctx = test_ctx();
        {
            let guard = ctx.db.lock().unwrap();
            seed_clickup(guard.conn());
            seed_linear(guard.conn());
        }
        let filter = ListTrackerTasksFilter {
            source: Some("clickup".into()),
            limit: Some(1),
        };
        let result = list_tracker_tasks(&ctx, &filter).unwrap();
        let tasks = result["tasks"].as_array().unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0]["source"], "clickup");
    }

    #[test]
    fn get_tracker_task_finds_clickup_and_linear_by_id() {
        let ctx = test_ctx();
        {
            let guard = ctx.db.lock().unwrap();
            seed_clickup(guard.conn());
            seed_linear(guard.conn());
        }
        let cu = get_tracker_task(&ctx, "cu1").unwrap().unwrap();
        assert_eq!(cu["source"], "clickup");
        assert_eq!(cu["description"], "cu description");
        assert_eq!(cu["subdata_counts"]["comments"], 0);
        assert_eq!(cu["mirror_updated_at"], 4242);

        let li = get_tracker_task(&ctx, "li1").unwrap().unwrap();
        assert_eq!(li["source"], "linear");
        assert_eq!(li["description"], "li description");
        assert_eq!(li["subdata_counts"]["sub_issues"], 0);
        assert_eq!(li["mirror_updated_at"], 5252);

        assert!(get_tracker_task(&ctx, "nonexistent").unwrap().is_none());
    }

    #[test]
    fn get_pr_status_rejects_cross_session_lookup() {
        let ctx = test_ctx();
        let err = get_pr_status(&ctx, "me", Some("someone-else")).unwrap_err();
        assert!(err.to_string().contains("workspace-scoped"));
    }

    #[test]
    fn get_git_status_rejects_cross_session_lookup() {
        let ctx = test_ctx();
        let err = get_git_status(&ctx, "me", "someone-else").unwrap_err();
        assert!(err.to_string().contains("workspace-scoped"));
    }

    #[test]
    fn get_pr_status_defaults_session_id_to_caller() {
        // No `git`/`gh` repo at cwd → resolve_session_cwd errors with "session
        // not found" (no such session in this empty DB), proving the default
        // path reached the same-identity branch rather than the scoping guard.
        let ctx = test_ctx();
        let err = get_pr_status(&ctx, "me", None).unwrap_err();
        assert!(!err.to_string().contains("workspace-scoped"));
    }
}
