//! Linear read mirror (linear-mirror change #1 of 3).
//!
//! Mirrors the ClickUp integration for Linear: Personal-API-key auth (with an
//! OAuth-extensible header seam), a GraphQL client, a SQLite mirror of
//! teams/states/labels/issues, a bounded poller, and a read-only panel. See
//! `openspec/changes/linear-mirror/` for the spec + design.

pub mod auth;
pub mod client;
pub mod mirror;
pub mod model;
pub mod poller;

use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use auth::AuthMode;
use client::LinearClient;

const DEFAULT_POLL_INTERVAL_SECS: u64 = 45;

// ── Sync status (pushed to the panel) ──

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncStatus {
    /// "idle" | "no_key" | "needs_team" | "syncing" | "ok" | "error"
    pub state: String,
    pub viewer_id: Option<String>,
    pub viewer_name: Option<String>,
    pub selected_team_ids: Vec<String>,
    pub last_sync: Option<i64>,
    pub baseline_done: bool,
    pub key_on_disk: bool,
    pub error: Option<String>,
}

pub struct LinearSyncState {
    handle: std::sync::Mutex<Option<tauri::async_runtime::JoinHandle<()>>>,
    status: std::sync::Mutex<SyncStatus>,
}

impl Default for LinearSyncState {
    fn default() -> Self {
        Self {
            handle: std::sync::Mutex::new(None),
            status: std::sync::Mutex::new(SyncStatus {
                state: "idle".into(),
                ..SyncStatus::default()
            }),
        }
    }
}

impl LinearSyncState {
    pub fn snapshot(&self) -> SyncStatus {
        self.status
            .lock()
            .map(|g| g.clone())
            .unwrap_or_else(|_| SyncStatus {
                state: "error".into(),
                error: Some("status lock poisoned".into()),
                ..SyncStatus::default()
            })
    }
}

fn set_status(app: &AppHandle, status: SyncStatus) {
    let state = app.state::<LinearSyncState>();
    if let Ok(mut guard) = state.status.lock() {
        if *guard == status {
            return;
        }
        *guard = status.clone();
    }
    let _ = app.emit("linear:sync-status", status);
}

// ── Resolved-user shape returned by validation ──

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedUser {
    pub id: String,
    pub name: Option<String>,
    pub email: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyStatus {
    pub key_on_disk: bool,
}

// ── Commands ──

/// Store the API key. Bumps the account-swap epoch (wipes the mirror + clears
/// viewer/team selection/baseline), then restarts the poller.
#[tauri::command]
pub async fn linear_set_key(
    key: String,
    app: AppHandle,
    db: tauri::State<'_, crate::db::SharedDb>,
) -> Result<KeyStatus, String> {
    let on_disk = tauri::async_runtime::spawn_blocking(move || auth::store_key(&key))
        .await
        .map_err(|e| format!("join: {e}"))?
        .map_err(|e| format!("{e:#}"))?;
    {
        let guard = db.lock().map_err(|_| "db lock poisoned".to_string())?;
        mirror::bump_generation_and_wipe(guard.conn()).map_err(|e| format!("{e:#}"))?;
    }
    note_key_set(&app);
    restart(&app);
    Ok(KeyStatus {
        key_on_disk: on_disk,
    })
}

#[tauri::command]
pub async fn linear_clear_key(
    app: AppHandle,
    db: tauri::State<'_, crate::db::SharedDb>,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(auth::clear_key)
        .await
        .map_err(|e| format!("join: {e}"))?
        .map_err(|e| format!("{e:#}"))?;
    {
        let guard = db.lock().map_err(|_| "db lock poisoned".to_string())?;
        mirror::bump_generation_and_wipe(guard.conn()).map_err(|e| format!("{e:#}"))?;
    }
    note_key_cleared(&app);
    restart(&app);
    Ok(())
}

/// Validate the stored key against `viewer`. Does not persist anything.
#[tauri::command]
pub async fn linear_validate_key() -> Result<ResolvedUser, String> {
    let stored = tauri::async_runtime::spawn_blocking(auth::load_key)
        .await
        .map_err(|e| format!("join: {e}"))?
        .map_err(|e| format!("{e:#}"))?
        .ok_or_else(|| "no Linear key configured".to_string())?;
    let client = build_client(stored.key);
    let viewer = client.get_viewer().await.map_err(|e| format!("{e:#}"))?;
    Ok(ResolvedUser {
        id: viewer.id,
        name: viewer.name,
        email: viewer.email,
    })
}

#[tauri::command]
pub fn linear_sync_status(state: tauri::State<'_, LinearSyncState>) -> SyncStatus {
    state.snapshot()
}

/// Persist the team selection and restart the poller.
#[tauri::command]
pub fn linear_select_teams(
    team_ids: Vec<String>,
    app: AppHandle,
    db: tauri::State<'_, crate::db::SharedDb>,
) -> Result<(), String> {
    {
        let guard = db.lock().map_err(|_| "db lock poisoned".to_string())?;
        mirror::set_selected_teams(guard.conn(), &team_ids).map_err(|e| format!("{e:#}"))?;
    }
    note_teams_selected(&app, &team_ids);
    restart(&app);
    Ok(())
}

#[tauri::command]
pub fn linear_read_issues(
    team_id: Option<String>,
    include_stale: Option<bool>,
    db: tauri::State<'_, crate::db::SharedDb>,
) -> Result<Vec<mirror::IssueView>, String> {
    let guard = db.lock().map_err(|_| "db lock poisoned".to_string())?;
    mirror::read_issues(
        guard.conn(),
        &mirror::IssueFilter {
            team_id,
            include_stale: include_stale.unwrap_or(false),
        },
    )
    .map_err(|e| format!("{e:#}"))
}

#[tauri::command]
pub fn linear_read_teams(
    db: tauri::State<'_, crate::db::SharedDb>,
) -> Result<Vec<mirror::TeamView>, String> {
    let guard = db.lock().map_err(|_| "db lock poisoned".to_string())?;
    mirror::read_teams(guard.conn()).map_err(|e| format!("{e:#}"))
}

/// Lazily fetch an issue's comments (detail-open). Network call; not from the
/// mirror.
#[tauri::command]
pub async fn linear_issue_comments(issue_id: String) -> Result<Vec<LinearComment>, String> {
    let stored = tauri::async_runtime::spawn_blocking(auth::load_key)
        .await
        .map_err(|e| format!("join: {e}"))?
        .map_err(|e| format!("{e:#}"))?
        .ok_or_else(|| "no Linear key configured".to_string())?;
    let client = build_client(stored.key);
    let comments = client
        .issue_comments(&issue_id)
        .await
        .map_err(|e| format!("{e:#}"))?;
    Ok(comments
        .into_iter()
        .map(|c| LinearComment {
            id: c.id,
            body: c.body,
            created_at: c.created_at.as_deref().and_then(model::iso8601_to_epoch),
            author: c.user.and_then(|u| u.display_name.or(u.name)),
        })
        .collect())
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinearComment {
    pub id: String,
    pub body: Option<String>,
    pub created_at: Option<i64>,
    pub author: Option<String>,
}

// ── Poller lifecycle ──

fn build_client(key: String) -> LinearClient {
    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .unwrap_or_default();
    LinearClient::new(http, key, AuthMode::Personal)
}

/// (Re)start the poll loop. Safe to call any time; key set/clear and team
/// selection all funnel here. The loop decides whether it can run.
pub fn restart(app: &AppHandle) {
    let state = app.state::<LinearSyncState>();
    let Ok(mut guard) = state.handle.lock() else {
        return;
    };
    if let Some(handle) = guard.take() {
        handle.abort();
    }
    let loop_app = app.clone();
    *guard = Some(tauri::async_runtime::spawn(async move {
        run_loop(loop_app).await;
    }));
}

pub fn note_key_set(app: &AppHandle) {
    let mut status = app.state::<LinearSyncState>().snapshot();
    status.state = "syncing".into();
    status.viewer_id = None;
    status.viewer_name = None;
    status.selected_team_ids = Vec::new();
    status.error = None;
    set_status(app, status);
}

pub fn note_key_cleared(app: &AppHandle) {
    set_status(
        app,
        SyncStatus {
            state: "no_key".into(),
            ..SyncStatus::default()
        },
    );
}

pub fn note_teams_selected(app: &AppHandle, team_ids: &[String]) {
    let mut status = app.state::<LinearSyncState>().snapshot();
    status.state = "syncing".into();
    status.selected_team_ids = team_ids.to_vec();
    status.error = None;
    set_status(app, status);
}

fn poll_interval() -> Duration {
    let secs = crate::config::Config::load()
        .linear_poll_interval_secs
        .unwrap_or(DEFAULT_POLL_INTERVAL_SECS);
    Duration::from_secs(secs.max(10))
}

async fn run_loop(app: AppHandle) {
    loop {
        // Keyring access blocks on D-Bus; keep it off the async workers.
        let stored = match tauri::async_runtime::spawn_blocking(auth::load_key).await {
            Ok(Ok(Some(s))) => s,
            Ok(Ok(None)) => {
                set_status(
                    &app,
                    SyncStatus {
                        state: "no_key".into(),
                        ..SyncStatus::default()
                    },
                );
                return;
            }
            Ok(Err(e)) => {
                // Transient keyring error: retry, don't park on no_key.
                set_status(&app, err_status(format!("{e:#}")));
                tokio::time::sleep(poll_interval()).await;
                continue;
            }
            Err(e) => {
                tracing::warn!("linear key load join failed: {e}");
                tokio::time::sleep(poll_interval()).await;
                continue;
            }
        };
        let key_on_disk = stored.on_disk;
        let client = build_client(stored.key);
        let db = app.state::<crate::db::SharedDb>().inner().clone();

        // Pre-cycle baseline snapshot (suppress notifications until established).
        let pre = {
            db.lock()
                .ok()
                .and_then(|g| mirror::get_sync_state(g.conn()).ok())
        };
        let pre_baseline = pre.as_ref().map(|s| s.baseline_done).unwrap_or(false);
        let selected = pre
            .as_ref()
            .map(|s| s.selected_team_ids.clone())
            .unwrap_or_default();

        match poller::run_cycle(&client, &db).await {
            Ok(Some(outcome)) => {
                if !outcome.discarded {
                    let _ = app.emit("linear:changed", ());
                    // Notify only post-baseline (suppress the seeding cycle).
                    if pre_baseline && !outcome.newly_assigned.is_empty() {
                        notify_assignments(&app, &db, &outcome.newly_assigned);
                    }
                }
                let st = {
                    db.lock()
                        .ok()
                        .and_then(|g| mirror::get_sync_state(g.conn()).ok())
                };
                // A fetch error (e.g. a rejected issues query) surfaces even
                // though the reconcile committed what it got — otherwise the
                // panel reads as a silent empty list.
                let state_label = if outcome.fetch_error.is_some() {
                    "error"
                } else if selected.is_empty() {
                    "needs_team"
                } else {
                    "ok"
                };
                set_status(
                    &app,
                    SyncStatus {
                        state: state_label.into(),
                        viewer_id: st.as_ref().and_then(|s| s.viewer_id.clone()),
                        viewer_name: None,
                        selected_team_ids: st
                            .as_ref()
                            .map(|s| s.selected_team_ids.clone())
                            .unwrap_or_default(),
                        last_sync: st.as_ref().and_then(|s| s.last_full_sync),
                        baseline_done: st.as_ref().map(|s| s.baseline_done).unwrap_or(false),
                        key_on_disk,
                        error: outcome.fetch_error.clone(),
                    },
                );
            }
            // Cycle skipped (viewer resolve failed): keep prior status, retry.
            Ok(None) => {}
            Err(e) => {
                set_status(&app, err_status(format!("{e:#}")));
            }
        }
        tokio::time::sleep(poll_interval()).await;
    }
}

fn err_status(message: String) -> SyncStatus {
    SyncStatus {
        state: "error".into(),
        error: Some(message),
        ..SyncStatus::default()
    }
}

/// Coalesced notification for newly-assigned issues (post-baseline): emits the
/// in-app `linear:assigned` event (panel toast) AND an OS desktop notification.
fn notify_assignments(app: &AppHandle, db: &crate::db::SharedDb, ids: &[String]) {
    if ids.is_empty() {
        return;
    }
    // Resolve titles for a friendly body; fall back to the count.
    let titles: Vec<String> = db
        .lock()
        .ok()
        .map(|g| {
            ids.iter()
                .filter_map(|id| {
                    g.conn()
                        .query_row("SELECT title FROM linear_issues WHERE id=?1", [id], |r| {
                            r.get::<_, String>(0)
                        })
                        .ok()
                })
                .collect()
        })
        .unwrap_or_default();
    // In-app toast (the frontend listens for this; without it only the OS
    // notification fires, matching ClickUp's `clickup:assigned`).
    let _ = app.emit("linear:assigned", &titles);
    let body = if titles.len() == 1 {
        titles[0].clone()
    } else {
        format!("{} new issues assigned to you", ids.len().max(titles.len()))
    };
    if let Err(e) = std::process::Command::new("notify-send")
        .arg("Linear")
        .arg(&body)
        .spawn()
    {
        tracing::warn!("linear assignment notify-send failed: {e}");
    }
}
