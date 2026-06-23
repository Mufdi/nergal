//! Linear read mirror (linear-mirror change #1 of 3).
//!
//! Mirrors the ClickUp integration for Linear: Personal-API-key auth (with an
//! OAuth-extensible header seam), a GraphQL client, a SQLite mirror of
//! teams/states/labels/issues, a bounded poller, and a read-only panel. See
//! `openspec/changes/linear-mirror/` for the spec + design.

pub mod auth;
pub mod client;
pub mod closure;
pub mod integration;
pub mod mirror;
pub mod model;
pub mod poller;
pub mod writeback;

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

/// Validate the active workspace's key against `viewer`. Does not persist.
#[tauri::command]
pub async fn linear_validate_key(
    db: tauri::State<'_, crate::db::SharedDb>,
) -> Result<ResolvedUser, String> {
    let stored = load_active_stored_key(&db).await?;
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

/// Manual "sync now": flips status to syncing and restarts the poll loop so it
/// runs a fresh cycle immediately (run_loop polls before its first sleep).
#[tauri::command]
pub fn linear_sync_now(app: AppHandle) {
    let mut status = app.state::<LinearSyncState>().snapshot();
    if status.state == "no_key" {
        return;
    }
    status.state = "syncing".into();
    status.error = None;
    set_status(&app, status);
    restart(&app);
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

// ── Multi-workspace (linear-mirror-enhancements) ──

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// List stored Linear workspaces (the active one flagged).
#[tauri::command]
pub fn linear_list_workspaces(
    db: tauri::State<'_, crate::db::SharedDb>,
) -> Result<Vec<mirror::WorkspaceRow>, String> {
    let guard = db.lock().map_err(|_| "db lock poisoned".to_string())?;
    mirror::list_workspaces(guard.conn()).map_err(|e| format!("{e:#}"))
}

/// Add a workspace: validate the key, resolve its organization, store the key
/// under the per-org keyring account, and record the workspace. The first
/// workspace added becomes active (wipes+syncs). Returns the workspace row.
#[tauri::command]
pub async fn linear_add_workspace(
    key: String,
    app: AppHandle,
    db: tauri::State<'_, crate::db::SharedDb>,
) -> Result<mirror::WorkspaceRow, String> {
    let key = key.trim().to_string();
    if key.is_empty() {
        return Err("key is empty".into());
    }
    // Validate + resolve the workspace this key belongs to.
    let org = build_client(key.clone())
        .resolve_organization()
        .await
        .map_err(|e| format!("{e:#}"))?;

    let org_id = org.id.clone();
    let key_for_store = key.clone();
    let org_id_for_store = org_id.clone();
    tauri::async_runtime::spawn_blocking(move || {
        auth::store_key_for(&org_id_for_store, &key_for_store)
    })
    .await
    .map_err(|e| format!("join: {e}"))?
    .map_err(|e| format!("{e:#}"))?;

    let make_active = {
        let guard = db.lock().map_err(|_| "db lock poisoned".to_string())?;
        mirror::upsert_workspace(
            guard.conn(),
            &org.id,
            &org.name,
            org.url_key.as_deref(),
            now_secs(),
        )
        .map_err(|e| format!("{e:#}"))?;
        let first = mirror::get_active_org(guard.conn())
            .map_err(|e| format!("{e:#}"))?
            .is_none();
        if first {
            mirror::set_active_org(guard.conn(), Some(&org.id)).map_err(|e| format!("{e:#}"))?;
            // New active workspace: wipe + epoch bump so a fresh sync seeds it.
            mirror::bump_generation_and_wipe(guard.conn()).map_err(|e| format!("{e:#}"))?;
        }
        first
    };
    if make_active {
        note_key_set(&app);
        restart(&app);
    }
    Ok(mirror::WorkspaceRow {
        active: make_active,
        org_id: org.id,
        name: org.name,
        url_key: org.url_key,
    })
}

/// Remove a workspace: delete its key + row. Removing the active one clears the
/// active selection and wipes the mirror.
#[tauri::command]
pub async fn linear_remove_workspace(
    org_id: String,
    app: AppHandle,
    db: tauri::State<'_, crate::db::SharedDb>,
) -> Result<(), String> {
    let org_for_key = org_id.clone();
    tauri::async_runtime::spawn_blocking(move || auth::remove_key_for(&org_for_key))
        .await
        .map_err(|e| format!("join: {e}"))?
        .map_err(|e| format!("{e:#}"))?;
    let was_active = {
        let guard = db.lock().map_err(|_| "db lock poisoned".to_string())?;
        let active = mirror::get_active_org(guard.conn()).map_err(|e| format!("{e:#}"))?;
        mirror::remove_workspace(guard.conn(), &org_id).map_err(|e| format!("{e:#}"))?;
        if active.as_deref() == Some(org_id.as_str()) {
            mirror::set_active_org(guard.conn(), None).map_err(|e| format!("{e:#}"))?;
            mirror::bump_generation_and_wipe(guard.conn()).map_err(|e| format!("{e:#}"))?;
            true
        } else {
            false
        }
    };
    if was_active {
        note_key_cleared(&app);
        restart(&app);
    }
    Ok(())
}

/// Make a stored workspace active: bump the epoch, wipe the current mirror,
/// clear team selection, and re-sync the chosen workspace. The epoch guard
/// prevents a late in-flight poll from the previous workspace committing here.
#[tauri::command]
pub fn linear_set_active_workspace(
    org_id: String,
    app: AppHandle,
    db: tauri::State<'_, crate::db::SharedDb>,
) -> Result<(), String> {
    {
        let guard = db.lock().map_err(|_| "db lock poisoned".to_string())?;
        // Guard: only a known workspace may become active — otherwise we'd wipe
        // the mirror for an org with no key and strand the panel in an error.
        if !mirror::workspace_exists(guard.conn(), &org_id).map_err(|e| format!("{e:#}"))? {
            return Err("unknown workspace".into());
        }
        mirror::set_active_org(guard.conn(), Some(&org_id)).map_err(|e| format!("{e:#}"))?;
        mirror::bump_generation_and_wipe(guard.conn()).map_err(|e| format!("{e:#}"))?;
    }
    note_key_set(&app);
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

// ── Session binding + issue-to-agent verbs (linear-agent-integration) ──

/// Bind `issue_id` as the session's single active issue (the write-back target
/// and session-tab indicator). Replaces an existing active issue — the UI
/// confirms the replacement upstream. Affects future spawns/resumes only.
#[tauri::command]
pub fn linear_bind_issue(
    session_id: String,
    issue_id: String,
    db: tauri::State<'_, crate::db::SharedDb>,
) -> Result<(), String> {
    let guard = db.lock().map_err(|_| "db lock poisoned".to_string())?;
    guard
        .set_active_linear_issue(&session_id, Some(&issue_id))
        .map_err(|e| format!("{e:#}"))
}

/// Clear the active issue. Future spawns/resumes only — context already in a
/// running agent's window is not retracted (design Decision 7).
#[tauri::command]
pub fn linear_unbind_issue(
    session_id: String,
    db: tauri::State<'_, crate::db::SharedDb>,
) -> Result<(), String> {
    let guard = db.lock().map_err(|_| "db lock poisoned".to_string())?;
    guard
        .set_active_linear_issue(&session_id, None)
        .map_err(|e| format!("{e:#}"))
}

/// Pin an issue as context-only. Ordered, idempotent JSON-array edit (mirrors
/// `clickup_pin_task`); returns the updated pin list.
#[tauri::command]
pub fn linear_pin_issue(
    session_id: String,
    issue_id: String,
    db: tauri::State<'_, crate::db::SharedDb>,
) -> Result<Vec<String>, String> {
    let guard = db.lock().map_err(|_| "db lock poisoned".to_string())?;
    guard
        .add_pinned_linear_issue(&session_id, &issue_id)
        .map_err(|e| format!("{e:#}"))?;
    guard
        .get_pinned_linear_issues(&session_id)
        .map_err(|e| format!("{e:#}"))
}

#[tauri::command]
pub fn linear_unpin_issue(
    session_id: String,
    issue_id: String,
    db: tauri::State<'_, crate::db::SharedDb>,
) -> Result<Vec<String>, String> {
    let guard = db.lock().map_err(|_| "db lock poisoned".to_string())?;
    guard
        .remove_pinned_linear_issue(&session_id, &issue_id)
        .map_err(|e| format!("{e:#}"))?;
    guard
        .get_pinned_linear_issues(&session_id)
        .map_err(|e| format!("{e:#}"))
}

/// Compose an issue from the mirror, sanitized for PTY delivery (a crafted
/// comment/description must not smuggle escape sequences that could close the
/// bracketed paste early or drive the terminal).
fn compose_issue_for_delivery(
    db: &tauri::State<'_, crate::db::SharedDb>,
    issue_id: &str,
) -> Result<String, String> {
    let guard = db.lock().map_err(|_| "db lock poisoned".to_string())?;
    let composed = integration::compose_issue_markdown(guard.conn(), issue_id)
        .map_err(|e| format!("{e:#}"))?
        .ok_or_else(|| "issue not found in the local mirror".to_string())?;
    Ok(crate::pty::sanitize_for_pty(&composed))
}

/// Compose-for-confirm step of send-as-prompt: the frontend shows this exact
/// block before any submit (the send auto-submits a turn, so the user must see
/// WHAT will be submitted first).
#[tauri::command]
pub fn linear_compose_issue_prompt(
    issue_id: String,
    db: tauri::State<'_, crate::db::SharedDb>,
) -> Result<String, String> {
    compose_issue_for_delivery(&db, &issue_id)
}

/// Send an issue as a prompt to a live session, after the frontend confirmed
/// the composed block. Re-composes (fresh mirror read), then delivers via
/// bracketed paste + `\r`. A send while the agent is mid-turn relies on the
/// agent's own prompt queueing (CC queues natively). One-shot: no binding.
#[tauri::command]
pub fn linear_send_issue_as_prompt(
    session_id: String,
    issue_id: String,
    db: tauri::State<'_, crate::db::SharedDb>,
    pty: tauri::State<'_, crate::pty::PtyManager>,
) -> Result<(), String> {
    let text = compose_issue_for_delivery(&db, &issue_id)?;
    crate::pty::paste_to_session(&pty, &session_id, &text, true)
        .map_err(|e| format!("pty write failed: {e}"))
}

/// Deliver an issue's composed block to a live session via bracketed paste.
/// Two callers, two stances on `submit`: pin / explicit refresh paste WITHOUT
/// submit (context the user folds into their next turn); bind submits as a turn
/// (the deliberate "work on this" act).
#[tauri::command]
pub fn linear_reinject_issue(
    session_id: String,
    issue_id: String,
    submit: Option<bool>,
    db: tauri::State<'_, crate::db::SharedDb>,
    pty: tauri::State<'_, crate::pty::PtyManager>,
) -> Result<(), String> {
    let text = compose_issue_for_delivery(&db, &issue_id)?;
    crate::pty::paste_to_session(&pty, &session_id, &text, submit.unwrap_or(false))
        .map_err(|e| format!("pty write failed: {e}"))
}

/// Spawn a new worktree session seeded with the issue: derive the slug from the
/// issue title (existing convention: diacritics stripped + timestamp), create
/// the worktree, stash the composed block as the initial prompt
/// (`pending_prompts`, consumed at PTY spawn), and bind the issue as the new
/// session's active issue — the loop-closure this verb exists for. The returned
/// session is activated by the frontend through the normal session-start flow.
#[tauri::command]
#[allow(clippy::too_many_arguments)] // Tauri command surface — collapsing to a struct breaks the JS call shape.
pub fn linear_spawn_worktree_with_issue(
    workspace_id: String,
    issue_id: String,
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

        let composed = integration::compose_issue_markdown(guard.conn(), &issue_id)
            .map_err(|e| format!("{e:#}"))?
            .ok_or_else(|| "issue not found in the local mirror".to_string())?;
        let text = crate::pty::sanitize_for_pty(&composed);

        use rusqlite::OptionalExtension;
        let issue_title: String = guard
            .conn()
            .query_row(
                "SELECT title FROM linear_issues WHERE id = ?1",
                [&issue_id],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| format!("{e:#}"))?
            .ok_or_else(|| "issue not found in the local mirror".to_string())?;

        let base = slug.as_deref().unwrap_or(&issue_title);
        let slug = crate::commands::derive_worktree_slug(base, ts);
        let worktree_dir = repo_path.join(".worktrees").join("nergal").join(&slug);
        if worktree_dir.exists() {
            return Err(format!("a worktree already exists for slug '{slug}'"));
        }
        let wt_path =
            crate::worktree::create_worktree(&repo_path, &slug).map_err(|e| e.to_string())?;

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
            name: issue_title,
            workspace_id: workspace_id.clone(),
            worktree_path: Some(wt_path),
            worktree_branch: Some(format!("nergal/{slug}")),
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
            active_clickup_task_id: None,
            pinned_clickup_task_ids: Vec::new(),
            active_linear_issue_id: Some(issue_id.clone()),
            pinned_linear_issue_ids: Vec::new(),
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

/// Proxy an `uploads.linear.app` image with the Linear auth header attached and
/// return it as a base64 `data:` URL the webview can render directly. A bare
/// `<img src>` to that CDN 401s — only the backend holds the key. Host-pinned +
/// `image/*`-only inside the client (SSRF-safe).
#[tauri::command]
pub async fn linear_fetch_image(
    url: String,
    db: tauri::State<'_, crate::db::SharedDb>,
) -> Result<String, String> {
    use base64::Engine as _;
    let stored = load_active_stored_key(&db).await?;
    let client = build_client(stored.key);
    let (content_type, bytes) = client
        .fetch_image(&url)
        .await
        .map_err(|e| format!("{e:#}"))?;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
    Ok(format!("data:{content_type};base64,{b64}"))
}

/// Lazily fetch an issue's detail (comments + attachments + relations +
/// activity) on detail-open. Network call; not from the mirror. Label ids in
/// the history are resolved to names from the local mirror (no extra round-trip).
#[tauri::command]
pub async fn linear_issue_detail(
    issue_id: String,
    db: tauri::State<'_, crate::db::SharedDb>,
) -> Result<LinearIssueDetail, String> {
    let stored = load_active_stored_key(&db).await?;
    let client = build_client(stored.key);
    let raw = client
        .issue_detail(&issue_id)
        .await
        .map_err(|e| format!("{e:#}"))?;

    let label_names: std::collections::HashMap<String, String> = {
        let guard = db.lock().map_err(|_| "db lock poisoned".to_string())?;
        let mut stmt = guard
            .conn()
            .prepare("SELECT id, name FROM linear_labels")
            .map_err(|e| format!("{e:#}"))?;
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
            .map_err(|e| format!("{e:#}"))?;
        rows.filter_map(|r| r.ok()).collect()
    };
    let activity = normalize_activity(&raw, &label_names);

    let crate::linear::client::RawIssueDetail {
        comments,
        attachments,
        relations,
        ..
    } = raw;
    Ok(LinearIssueDetail {
        activity,
        comments: comments
            .into_iter()
            .map(|c| LinearComment {
                id: c.id,
                body: c.body,
                created_at: c.created_at.as_deref().and_then(model::iso8601_to_epoch),
                author: c.user.and_then(|u| u.display_name.or(u.name)),
                parent_id: c.parent.map(|p| p.id),
            })
            .collect(),
        attachments: attachments
            .into_iter()
            .map(|a| LinearAttachment {
                id: a.id,
                title: a.title,
                subtitle: a.subtitle,
                url: a.url,
            })
            .collect(),
        relations: relations
            .into_iter()
            .filter_map(|r| {
                r.related_issue.map(|ri| LinearRelation {
                    relation_type: r.relation_type,
                    related_id: ri.id,
                    related_identifier: ri.identifier,
                    related_title: ri.title,
                })
            })
            .collect(),
    })
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinearIssueDetail {
    pub comments: Vec<LinearComment>,
    pub attachments: Vec<LinearAttachment>,
    pub relations: Vec<LinearRelation>,
    /// Issue history (activity) — creation + state/assignee/label/cycle/priority
    /// changes, oldest first, like Linear's Activity section.
    pub activity: Vec<LinearActivityEntry>,
}

/// One normalized activity line. `kind` drives the frontend's verb; `from`/`to`
/// carry the change, `added`/`removed` carry label names.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinearActivityEntry {
    pub id: String,
    pub created_at: Option<i64>,
    pub actor: Option<String>,
    pub actor_avatar_url: Option<String>,
    /// "created" | "state" | "assignee" | "label" | "cycle" | "priority"
    pub kind: String,
    pub from: Option<String>,
    pub to: Option<String>,
    pub added: Vec<String>,
    pub removed: Vec<String>,
}

fn priority_word(p: i64) -> String {
    match p {
        1 => "urgent",
        2 => "high",
        3 => "normal",
        4 => "low",
        _ => "no priority",
    }
    .to_string()
}

/// Estimate as a compact string: whole numbers lose the decimal (Linear's
/// t-shirt scale is 1..=6; points are usually integers too).
fn estimate_str(v: f64) -> String {
    if v.fract() == 0.0 {
        format!("{}", v as i64)
    } else {
        format!("{v}")
    }
}

fn cycle_label(c: &crate::linear::model::CycleRef) -> String {
    match (&c.name, c.number) {
        (Some(n), _) if !n.is_empty() => n.clone(),
        (_, Some(num)) => format!("Cycle {num}"),
        _ => "a cycle".to_string(),
    }
}

/// Normalize the raw history into render-ready lines (oldest first) plus a
/// synthesized "created" entry from the issue's creation meta. Entries with no
/// recognized change are dropped (Linear emits some no-op history rows).
fn normalize_activity(
    raw: &crate::linear::client::RawIssueDetail,
    label_names: &std::collections::HashMap<String, String>,
) -> Vec<LinearActivityEntry> {
    let resolve = |ids: &Option<Vec<String>>| -> Vec<String> {
        ids.as_ref()
            .map(|v| {
                v.iter()
                    .map(|id| {
                        label_names
                            .get(id)
                            .cloned()
                            .unwrap_or_else(|| "a label".to_string())
                    })
                    .collect()
            })
            .unwrap_or_default()
    };

    let mut out: Vec<LinearActivityEntry> = Vec::new();

    // Synthesized creation entry (oldest).
    out.push(LinearActivityEntry {
        id: "created".to_string(),
        created_at: raw.created_at.as_deref().and_then(model::iso8601_to_epoch),
        actor: raw
            .creator
            .as_ref()
            .and_then(|u| u.display_name.clone().or_else(|| u.name.clone())),
        actor_avatar_url: raw.creator.as_ref().and_then(|u| u.avatar_url.clone()),
        kind: "created".to_string(),
        from: None,
        to: None,
        added: Vec::new(),
        removed: Vec::new(),
    });

    for h in &raw.history {
        let actor = h
            .actor
            .as_ref()
            .and_then(|u| u.display_name.clone().or_else(|| u.name.clone()))
            .or_else(|| h.bot_actor.as_ref().and_then(|b| b.name.clone()));
        let actor_avatar_url = h.actor.as_ref().and_then(|u| u.avatar_url.clone());
        let created_at = h.created_at.as_deref().and_then(model::iso8601_to_epoch);
        let id = h.id.clone();

        let entry = if h.from_state.is_some() || h.to_state.is_some() {
            Some(LinearActivityEntry {
                id,
                created_at,
                actor,
                actor_avatar_url,
                kind: "state".to_string(),
                from: h.from_state.as_ref().and_then(|s| s.name.clone()),
                to: h.to_state.as_ref().and_then(|s| s.name.clone()),
                added: Vec::new(),
                removed: Vec::new(),
            })
        } else if h.from_assignee.is_some() || h.to_assignee.is_some() {
            Some(LinearActivityEntry {
                id,
                created_at,
                actor,
                actor_avatar_url,
                kind: "assignee".to_string(),
                from: h
                    .from_assignee
                    .as_ref()
                    .and_then(|u| u.display_name.clone().or_else(|| u.name.clone())),
                to: h
                    .to_assignee
                    .as_ref()
                    .and_then(|u| u.display_name.clone().or_else(|| u.name.clone())),
                added: Vec::new(),
                removed: Vec::new(),
            })
        } else if h.added_label_ids.is_some() || h.removed_label_ids.is_some() {
            Some(LinearActivityEntry {
                id,
                created_at,
                actor,
                actor_avatar_url,
                kind: "label".to_string(),
                from: None,
                to: None,
                added: resolve(&h.added_label_ids),
                removed: resolve(&h.removed_label_ids),
            })
        } else if h.from_cycle.is_some() || h.to_cycle.is_some() {
            Some(LinearActivityEntry {
                id,
                created_at,
                actor,
                actor_avatar_url,
                kind: "cycle".to_string(),
                from: h.from_cycle.as_ref().map(cycle_label),
                to: h.to_cycle.as_ref().map(cycle_label),
                added: Vec::new(),
                removed: Vec::new(),
            })
        } else if h.from_priority.is_some() || h.to_priority.is_some() {
            Some(LinearActivityEntry {
                id,
                created_at,
                actor,
                actor_avatar_url,
                kind: "priority".to_string(),
                from: h.from_priority.map(priority_word),
                to: h.to_priority.map(priority_word),
                added: Vec::new(),
                removed: Vec::new(),
            })
        } else if h.from_estimate.is_some() || h.to_estimate.is_some() {
            Some(LinearActivityEntry {
                id,
                created_at,
                actor,
                actor_avatar_url,
                kind: "estimate".to_string(),
                // Raw estimate value; the frontend maps to the t-shirt label when
                // the team uses t-shirt sizing.
                from: h.from_estimate.map(estimate_str),
                to: h.to_estimate.map(estimate_str),
                added: Vec::new(),
                removed: Vec::new(),
            })
        } else if h.from_due_date.is_some() || h.to_due_date.is_some() {
            Some(LinearActivityEntry {
                id,
                created_at,
                actor,
                actor_avatar_url,
                kind: "dueDate".to_string(),
                from: h.from_due_date.clone(),
                to: h.to_due_date.clone(),
                added: Vec::new(),
                removed: Vec::new(),
            })
        } else if h.from_title.is_some() || h.to_title.is_some() {
            Some(LinearActivityEntry {
                id,
                created_at,
                actor,
                actor_avatar_url,
                kind: "title".to_string(),
                from: h.from_title.clone(),
                to: h.to_title.clone(),
                added: Vec::new(),
                removed: Vec::new(),
            })
        } else if h.updated_description == Some(true) {
            Some(LinearActivityEntry {
                id,
                created_at,
                actor,
                actor_avatar_url,
                kind: "description".to_string(),
                from: None,
                to: None,
                added: Vec::new(),
                removed: Vec::new(),
            })
        } else {
            None
        };
        if let Some(e) = entry {
            out.push(e);
        }
    }

    // Oldest first (creation at the top), matching Linear's Activity section.
    out.sort_by_key(|e| e.created_at.unwrap_or(i64::MAX));
    out
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinearComment {
    pub id: String,
    pub body: Option<String>,
    pub created_at: Option<i64>,
    pub author: Option<String>,
    /// Id of the parent comment when this is a reply; `null` for top-level comments.
    pub parent_id: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinearAttachment {
    pub id: String,
    pub title: Option<String>,
    pub subtitle: Option<String>,
    pub url: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinearRelation {
    pub relation_type: String,
    pub related_id: String,
    pub related_identifier: Option<String>,
    pub related_title: Option<String>,
}

// ── Write-back commands (linear-writeback) ──

/// Non-synthetic workflow states for a team — feeds the state picker in the
/// detail and the closure prompt.  Synthetic placeholder rows are excluded
/// (Decision 8 / review #3: a placeholder id must never reach `issueUpdate`).
#[tauri::command]
pub fn linear_read_team_states(
    team_id: String,
    db: tauri::State<'_, crate::db::SharedDb>,
) -> Result<Vec<mirror::WorkflowStateView>, String> {
    let guard = db.lock().map_err(|_| "db lock poisoned".to_string())?;
    mirror::read_team_states(guard.conn(), &team_id).map_err(|e| format!("{e:#}"))
}

/// Update the issue's state (reversible, un-token-gated; validates non-synthetic
/// team membership at the command boundary — Decision 5).
///
/// Records a provisional registry entry BEFORE the API call (TOCTOU-safe —
/// review #7).  Clears it on failure.  On success the reconcile will write the
/// acked value from the server to the mirror; the frontend overlay reverts.
#[tauri::command]
pub async fn linear_set_issue_state(
    issue_id: String,
    state_id: String,
    db: tauri::State<'_, crate::db::SharedDb>,
    registry: tauri::State<'_, writeback::WritebackRegistry>,
) -> Result<(), String> {
    // Server-side state validation.
    let pre = {
        let guard = db.lock().map_err(|_| "db lock poisoned".to_string())?;
        let team_id =
            mirror::get_issue_team_id(guard.conn(), &issue_id).map_err(|e| format!("{e:#}"))?;
        mirror::validate_state_for_team(guard.conn(), &team_id, &state_id)
            .map_err(|e| format!("{e:#}"))?;
        // Read the pre-write value for echo/conflict comparison.
        use rusqlite::OptionalExtension;
        guard
            .conn()
            .query_row(
                "SELECT state_id FROM linear_issues WHERE id = ?1",
                [&issue_id],
                |r| r.get::<_, Option<String>>(0),
            )
            .optional()
            .map_err(|e| format!("{e:#}"))?
            .flatten()
    };

    // Provisional record BEFORE the API call.
    registry.record(
        &issue_id,
        writeback::WriteField::State,
        &state_id,
        pre.as_deref(),
    );

    let stored = load_active_stored_key(&db).await?;
    let client = build_client(stored.key);
    let input = client::IssueUpdateInput {
        state_id: Some(state_id.clone()),
        assignee_id: None,
        cycle_id: None,
    };
    match client.issue_update(&issue_id, input).await {
        Ok(_) => Ok(()),
        Err(e) => {
            registry.clear_entry(&issue_id, &writeback::WriteField::State);
            Err(format!("{e:#}"))
        }
    }
}

/// Update the issue's assignee (reversible, un-token-gated).
///
/// `assignee_id = None` → explicit unassign (sends `assigneeId: null`).
/// The frontend "assign to me" path resolves the viewer id and passes it;
/// the frontend must error when `viewer_id` is unresolved rather than pass
/// `None` (which would unassign — the opposite of intent, Decision 8 / review
/// #11).
#[tauri::command]
pub async fn linear_set_assignee(
    issue_id: String,
    assignee_id: Option<String>,
    db: tauri::State<'_, crate::db::SharedDb>,
    registry: tauri::State<'_, writeback::WritebackRegistry>,
) -> Result<(), String> {
    let pre = {
        let guard = db.lock().map_err(|_| "db lock poisoned".to_string())?;
        use rusqlite::OptionalExtension;
        guard
            .conn()
            .query_row(
                "SELECT assignee_id FROM linear_issues WHERE id = ?1",
                [&issue_id],
                |r| r.get::<_, Option<String>>(0),
            )
            .optional()
            .map_err(|e| format!("{e:#}"))?
            .flatten()
    };

    // Provisional record BEFORE the API call.
    let written = assignee_id.as_deref().unwrap_or("");
    registry.record(
        &issue_id,
        writeback::WriteField::Assignee,
        written,
        pre.as_deref(),
    );

    let stored = load_active_stored_key(&db).await?;
    let client = build_client(stored.key);
    let input = client::IssueUpdateInput {
        state_id: None,
        // outer Some → include the key; inner None → set to null (unassign).
        assignee_id: Some(assignee_id.clone()),
        cycle_id: None,
    };
    match client.issue_update(&issue_id, input).await {
        Ok(_) => Ok(()),
        Err(e) => {
            registry.clear_entry(&issue_id, &writeback::WriteField::Assignee);
            Err(format!("{e:#}"))
        }
    }
}

/// Cycles for a team — feeds the cycle picker in the detail rail.
#[tauri::command]
pub fn linear_read_team_cycles(
    team_id: String,
    db: tauri::State<'_, crate::db::SharedDb>,
) -> Result<Vec<mirror::CycleView>, String> {
    let guard = db.lock().map_err(|_| "db lock poisoned".to_string())?;
    mirror::read_team_cycles(guard.conn(), &team_id).map_err(|e| format!("{e:#}"))
}

/// Move the issue into a cycle (`Some(id)`) or remove it from its cycle
/// (`None` → `cycleId: null`). Reversible, un-token-gated — mirrors
/// `linear_set_assignee` (provisional registry record before the API call).
#[tauri::command]
pub async fn linear_set_issue_cycle(
    issue_id: String,
    cycle_id: Option<String>,
    db: tauri::State<'_, crate::db::SharedDb>,
    registry: tauri::State<'_, writeback::WritebackRegistry>,
) -> Result<(), String> {
    let pre = {
        let guard = db.lock().map_err(|_| "db lock poisoned".to_string())?;
        use rusqlite::OptionalExtension;
        guard
            .conn()
            .query_row(
                "SELECT cycle_id FROM linear_issues WHERE id = ?1",
                [&issue_id],
                |r| r.get::<_, Option<String>>(0),
            )
            .optional()
            .map_err(|e| format!("{e:#}"))?
            .flatten()
    };

    // Provisional record BEFORE the API call (empty string = removed from cycle).
    let written = cycle_id.as_deref().unwrap_or("");
    registry.record(
        &issue_id,
        writeback::WriteField::Cycle,
        written,
        pre.as_deref(),
    );

    let stored = load_active_stored_key(&db).await?;
    let client = build_client(stored.key);
    let input = client::IssueUpdateInput {
        state_id: None,
        assignee_id: None,
        // outer Some → include the key; inner None → set to null (remove).
        cycle_id: Some(cycle_id.clone()),
    };
    match client.issue_update(&issue_id, input).await {
        Ok(_) => Ok(()),
        Err(e) => {
            registry.clear_entry(&issue_id, &writeback::WriteField::Cycle);
            Err(format!("{e:#}"))
        }
    }
}

/// Record that an issue was closed out from this session (durable local marker,
/// separate from any Linear state).
#[tauri::command]
pub fn linear_mark_closed_out(
    issue_id: String,
    db: tauri::State<'_, crate::db::SharedDb>,
) -> Result<(), String> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let guard = db.lock().map_err(|_| "db lock poisoned".to_string())?;
    mirror::mark_closed_out(guard.conn(), &issue_id, now).map_err(|e| format!("{e:#}"))
}

/// Issue ids that were closed out — drives the panel badge.
#[tauri::command]
pub fn linear_read_closed_out(
    db: tauri::State<'_, crate::db::SharedDb>,
) -> Result<Vec<String>, String> {
    let guard = db.lock().map_err(|_| "db lock poisoned".to_string())?;
    mirror::read_closed_out(guard.conn()).map_err(|e| format!("{e:#}"))
}

/// Remove the worked-closed marker so the frontend can un-close-out an issue.
#[tauri::command]
pub fn linear_unmark_closed_out(
    issue_id: String,
    db: tauri::State<'_, crate::db::SharedDb>,
) -> Result<(), String> {
    let guard = db.lock().map_err(|_| "db lock poisoned".to_string())?;
    mirror::unmark_closed_out(guard.conn(), &issue_id).map_err(|e| format!("{e:#}"))
}

/// Projects in the mirror — feeds the panel project-select.
#[tauri::command]
pub fn linear_read_projects(
    db: tauri::State<'_, crate::db::SharedDb>,
) -> Result<Vec<mirror::ProjectView>, String> {
    let guard = db.lock().map_err(|_| "db lock poisoned".to_string())?;
    mirror::read_projects(guard.conn()).map_err(|e| format!("{e:#}"))
}

// ── Poller lifecycle ──

fn build_client(key: String) -> LinearClient {
    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .unwrap_or_default();
    LinearClient::new(http, key, AuthMode::Personal)
}

/// Load the active workspace's key for a one-shot command (validate/detail/
/// image). Errors with a clear message when no workspace is active or its key
/// is missing.
async fn load_active_stored_key(db: &crate::db::SharedDb) -> Result<auth::StoredKey, String> {
    let active = {
        let guard = db.lock().map_err(|_| "db lock poisoned".to_string())?;
        mirror::get_active_org(guard.conn()).map_err(|e| format!("{e:#}"))?
    };
    let org = active.ok_or_else(|| "no active Linear workspace".to_string())?;
    let stored = tauri::async_runtime::spawn_blocking(move || auth::load_key_for(&org))
        .await
        .map_err(|e| format!("join: {e}"))?
        .map_err(|e| format!("{e:#}"))?
        .ok_or_else(|| "no Linear key for the active workspace".to_string())?;
    Ok(stored)
}

/// Resolve the key the poll loop should use this cycle, migrating a legacy
/// single key into a per-workspace entry the first time. Returns `None` when no
/// workspace is configured (the loop parks on `no_key`).
async fn active_key_for_cycle(db: &crate::db::SharedDb) -> anyhow::Result<Option<auth::StoredKey>> {
    // Active workspace already chosen → just load its key. Propagate a poisoned
    // lock as Err (don't fall through to re-migrating a possibly-cleared legacy
    // key — that would route the wrong key state).
    let active = {
        let guard = db.lock().map_err(|_| anyhow::anyhow!("db lock poisoned"))?;
        mirror::get_active_org(guard.conn())?
    };
    if let Some(org) = active {
        let key = tauri::async_runtime::spawn_blocking(move || auth::load_key_for(&org))
            .await
            .map_err(|e| anyhow::anyhow!("join: {e}"))??;
        return Ok(key);
    }

    // No active workspace — try migrating a legacy single key.
    let legacy = tauri::async_runtime::spawn_blocking(auth::load_key)
        .await
        .map_err(|e| anyhow::anyhow!("join: {e}"))??;
    let Some(legacy) = legacy else {
        return Ok(None);
    };
    match build_client(legacy.key.clone())
        .resolve_organization()
        .await
    {
        Ok(org) => {
            let key = legacy.key.clone();
            let org_id = org.id.clone();
            let on_disk =
                tauri::async_runtime::spawn_blocking(move || auth::store_key_for(&org_id, &key))
                    .await
                    .map_err(|e| anyhow::anyhow!("join: {e}"))??;
            // Commit the workspace row BEFORE clearing the legacy key — a poisoned
            // lock must NOT fall through to clear_key (that would delete the user's
            // only working key after the namespaced copy isn't yet recorded).
            {
                let guard = db.lock().map_err(|_| anyhow::anyhow!("db lock poisoned"))?;
                mirror::upsert_workspace(
                    guard.conn(),
                    &org.id,
                    &org.name,
                    org.url_key.as_deref(),
                    now_secs(),
                )?;
                mirror::set_active_org(guard.conn(), Some(&org.id))?;
            }
            // Legacy entry migrated + recorded; remove it so it can't resurface.
            let _ = tauri::async_runtime::spawn_blocking(auth::clear_key).await;
            Ok(Some(auth::StoredKey {
                key: legacy.key,
                on_disk,
            }))
        }
        Err(e) => {
            // Offline/transient: use the legacy key directly this cycle, retry
            // the migration next cycle (never blocks an existing user).
            tracing::warn!("linear legacy-key migration deferred: {e:#}");
            Ok(Some(legacy))
        }
    }
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
        let db = app.state::<crate::db::SharedDb>().inner().clone();
        // Resolve the active workspace's key (migrating a legacy single key on
        // first run). Keyring access blocks on D-Bus → handled off the workers
        // inside the helper.
        let stored = match active_key_for_cycle(&db).await {
            Ok(Some(s)) => s,
            Ok(None) => {
                set_status(
                    &app,
                    SyncStatus {
                        state: "no_key".into(),
                        ..SyncStatus::default()
                    },
                );
                return;
            }
            Err(e) => {
                // Transient keyring/network error: retry, don't park on no_key.
                set_status(&app, err_status(format!("{e:#}")));
                tokio::time::sleep(poll_interval()).await;
                continue;
            }
        };
        let key_on_disk = stored.on_disk;
        let client = build_client(stored.key);

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
            Ok(Some(mut outcome)) => {
                if !outcome.discarded {
                    let _ = app.emit("linear:changed", ());

                    // ── Echo/conflict hook (linear-writeback, Decision 2/3) ──
                    //
                    // run_cycle drops the FetchedCycle and returns only
                    // ReconcileOutcome with no per-issue values (review N2), so
                    // we read the post-reconcile mirror (server truth after blind
                    // upsert) for each recent-write entry.
                    //
                    // The WritebackRegistry is reached via app.state() — NOT via
                    // tauri::State injection (command-only), review N3.
                    if let Some(reg) = app.try_state::<writeback::WritebackRegistry>() {
                        reg.purge_expired();
                        let issue_ids = reg.tracked_issue_ids();
                        for issue_id in &issue_ids {
                            let fields = db
                                .lock()
                                .ok()
                                .and_then(|g| {
                                    mirror::get_issue_write_fields(g.conn(), issue_id).ok()
                                })
                                .flatten();
                            let Some((server_state, server_assignee, server_cycle)) = fields else {
                                // Issue fell out of the poll window; no fresh
                                // remote value — skip (correct by design,
                                // Decision 2 edge case).
                                continue;
                            };
                            let entries = reg.entries_for_issue(issue_id);
                            for entry in entries {
                                let server_val = match entry.field {
                                    writeback::WriteField::State => {
                                        server_state.as_deref().unwrap_or("")
                                    }
                                    writeback::WriteField::Assignee => {
                                        server_assignee.as_deref().unwrap_or("")
                                    }
                                    writeback::WriteField::Cycle => {
                                        server_cycle.as_deref().unwrap_or("")
                                    }
                                };
                                match writeback::check_echo(&entry, server_val) {
                                    writeback::EchoCheckResult::OwnEcho => {
                                        reg.clear_entry(issue_id, &entry.field);
                                        // Filter own assignee echo from
                                        // newly_assigned (REGRESSION guard —
                                        // own "assign to me" must not
                                        // self-fire linear:assigned).
                                        if entry.field == writeback::WriteField::Assignee {
                                            outcome.newly_assigned.retain(|id| id != issue_id);
                                        }
                                    }
                                    writeback::EchoCheckResult::ScalarConflict(c) => {
                                        reg.clear_entry(issue_id, &entry.field);
                                        let _ = app.emit("linear:write-conflict", &c);
                                    }
                                    writeback::EchoCheckResult::Unrelated => {
                                        // Write not yet landed; keep entry for
                                        // the next cycle.
                                    }
                                }
                            }
                        }
                    }

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::linear::client::RawIssueDetail;
    use std::collections::HashMap;

    #[test]
    fn normalize_activity_classifies_and_orders() {
        let history: Vec<crate::linear::model::HistoryEntry> = serde_json::from_value(serde_json::json!([
            {"id":"h2","createdAt":"2026-01-03T00:00:00Z","botActor":{"name":"Linear"},"addedLabelIds":["l1"]},
            {"id":"h1","createdAt":"2026-01-02T00:00:00Z","actor":{"id":"u1","displayName":"mufdidev"},"fromState":{"name":"Todo"},"toState":{"name":"Done"}},
            {"id":"h3","createdAt":"2026-01-04T00:00:00Z","actor":{"id":"u1","displayName":"mufdidev"}}
        ]))
        .unwrap();
        let raw = RawIssueDetail {
            comments: vec![],
            attachments: vec![],
            relations: vec![],
            history,
            created_at: Some("2026-01-01T00:00:00Z".into()),
            creator: serde_json::from_value(
                serde_json::json!({"id":"u1","displayName":"mufdidev"}),
            )
            .ok(),
        };
        let labels = HashMap::from([("l1".to_string(), "Improvement".to_string())]);
        let act = normalize_activity(&raw, &labels);

        // Created entry first (oldest), then state, then label. h3 (no change) dropped.
        assert_eq!(act.len(), 3);
        assert_eq!(act[0].kind, "created");
        assert_eq!(act[0].actor.as_deref(), Some("mufdidev"));
        assert_eq!(act[1].kind, "state");
        assert_eq!(act[1].from.as_deref(), Some("Todo"));
        assert_eq!(act[1].to.as_deref(), Some("Done"));
        assert_eq!(act[2].kind, "label");
        assert_eq!(act[2].actor.as_deref(), Some("Linear")); // bot actor fallback
        assert_eq!(act[2].added, vec!["Improvement".to_string()]);
    }

    // 8.1 comment parent_id round-trip: mapping propagates the parent id correctly
    #[test]
    fn comment_parent_id_round_trip() {
        use crate::linear::model;

        let raw_reply: model::Comment = serde_json::from_value(serde_json::json!({
            "id": "c-reply",
            "body": "A reply",
            "createdAt": "2026-06-18T10:00:00Z",
            "user": { "id": "u1", "displayName": "Alice" },
            "parent": { "id": "c-top" }
        }))
        .unwrap();
        let raw_top: model::Comment = serde_json::from_value(serde_json::json!({
            "id": "c-top",
            "body": "Top-level",
            "createdAt": "2026-06-18T09:00:00Z",
            "user": { "id": "u1", "displayName": "Alice" }
        }))
        .unwrap();

        let mapped_reply = LinearComment {
            id: raw_reply.id,
            body: raw_reply.body,
            created_at: raw_reply
                .created_at
                .as_deref()
                .and_then(model::iso8601_to_epoch),
            author: raw_reply.user.and_then(|u| u.display_name.or(u.name)),
            parent_id: raw_reply.parent.map(|p| p.id),
        };
        let mapped_top = LinearComment {
            id: raw_top.id,
            body: raw_top.body,
            created_at: raw_top
                .created_at
                .as_deref()
                .and_then(model::iso8601_to_epoch),
            author: raw_top.user.and_then(|u| u.display_name.or(u.name)),
            parent_id: raw_top.parent.map(|p| p.id),
        };

        assert_eq!(mapped_reply.parent_id.as_deref(), Some("c-top"));
        assert_eq!(
            mapped_top.parent_id, None,
            "top-level comment must have no parent_id"
        );
    }
}
