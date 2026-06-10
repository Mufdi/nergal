//! ClickUp integration (clickup-sync change): token auth, REST read client,
//! the local SQLite mirror, and the interval poller with atomic reconcile.
//! The panel (group 5) consumes the commands below — it reads exclusively
//! from the mirror, never live ClickUp calls per render.

pub mod auth;
pub mod client;
pub mod mirror;
pub mod model;
pub mod poller;

#[derive(Debug, serde::Serialize)]
pub struct TokenStatus {
    /// True when the token lives in the 0600 fallback file instead of the
    /// OS keyring — the UI must disclose it.
    pub token_on_disk: bool,
}

/// Validation result surfaced to the frontend — never the token itself.
#[derive(Debug, serde::Serialize)]
pub struct ResolvedUser {
    pub id: i64,
    pub username: String,
    pub email: String,
}

/// A token change may be a different account — drop the cached user id so
/// the poller re-resolves it via `GET /user`.
fn clear_cached_user(db: &tauri::State<'_, crate::db::SharedDb>) {
    if let Ok(guard) = db.lock()
        && let Err(e) = mirror::set_cached_user_id(guard.conn(), None)
    {
        tracing::warn!("user id cache clear failed: {e:#}");
    }
}

#[tauri::command]
pub fn clickup_set_token(
    token: String,
    app: tauri::AppHandle,
    db: tauri::State<'_, crate::db::SharedDb>,
) -> Result<TokenStatus, String> {
    let status = auth::store_token(&token)
        .map(|on_disk| TokenStatus {
            token_on_disk: on_disk,
        })
        .map_err(|e| format!("{e:#}"))?;
    clear_cached_user(&db);
    poller::note_token_set(&app);
    poller::restart(&app);
    Ok(status)
}

#[tauri::command]
pub fn clickup_clear_token(
    app: tauri::AppHandle,
    db: tauri::State<'_, crate::db::SharedDb>,
) -> Result<(), String> {
    auth::clear_token().map_err(|e| format!("{e:#}"))?;
    clear_cached_user(&db);
    poller::note_token_cleared(&app);
    // The restarted loop re-checks and keeps the status parked on no_token.
    poller::restart(&app);
    Ok(())
}

/// Validates the stored token against `GET /user` and returns the resolved
/// ClickUp user.
#[tauri::command]
pub async fn clickup_validate_token() -> Result<ResolvedUser, String> {
    // Keyring access blocks on D-Bus; keep it off the async workers.
    let stored = tauri::async_runtime::spawn_blocking(auth::load_token)
        .await
        .map_err(|e| format!("token load task failed: {e}"))?
        .map_err(|e| format!("{e:#}"))?;
    let Some(stored) = stored else {
        return Err("no ClickUp token configured".into());
    };
    let client = client::ClickUpClient::new(&stored.token).map_err(|e| format!("{e:#}"))?;
    let user = client.get_user().await.map_err(|e| format!("{e:#}"))?;
    Ok(ResolvedUser {
        id: user.id.unwrap_or_default(),
        username: user.username.unwrap_or_default(),
        email: user.email.unwrap_or_default(),
    })
}

// ── Sync / panel commands (group 4 surface, consumed by group 5) ──

/// Poller + team-selection state for the settings UI and the panel's
/// team-picker prompt.
#[tauri::command]
pub fn clickup_sync_status(
    state: tauri::State<'_, poller::ClickUpSyncState>,
) -> Result<poller::SyncStatus, String> {
    Ok(state.snapshot())
}

/// Persist the team choice (Decision 9: never silently sync `teams[0]`)
/// and restart the poller against it.
#[tauri::command]
pub fn clickup_select_team(
    team_id: String,
    app: tauri::AppHandle,
    db: tauri::State<'_, crate::db::SharedDb>,
) -> Result<(), String> {
    {
        let guard = db.lock().map_err(|_| "db lock poisoned".to_string())?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        poller::select_team(guard.conn(), &team_id, now).map_err(|e| format!("{e:#}"))?;
    }
    poller::note_team_selected(&app, &team_id);
    poller::restart(&app);
    Ok(())
}

/// The panel's only task read path — mirror-only (Decision 1).
#[tauri::command]
pub fn clickup_read_tasks(
    space_id: Option<String>,
    assignee_id: Option<i64>,
    include_stale: Option<bool>,
    db: tauri::State<'_, crate::db::SharedDb>,
) -> Result<Vec<mirror::TaskView>, String> {
    let guard = db.lock().map_err(|_| "db lock poisoned".to_string())?;
    mirror::read_tasks(
        guard.conn(),
        &mirror::TaskFilter {
            space_id,
            assignee_id,
            include_stale: include_stale.unwrap_or(false),
        },
    )
    .map_err(|e| format!("{e:#}"))
}

/// Spaces for the panel's persistent Space selector.
#[tauri::command]
pub fn clickup_read_spaces(
    db: tauri::State<'_, crate::db::SharedDb>,
) -> Result<Vec<mirror::SpaceView>, String> {
    let guard = db.lock().map_err(|_| "db lock poisoned".to_string())?;
    mirror::read_spaces(guard.conn()).map_err(|e| format!("{e:#}"))
}

/// Closed = a terminal status or an explicit close date. The all-tasks poll
/// runs `include_closed=false`, so these rows only exist in the on-demand
/// fetch below.
fn task_is_closed(task: &model::Task) -> bool {
    task.date_closed.is_some()
        || task.status.as_ref().and_then(|s| s.status_type.as_deref()) == Some("closed")
}

async fn fetch_closed_live(
    teams: &[String],
    space_ids: &[String],
) -> anyhow::Result<Vec<mirror::TaskView>> {
    // Keyring access blocks on D-Bus; keep it off the async workers.
    let stored = tauri::async_runtime::spawn_blocking(auth::load_token)
        .await
        .map_err(|e| anyhow::anyhow!("token load task failed: {e}"))??;
    let Some(stored) = stored else {
        anyhow::bail!("no ClickUp token configured");
    };
    let client = client::ClickUpClient::new(&stored.token)?;
    let mut out = Vec::new();
    for team_id in teams {
        let tasks = client
            .filter_team_tasks_all(team_id, space_ids, true)
            .await?;
        for task in &tasks {
            if !task_is_closed(task) {
                continue;
            }
            if let Some(view) = mirror::task_to_view(task) {
                out.push(view);
            }
        }
    }
    Ok(out)
}

/// On-demand `include_closed=true` fetch for the panel's show-closed toggle.
/// The result is EPHEMERAL — never written to the mirror (an upsert would
/// un-tombstone closed tasks and fight the next poll's reconcile); the panel
/// merges it client-side. Offline (or any fetch failure) degrades to the
/// tombstoned mirror rows so the toggle still shows the last-open snapshot.
#[tauri::command]
pub async fn clickup_fetch_closed_tasks(
    space_id: Option<String>,
    db: tauri::State<'_, crate::db::SharedDb>,
) -> Result<Vec<mirror::TaskView>, String> {
    let (teams, space_ids) = {
        let guard = db.lock().map_err(|_| "db lock poisoned".to_string())?;
        let conn = guard.conn();
        let teams = mirror::list_sync_state_teams(conn).map_err(|e| format!("{e:#}"))?;
        let space_ids: Vec<String> = match &space_id {
            Some(id) => vec![id.clone()],
            None => mirror::read_spaces(conn)
                .map_err(|e| format!("{e:#}"))?
                .into_iter()
                .map(|s| s.id)
                .collect(),
        };
        (teams, space_ids)
    };

    match fetch_closed_live(&teams, &space_ids).await {
        Ok(views) => Ok(views),
        Err(e) => {
            tracing::warn!("closed-task fetch failed; serving tombstoned mirror rows: {e:#}");
            let guard = db.lock().map_err(|_| "db lock poisoned".to_string())?;
            let rows = mirror::read_tasks(
                guard.conn(),
                &mirror::TaskFilter {
                    space_id,
                    include_stale: true,
                    ..Default::default()
                },
            )
            .map_err(|e| format!("{e:#}"))?;
            Ok(rows.into_iter().filter(|t| t.stale).collect())
        }
    }
}

/// Full detail view for the floating module.
#[derive(Debug, serde::Serialize)]
pub struct TaskDetail {
    pub task: Option<mirror::TaskView>,
    /// Fresh `description` from the detail fetch when online; the stored
    /// plain `text_content` otherwise.
    pub description: Option<String>,
    pub comments: Vec<mirror::CommentView>,
    pub checklists: Vec<mirror::ChecklistView>,
    pub attachments: Vec<mirror::AttachmentView>,
    pub custom_values: Vec<mirror::CustomValueView>,
}

/// Detail-open entry point for the lazy heavy sub-data (Decision 5): fetch
/// the task + comments, fold them into the mirror, then serve the view from
/// the mirror. Offline (or any fetch failure) degrades to mirror-only.
#[tauri::command]
pub async fn clickup_task_detail(
    task_id: String,
    db: tauri::State<'_, crate::db::SharedDb>,
) -> Result<TaskDetail, String> {
    // Keyring access blocks on D-Bus; keep it off the async workers.
    let stored = tauri::async_runtime::spawn_blocking(auth::load_token)
        .await
        .map_err(|e| format!("token load task failed: {e}"))?
        .map_err(|e| format!("{e:#}"))?;

    let mut fetched: Option<(model::Task, Vec<model::Comment>)> = None;
    let mut fresh_description: Option<String> = None;
    if let Some(stored) = stored
        && let Ok(client) = client::ClickUpClient::new(&stored.token)
    {
        match client.get_task(&task_id, true).await {
            Ok(task) => match client.get_task_comments(&task_id).await {
                Ok(comments) => {
                    fresh_description = task.description.clone();
                    fetched = Some((task, comments));
                }
                Err(e) => {
                    tracing::warn!(task = %task_id, "comment fetch failed; serving mirror: {e:#}");
                }
            },
            Err(e) => {
                tracing::warn!(task = %task_id, "detail fetch failed; serving mirror: {e:#}");
            }
        }
    }

    let guard = db.lock().map_err(|_| "db lock poisoned".to_string())?;
    let conn = guard.conn();
    if let Some((task, comments)) = &fetched {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        if let Err(e) = poller::store_task_detail(conn, task, comments, now) {
            tracing::warn!(task = %task_id, "detail store failed; serving mirror: {e:#}");
        }
    }

    let task = mirror::read_tasks(
        conn,
        &mirror::TaskFilter {
            include_stale: true,
            ..Default::default()
        },
    )
    .map_err(|e| format!("{e:#}"))?
    .into_iter()
    .find(|t| t.id == task_id);
    let stored_text: Option<String> = {
        use rusqlite::OptionalExtension;
        conn.query_row(
            "SELECT text_content FROM clickup_tasks WHERE id = ?1",
            [&task_id],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| format!("{e:#}"))?
        .flatten()
    };

    Ok(TaskDetail {
        task,
        description: fresh_description.or(stored_text),
        comments: mirror::read_comments(conn, &task_id).map_err(|e| format!("{e:#}"))?,
        checklists: mirror::read_checklists(conn, &task_id).map_err(|e| format!("{e:#}"))?,
        attachments: mirror::read_attachments(conn, &task_id).map_err(|e| format!("{e:#}"))?,
        custom_values: mirror::read_custom_values(conn, &task_id).map_err(|e| format!("{e:#}"))?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closed_detection_covers_status_type_and_close_date() {
        let open: model::Task = serde_json::from_str(
            r#"{"id":"t1","name":"open","status":{"status":"en curso","type":"custom"}}"#,
        )
        .unwrap();
        assert!(!task_is_closed(&open));

        let closed_status: model::Task = serde_json::from_str(
            r#"{"id":"t2","name":"done","status":{"status":"terminado","type":"closed"}}"#,
        )
        .unwrap();
        assert!(task_is_closed(&closed_status));

        let closed_date: model::Task =
            serde_json::from_str(r#"{"id":"t3","name":"done","date_closed":"1717090000000"}"#)
                .unwrap();
        assert!(task_is_closed(&closed_date));
    }
}
