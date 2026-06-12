//! ClickUp integration (clickup-sync change): token auth, REST read client,
//! the local SQLite mirror, and the interval poller with atomic reconcile.
//! The panel (group 5) consumes the commands below — it reads exclusively
//! from the mirror, never live ClickUp calls per render.

pub mod auth;
pub mod client;
pub mod closure;
pub mod integration;
pub mod mirror;
pub mod model;
pub mod poller;
pub mod writeback;

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

// ── Status read (clickup-writeback: 1.5) ──

/// Ordered statuses for a List, consumed by the write-back controls and the
/// closure prompt's status picker. Mirror-only — no live API call.
#[tauri::command]
pub fn clickup_read_list_statuses(
    list_id: String,
    db: tauri::State<'_, crate::db::SharedDb>,
) -> Result<Vec<mirror::StatusView>, String> {
    let guard = db.lock().map_err(|_| "db lock poisoned".to_string())?;
    mirror::read_list_statuses(guard.conn(), &list_id).map_err(|e| format!("{e:#}"))
}

// ── Write commands (clickup-writeback: 1.2, 1.3) ──

/// Custom-field value wire shape; the caller must pass the right variant for
/// the field's type. The Tauri command validates this against the mirror's
/// `clickup_custom_field_defs` before calling the client.
#[derive(Debug, serde::Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum CustomFieldValue {
    DropDown(String),
    Labels(Vec<String>),
    Number(f64),
    /// Unix milliseconds.
    Date(i64),
    Url(String),
    Text(String),
    ShortText(String),
    Checkbox(bool),
}

impl CustomFieldValue {
    /// Serialize to the JSON value the ClickUp API expects inside
    /// `{ "value": <here> }`.
    pub fn to_api_value(&self) -> serde_json::Value {
        use serde_json::json;
        match self {
            Self::DropDown(id) => json!(id),
            Self::Labels(ids) => json!(ids),
            Self::Number(n) => json!(n),
            Self::Date(ms) => json!(ms.to_string()),
            Self::Url(u) => json!(u),
            Self::Text(t) => json!(t),
            Self::ShortText(t) => json!(t),
            Self::Checkbox(b) => json!(b),
        }
    }
}

/// Supported writable field types (from tasks.md § 1.2). Anything not in this
/// set is rejected at the command boundary.
fn is_writable_field_type(field_type: &str) -> bool {
    matches!(
        field_type,
        "drop_down" | "labels" | "number" | "date" | "url" | "text" | "short_text" | "checkbox"
    )
}

/// Validate that `status_name` is a real status for the task's list.
/// Returns `Err` with a human-readable message when the status is unknown.
fn validate_status_for_task(
    conn: &rusqlite::Connection,
    task_id: &str,
    status_name: &str,
) -> anyhow::Result<()> {
    use rusqlite::OptionalExtension;
    let list_id: Option<String> = conn
        .query_row(
            "SELECT list_id FROM clickup_tasks WHERE id = ?1",
            [task_id],
            |r| r.get(0),
        )
        .optional()?;
    let list_id =
        list_id.ok_or_else(|| anyhow::anyhow!("task '{task_id}' not found in the local mirror"))?;
    let statuses = mirror::read_list_statuses(conn, &list_id)?;
    let valid = statuses.iter().any(|s| s.name == status_name);
    if !valid {
        let names: Vec<&str> = statuses.iter().map(|s| s.name.as_str()).collect();
        anyhow::bail!(
            "'{status_name}' is not a valid status for list '{list_id}'; \
             known statuses: [{}]",
            names.join(", ")
        );
    }
    Ok(())
}

/// Load the stored token and build a client; returns a clean `Err(String)`
/// for the command surface (mirrors the pattern in `clickup_validate_token`).
async fn load_client() -> Result<client::ClickUpClient, String> {
    let stored = tauri::async_runtime::spawn_blocking(auth::load_token)
        .await
        .map_err(|e| format!("token load task failed: {e}"))?
        .map_err(|e| format!("{e:#}"))?;
    let stored = stored.ok_or_else(|| "no ClickUp token configured".to_string())?;
    client::ClickUpClient::new(&stored.token).map_err(|e| format!("{e:#}"))
}

/// Move a task to `status_name`. Validated server-side against the mirrored
/// status list for the task's List (Decision 5, task 1.3).
#[tauri::command]
pub async fn clickup_set_task_status(
    task_id: String,
    status_name: String,
    db: tauri::State<'_, crate::db::SharedDb>,
    registry: tauri::State<'_, writeback::WritebackRegistry>,
) -> Result<(), String> {
    let pre = {
        use rusqlite::OptionalExtension;
        let guard = db.lock().map_err(|_| "db lock poisoned".to_string())?;
        validate_status_for_task(guard.conn(), &task_id, &status_name)
            .map_err(|e| format!("{e:#}"))?;
        guard
            .conn()
            .query_row(
                "SELECT status_name FROM clickup_tasks WHERE id = ?1",
                [&task_id],
                |r| r.get::<_, Option<String>>(0),
            )
            .optional()
            .map_err(|e| format!("{e:#}"))?
            .flatten()
    };
    let cl = load_client().await?;
    cl.set_task_status(&task_id, &status_name)
        .await
        .map_err(|e| format!("{e:#}"))?;
    registry.record(
        &task_id,
        writeback::WriteField::Status,
        &status_name,
        pre.as_deref(),
    );
    Ok(())
}

/// Toggle a checklist item's resolved state.
#[tauri::command]
pub async fn clickup_set_checklist_item(
    checklist_id: String,
    item_id: String,
    resolved: bool,
    registry: tauri::State<'_, writeback::WritebackRegistry>,
) -> Result<(), String> {
    let cl = load_client().await?;
    cl.set_checklist_item(&checklist_id, &item_id, resolved)
        .await
        .map_err(|e| format!("{e:#}"))?;
    let key = format!("{checklist_id}:{item_id}");
    registry.record(
        // checklist items carry no task_id at the command surface; the key
        // encodes checklist+item so echo matching can still find it.
        &checklist_id,
        writeback::WriteField::ChecklistItem(key),
        if resolved { "true" } else { "false" },
        None::<&str>,
    );
    Ok(())
}

/// Partial task update: description, assignee deltas, due date. Fields absent
/// from the payload are not touched.
#[tauri::command]
pub async fn clickup_update_task(
    task_id: String,
    description: Option<String>,
    assignees_add: Option<Vec<i64>>,
    assignees_rem: Option<Vec<i64>>,
    due_date: Option<serde_json::Value>,
    registry: tauri::State<'_, writeback::WritebackRegistry>,
) -> Result<(), String> {
    let due = due_date.map(|v| match &v {
        serde_json::Value::Number(n) => Some(n.as_i64().unwrap_or_default()),
        serde_json::Value::Null => None,
        _ => None,
    });
    let update = client::TaskUpdate {
        description: description.clone(),
        assignees_add: assignees_add.clone().unwrap_or_default(),
        assignees_rem: assignees_rem.unwrap_or_default(),
        due_date: due,
    };
    let cl = load_client().await?;
    cl.update_task(&task_id, &update)
        .await
        .map_err(|e| format!("{e:#}"))?;
    if let Some(desc) = description {
        registry.record(
            &task_id,
            writeback::WriteField::Description,
            &desc,
            None::<&str>,
        );
    }
    if let Some(adds) = assignees_add.as_ref().filter(|v| !v.is_empty()) {
        // Use the same canonical form as task_field_value: sorted
        // comma-separated integer ids (not a JSON array) so echo matching
        // works across the value comparison.
        let mut sorted = adds.clone();
        sorted.sort_unstable();
        let canonical = sorted
            .iter()
            .map(|id| id.to_string())
            .collect::<Vec<_>>()
            .join(",");
        registry.record(
            &task_id,
            writeback::WriteField::Assignees,
            &canonical,
            None::<&str>,
        );
    }
    if let Some(ms) = due.flatten() {
        registry.record(
            &task_id,
            writeback::WriteField::DueDate,
            ms.to_string(),
            None::<&str>,
        );
    }
    Ok(())
}

/// Set a custom field value. Rejects computed types and unsupported types at
/// the command boundary (Decision 5, tasks 1.2 / 1.3).
#[tauri::command]
pub async fn clickup_set_custom_field(
    task_id: String,
    field_id: String,
    value: CustomFieldValue,
    db: tauri::State<'_, crate::db::SharedDb>,
    registry: tauri::State<'_, writeback::WritebackRegistry>,
) -> Result<(), String> {
    {
        use rusqlite::OptionalExtension;
        let guard = db.lock().map_err(|_| "db lock poisoned".to_string())?;
        let field_type: Option<String> = guard
            .conn()
            .query_row(
                "SELECT type FROM clickup_custom_field_defs WHERE id = ?1",
                [&field_id],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| format!("{e:#}"))?;
        let field_type =
            field_type.ok_or_else(|| format!("field '{field_id}' not found in mirror"))?;
        if field_type == "automatic_progress" {
            return Err(format!(
                "field '{field_id}' is computed (automatic_progress) and cannot be written"
            ));
        }
        if !is_writable_field_type(&field_type) {
            return Err(format!(
                "field type '{field_type}' is not in the supported writable set"
            ));
        }
    }
    let api_value = value.to_api_value();
    let written_str = api_value.to_string();
    let cl = load_client().await?;
    cl.set_custom_field(&task_id, &field_id, api_value)
        .await
        .map_err(|e| format!("{e:#}"))?;
    registry.record(
        &task_id,
        writeback::WriteField::CustomField(field_id),
        &written_str,
        None::<&str>,
    );
    Ok(())
}

// ── Session binding + task-to-agent verbs (clickup-task-integration) ──

/// Bind `task_id` as the session's single active task (the write-back target
/// and session-tab indicator). Replaces an existing active task — the UI
/// confirms the replacement upstream. Affects future spawns/resumes only.
#[tauri::command]
pub fn clickup_bind_task(
    session_id: String,
    task_id: String,
    db: tauri::State<'_, crate::db::SharedDb>,
) -> Result<(), String> {
    let guard = db.lock().map_err(|_| "db lock poisoned".to_string())?;
    guard
        .set_active_clickup_task(&session_id, Some(&task_id))
        .map_err(|e| format!("{e:#}"))
}

/// Clear the active task. Future spawns/resumes only — context already in a
/// running agent's window is not retracted (design Decision 7).
#[tauri::command]
pub fn clickup_unbind_task(
    session_id: String,
    db: tauri::State<'_, crate::db::SharedDb>,
) -> Result<(), String> {
    let guard = db.lock().map_err(|_| "db lock poisoned".to_string())?;
    guard
        .set_active_clickup_task(&session_id, None)
        .map_err(|e| format!("{e:#}"))
}

/// Pin a task as context-only. Ordered, idempotent JSON-array edit (mirrors
/// `pin_vault_note`); returns the updated pin list.
#[tauri::command]
pub fn clickup_pin_task(
    session_id: String,
    task_id: String,
    db: tauri::State<'_, crate::db::SharedDb>,
) -> Result<Vec<String>, String> {
    let guard = db.lock().map_err(|_| "db lock poisoned".to_string())?;
    guard
        .add_pinned_clickup_task(&session_id, &task_id)
        .map_err(|e| format!("{e:#}"))?;
    guard
        .get_pinned_clickup_tasks(&session_id)
        .map_err(|e| format!("{e:#}"))
}

#[tauri::command]
pub fn clickup_unpin_task(
    session_id: String,
    task_id: String,
    db: tauri::State<'_, crate::db::SharedDb>,
) -> Result<Vec<String>, String> {
    let guard = db.lock().map_err(|_| "db lock poisoned".to_string())?;
    guard
        .remove_pinned_clickup_task(&session_id, &task_id)
        .map_err(|e| format!("{e:#}"))?;
    guard
        .get_pinned_clickup_tasks(&session_id)
        .map_err(|e| format!("{e:#}"))
}

/// Compose a task from the mirror, sanitized for PTY delivery (a crafted
/// comment must not smuggle escape sequences that could close the bracketed
/// paste early or drive the terminal).
fn compose_for_delivery(
    db: &tauri::State<'_, crate::db::SharedDb>,
    task_id: &str,
) -> Result<String, String> {
    let guard = db.lock().map_err(|_| "db lock poisoned".to_string())?;
    let composed = integration::compose_task_markdown(guard.conn(), task_id)
        .map_err(|e| format!("{e:#}"))?
        .ok_or_else(|| "task not found in the local mirror".to_string())?;
    Ok(crate::pty::sanitize_for_pty(&composed))
}

/// Compose-for-confirm step of send-as-prompt: the frontend shows this exact
/// block before any submit (design Decision 6 — the send auto-submits a turn,
/// so the user must see WHAT will be submitted first).
#[tauri::command]
pub fn clickup_compose_task_prompt(
    task_id: String,
    db: tauri::State<'_, crate::db::SharedDb>,
) -> Result<String, String> {
    compose_for_delivery(&db, &task_id)
}

/// Send a task as a prompt to a live session, after the frontend confirmed
/// the composed block. Re-composes (fresh mirror read), then delivers via
/// bracketed paste + `\r` (never the raw reinject path — embedded `\n`
/// would fragment the block into partial turns). A send while the agent is
/// mid-turn relies on the agent's own prompt queueing (CC queues natively).
/// One-shot: no binding.
#[tauri::command]
pub fn clickup_send_task_as_prompt(
    session_id: String,
    task_id: String,
    db: tauri::State<'_, crate::db::SharedDb>,
    pty: tauri::State<'_, crate::pty::PtyManager>,
) -> Result<(), String> {
    let text = compose_for_delivery(&db, &task_id)?;
    crate::pty::paste_to_session(&pty, &session_id, &text, true)
        .map_err(|e| format!("pty write failed: {e}"))
}

/// Deliver a task's composed block to a live session via bracketed paste —
/// not the raw write path — so the multi-line block stays one paste. Two
/// callers, two stances on `submit`: pin / explicit refresh paste WITHOUT
/// submit (context the user folds into their next turn, mirroring the
/// vault-note reinject rule); bind submits as a turn (the deliberate "work
/// on this" act — the agent ingests the brief immediately).
#[tauri::command]
pub fn clickup_reinject_task(
    session_id: String,
    task_id: String,
    submit: Option<bool>,
    db: tauri::State<'_, crate::db::SharedDb>,
    pty: tauri::State<'_, crate::pty::PtyManager>,
) -> Result<(), String> {
    let text = compose_for_delivery(&db, &task_id)?;
    crate::pty::paste_to_session(&pty, &session_id, &text, submit.unwrap_or(false))
}

/// Spawn a new worktree session seeded with the task: derive the slug from
/// the task name (existing convention: diacritics stripped + timestamp),
/// create the worktree, stash the composed block as the initial prompt
/// (`pending_prompts`, consumed at PTY spawn), and bind the task as the new
/// session's active task — the loop-closure this verb exists for. The
/// returned session is activated by the frontend through the normal
/// session-start flow (same as deep-link `session/new`).
#[tauri::command]
#[allow(clippy::too_many_arguments)] // Tauri command surface — collapsing to a struct breaks the JS call shape.
pub fn clickup_spawn_worktree_with_task(
    workspace_id: String,
    task_id: String,
    slug: Option<String>,
    db: tauri::State<'_, crate::db::SharedDb>,
    agents: tauri::State<'_, crate::agents::state::AgentRuntimeState>,
    plan_watcher: tauri::State<'_, crate::agents::claude_code::plan::SharedPlanWatcher>,
    pty: tauri::State<'_, crate::pty::PtyManager>,
) -> Result<crate::models::Session, String> {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let (session, text) = {
        let guard = db.lock().map_err(|_| "db lock poisoned".to_string())?;
        let repo_path = guard
            .workspace_repo_path(&workspace_id)
            .map_err(|e| format!("{e:#}"))?
            .ok_or("workspace not found")?;
        if !crate::worktree::is_git_repo(&repo_path) {
            return Err("workspace is not a git repository".into());
        }

        let composed = integration::compose_task_markdown(guard.conn(), &task_id)
            .map_err(|e| format!("{e:#}"))?
            .ok_or_else(|| "task not found in the local mirror".to_string())?;
        let text = crate::pty::sanitize_for_pty(&composed);

        use rusqlite::OptionalExtension;
        let task_name: String = guard
            .conn()
            .query_row(
                "SELECT name FROM clickup_tasks WHERE id = ?1",
                [&task_id],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| format!("{e:#}"))?
            .ok_or_else(|| "task not found in the local mirror".to_string())?;

        let base = slug.as_deref().unwrap_or(&task_name);
        let slug = crate::commands::derive_worktree_slug(base, ts);
        let worktree_dir = repo_path.join(".worktrees").join("cluihud").join(&slug);
        if worktree_dir.exists() {
            return Err(format!("a worktree already exists for slug '{slug}'"));
        }
        let wt_path =
            crate::worktree::create_worktree(&repo_path, &slug).map_err(|e| e.to_string())?;

        // Mirror the agent resolution from create_session: config override >
        // default_agent > CC fallback. Worktrees are always in this workspace's
        // repo, so repo_path is the right key for agent_overrides.
        let agent_id = {
            let cfg = crate::config::Config::load();
            cfg.resolve_agent_for_project(&repo_path)
                .as_deref()
                .and_then(|s| crate::agents::AgentId::new(s).ok())
                .unwrap_or_else(crate::agents::AgentId::claude_code)
        };
        let session = crate::models::Session {
            // char-safe truncation: a byte slice panics mid-codepoint.
            id: format!("{}-{ts}", workspace_id.chars().take(6).collect::<String>()),
            name: task_name,
            workspace_id: workspace_id.clone(),
            worktree_path: Some(wt_path),
            worktree_branch: Some(format!("cluihud/{slug}")),
            merge_target: None,
            status: crate::models::SessionStatus::Idle,
            created_at: ts,
            updated_at: ts,
            agent_id: agent_id.as_str().to_string(),
            agent_internal_session_id: None,
            agent_capabilities: Vec::new(),
            pinned_note_paths: Vec::new(),
            launch_options: None,
            env_shells: Vec::new(),
            active_clickup_task_id: Some(task_id.clone()),
            pinned_clickup_task_ids: Vec::new(),
        };
        guard
            .create_session(&session)
            .map_err(|e| format!("{e:#}"))?;
        agents.register_session(&session.id, agent_id);
        crate::commands::extend_plan_watcher_for_session(
            &agents,
            &plan_watcher,
            &session,
            &repo_path,
        );
        (session, text)
    };

    crate::pty::queue_session_prompt(pty, session.id.clone(), text)?;
    Ok(session)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn mirror_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        conn.execute_batch(include_str!("../../migrations/015_clickup_mirror.sql"))
            .unwrap();
        conn
    }

    /// Seed minimal hierarchy + one task so validation helpers have data.
    fn seed_validation_fixtures(conn: &Connection) {
        conn.execute_batch(
            "INSERT INTO clickup_spaces(id, name, synced_at) VALUES ('sp1','S',0);
             INSERT INTO clickup_folders(id, space_id, name) VALUES ('f1','sp1','F');
             INSERT INTO clickup_lists(id, folder_id, space_id, name) VALUES ('list1','f1','sp1','L');
             INSERT INTO clickup_statuses(id, list_id, status, orderindex)
               VALUES ('s-todo','list1','todo',0),
                      ('s-prog','list1','in progress',1),
                      ('s-done','list1','done',2);
             INSERT INTO clickup_tasks(id, list_id, name)
               VALUES ('task1','list1','Task 1');
             INSERT INTO clickup_custom_field_defs(id, name, type)
               VALUES ('cf-text','Notes','short_text'),
                      ('cf-auto','Progress','automatic_progress'),
                      ('cf-unknown','Widget','3d_model');",
        )
        .unwrap();
    }

    // ── Status validation ──

    #[test]
    fn validate_status_accepts_known_status() {
        let conn = mirror_conn();
        seed_validation_fixtures(&conn);
        assert!(validate_status_for_task(&conn, "task1", "todo").is_ok());
        assert!(validate_status_for_task(&conn, "task1", "in progress").is_ok());
        assert!(validate_status_for_task(&conn, "task1", "done").is_ok());
    }

    #[test]
    fn validate_status_rejects_unknown_status() {
        let conn = mirror_conn();
        seed_validation_fixtures(&conn);
        let err = validate_status_for_task(&conn, "task1", "nonexistent").unwrap_err();
        assert!(
            format!("{err:#}").contains("not a valid status"),
            "unexpected error: {err:#}"
        );
    }

    #[test]
    fn validate_status_rejects_unknown_task() {
        let conn = mirror_conn();
        seed_validation_fixtures(&conn);
        let err = validate_status_for_task(&conn, "no-such-task", "todo").unwrap_err();
        assert!(
            format!("{err:#}").contains("not found"),
            "unexpected error: {err:#}"
        );
    }

    // ── Custom field type validation ──

    #[test]
    fn is_writable_accepts_supported_types() {
        for t in &[
            "drop_down",
            "labels",
            "number",
            "date",
            "url",
            "text",
            "short_text",
            "checkbox",
        ] {
            assert!(is_writable_field_type(t), "expected writable: {t}");
        }
    }

    #[test]
    fn is_writable_rejects_computed_and_unsupported() {
        assert!(!is_writable_field_type("automatic_progress"));
        assert!(!is_writable_field_type("3d_model"));
        assert!(!is_writable_field_type("rollup"));
        assert!(!is_writable_field_type("formula"));
    }

    // ── CustomFieldValue serialization ──

    #[test]
    fn custom_field_value_to_api_value_shapes() {
        use serde_json::json;
        assert_eq!(
            CustomFieldValue::DropDown("opt-id".into()).to_api_value(),
            json!("opt-id")
        );
        assert_eq!(
            CustomFieldValue::Labels(vec!["l1".into(), "l2".into()]).to_api_value(),
            json!(["l1", "l2"])
        );
        assert_eq!(CustomFieldValue::Number(3.14).to_api_value(), json!(3.14));
        // Date is sent as a string (ClickUp API convention).
        assert_eq!(
            CustomFieldValue::Date(1_717_000_000_000).to_api_value(),
            json!("1717000000000")
        );
        assert_eq!(CustomFieldValue::Checkbox(true).to_api_value(), json!(true));
        assert_eq!(
            CustomFieldValue::Url("https://example.com".into()).to_api_value(),
            json!("https://example.com")
        );
    }

    // ── read_list_statuses ──

    #[test]
    fn read_list_statuses_returns_ordered_statuses() {
        let conn = mirror_conn();
        seed_validation_fixtures(&conn);
        let statuses = mirror::read_list_statuses(&conn, "list1").unwrap();
        assert_eq!(statuses.len(), 3);
        assert_eq!(statuses[0].name, "todo");
        assert_eq!(statuses[1].name, "in progress");
        assert_eq!(statuses[2].name, "done");
    }

    #[test]
    fn read_list_statuses_returns_empty_for_unknown_list() {
        let conn = mirror_conn();
        seed_validation_fixtures(&conn);
        let statuses = mirror::read_list_statuses(&conn, "no-such-list").unwrap();
        assert!(statuses.is_empty());
    }

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
