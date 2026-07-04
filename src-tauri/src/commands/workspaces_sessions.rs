use std::path::PathBuf;

use tauri::State;

use crate::agents::AgentId;
use crate::agents::ThemePalette;
use crate::agents::state::AgentRuntimeState;
use crate::db::SharedDb;
use crate::models::{Session, SessionStatus, Workspace};

fn strip_diacritics(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'á' | 'à' | 'ä' | 'â' | 'ã' => 'a',
            'é' | 'è' | 'ë' | 'ê' => 'e',
            'í' | 'ì' | 'ï' | 'î' => 'i',
            'ó' | 'ò' | 'ö' | 'ô' | 'õ' => 'o',
            'ú' | 'ù' | 'ü' | 'û' => 'u',
            'ñ' => 'n',
            'Á' | 'À' | 'Ä' | 'Â' | 'Ã' => 'A',
            'É' | 'È' | 'Ë' | 'Ê' => 'E',
            'Í' | 'Ì' | 'Ï' | 'Î' => 'I',
            'Ó' | 'Ò' | 'Ö' | 'Ô' | 'Õ' => 'O',
            'Ú' | 'Ù' | 'Ü' | 'Û' => 'U',
            'Ñ' => 'N',
            other => other,
        })
        .collect()
}

/// Worktree-slug convention shared by session creation and the ClickUp
/// spawn-worktree verb: diacritics stripped, non-alphanumerics collapsed to
/// `-`, timestamp suffix.
pub(crate) fn derive_worktree_slug(name: &str, ts: u64) -> String {
    let normalized = strip_diacritics(name);
    let slug: String = normalized
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    format!("{slug}-{ts}")
}

/// Subscribe the dynamic plan watcher to a freshly created CC session's
/// plans dir (shared by `create_session` and the ClickUp spawn-worktree
/// verb). Logged, never fatal.
pub(crate) fn extend_plan_watcher_for_session(
    agents: &AgentRuntimeState,
    plan_watcher: &crate::agents::claude_code::plan::SharedPlanWatcher,
    session: &Session,
    repo_path: &std::path::Path,
) {
    let agent_id = match AgentId::new(&session.agent_id) {
        Ok(id) => id,
        Err(_) => return,
    };
    if agent_id != AgentId::claude_code() {
        return;
    }
    let Some(adapter) = agents.registry.get(&agent_id) else {
        return;
    };
    let cwd = session
        .worktree_path
        .clone()
        .unwrap_or_else(|| repo_path.to_path_buf());
    let cap = adapter.plan_capability(session, &cwd);
    if let crate::agents::PlanCapability::FileBased { dir, .. } = cap
        && let Ok(mut w) = plan_watcher.lock()
        && let Err(e) = w.ensure_dir_and_watch(&dir)
    {
        tracing::warn!(
            dir = %dir.display(),
            error = %e,
            "plan watcher extend failed"
        );
    }
}

// -- Setup command --

#[tauri::command]
pub fn setup_hooks() -> Result<String, String> {
    crate::setup::run().map_err(|e| e.to_string())?;
    Ok("Hooks configured successfully".into())
}

// -- Workspace commands --

#[tauri::command]
pub fn create_workspace(db: State<'_, SharedDb>, repo_path: String) -> Result<Workspace, String> {
    let path = PathBuf::from(&repo_path);
    if !path.is_dir() {
        return Err("Not a directory".into());
    }

    let db = db.lock().map_err(|e| e.to_string())?;

    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    use std::hash::{Hash, Hasher};
    path.to_string_lossy().hash(&mut hasher);
    let hash = hasher.finish();
    let id = format!("{hash:016x}")[..12].to_string();

    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| id.clone());

    db.create_workspace(&id, &name, &repo_path)
        .map_err(|e| e.to_string())?;

    Ok(Workspace {
        id,
        name,
        is_git: crate::worktree::is_git_repo(&path),
        repo_path: path,
        sessions: Vec::new(),
        created_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
    })
}

/// Side-effect-free pre-creation check for the deep-link confirmation gate:
/// reports dir + git-repo status for a path WITHOUT creating a workspace, so a
/// declined confirm leaves no trace (confirm-deep-link-session-spawn).
#[derive(serde::Serialize)]
pub struct WorkspaceProbe {
    pub is_dir: bool,
    pub is_git_repo: bool,
    pub resolved: String,
}

#[tauri::command]
pub fn probe_workspace_path(path: String) -> WorkspaceProbe {
    let raw = PathBuf::from(&path);
    let resolved = std::fs::canonicalize(&raw).unwrap_or(raw);
    WorkspaceProbe {
        is_dir: resolved.is_dir(),
        is_git_repo: crate::worktree::is_git_repo(&resolved),
        resolved: resolved.to_string_lossy().into_owned(),
    }
}

#[tauri::command]
pub fn get_workspaces(
    db: State<'_, SharedDb>,
    agents: State<'_, AgentRuntimeState>,
) -> Result<Vec<Workspace>, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let mut workspaces = db.get_workspaces().map_err(|e| e.to_string())?;
    // DB rows don't persist capabilities — they're a runtime property of the
    // adapter. Fill them in here so the frontend has the bitset to gate UI
    // (ResumeModal options, picker affordances) without an extra round-trip.
    for ws in &mut workspaces {
        for session in &mut ws.sessions {
            session.agent_capabilities = capabilities_for_agent_id(&agents, &session.agent_id);
        }
    }
    Ok(workspaces)
}

/// Lookup the wire-form capability list for an agent id, falling back to
/// an empty list if the adapter is unregistered (defensive — config drift).
fn capabilities_for_agent_id(agents: &AgentRuntimeState, agent_id: &str) -> Vec<String> {
    let Ok(parsed) = AgentId::new(agent_id) else {
        return Vec::new();
    };
    let Some(adapter) = agents.registry.get(&parsed) else {
        return Vec::new();
    };
    let value = match serde_json::to_value(adapter.capabilities().flags) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    serde_json::from_value::<Vec<String>>(value).unwrap_or_default()
}

#[tauri::command]
pub fn delete_workspace(db: State<'_, SharedDb>, workspace_id: String) -> Result<(), String> {
    struct SessionCleanup {
        moc_inputs: Option<crate::obsidian::moc::MocInputs>,
        worktree_path: Option<PathBuf>,
    }

    // Gather everything under one guard (moc inputs per-session + worktree
    // paths), then drop before the file I/O / worktree removal below.
    let (repo_path, cfg, sessions) = {
        let db = db.lock().map_err(|e| e.to_string())?;

        // Get workspace data for worktree cleanup before deletion
        let workspaces = db.get_workspaces().map_err(|e| e.to_string())?;
        let Some(ws) = workspaces.iter().find(|w| w.id == workspace_id) else {
            // Nothing to snapshot/clean up — still under this guard, so no
            // need to re-acquire for the delete.
            return db
                .delete_workspace(&workspace_id)
                .map_err(|e| e.to_string());
        };

        // Snapshot each session synchronously before its worktree + the
        // workspace row disappear (the detached runner runs too late).
        let cfg =
            crate::obsidian::config::resolve(&workspace_id, |w| db.get_obsidian_config(w)).ok();
        let want_moc = cfg
            .as_ref()
            .is_some_and(|c| c.moc_path.as_deref().filter(|p| !p.is_empty()).is_some());

        let sessions: Vec<SessionCleanup> = ws
            .sessions
            .iter()
            .map(|session| {
                let moc_inputs = if want_moc {
                    cfg.as_ref().and_then(|cfg| {
                        crate::obsidian::moc::MocBuilder::gather(&session.id, cfg, &db)
                            .ok()
                            .flatten()
                    })
                } else {
                    None
                };
                SessionCleanup {
                    moc_inputs,
                    worktree_path: session.worktree_path.clone(),
                }
            })
            .collect();

        (ws.repo_path.clone(), cfg, sessions)
    };

    if let Some(cfg) = &cfg {
        for s in &sessions {
            if let Some(inputs) = &s.moc_inputs
                && let Ok(Some(moc_path)) = inputs.render_and_write(cfg)
            {
                let _ = crate::obsidian::moc::BacklinkUpdater::propagate(&moc_path, cfg);
            }
        }
    }
    for s in &sessions {
        if let Some(wt) = &s.worktree_path {
            let _ = crate::worktree::remove_worktree(&repo_path, wt);
        }
    }

    let db = db.lock().map_err(|e| e.to_string())?;
    db.delete_workspace(&workspace_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn reorder_workspaces(ordered_ids: Vec<String>, db: State<'_, SharedDb>) -> Result<(), String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    db.set_workspace_order(&ordered_ids)
        .map_err(|e| e.to_string())
}

// -- Session commands --

#[allow(clippy::too_many_arguments)]
fn build_new_session(
    session_id: String,
    name: String,
    workspace_id: String,
    worktree_path: Option<PathBuf>,
    worktree_branch: Option<String>,
    ts: u64,
    agent_id: &AgentId,
    agent_capabilities: Vec<String>,
    launch_options: Option<crate::models::LaunchOptions>,
    env_shells: Option<Vec<crate::models::EnvShellDef>>,
) -> Session {
    Session {
        id: session_id,
        name,
        workspace_id,
        worktree_path,
        worktree_branch,
        merge_target: None,
        status: SessionStatus::Idle,
        created_at: ts,
        updated_at: ts,
        agent_id: agent_id.as_str().to_string(),
        agent_internal_session_id: None,
        agent_capabilities,
        pinned_note_paths: Vec::new(),
        // Drop all-default options so the column stays NULL for the common
        // case (and resume short-circuits the lookup).
        launch_options: launch_options.filter(|o| !o.is_noop()),
        env_shells: env_shells
            .unwrap_or_default()
            .into_iter()
            .filter(|d| !d.command.trim().is_empty())
            .collect(),
        active_clickup_task_id: None,
        pinned_clickup_task_ids: Vec::new(),
        active_linear_issue_id: None,
        pinned_linear_issue_ids: Vec::new(),
    }
}

#[tauri::command]
#[allow(clippy::too_many_arguments)] // Tauri command surface — collapsing to a struct breaks the JS call shape.
pub fn create_session(
    db: State<'_, SharedDb>,
    agents: State<'_, AgentRuntimeState>,
    plan_watcher: State<'_, crate::agents::claude_code::plan::SharedPlanWatcher>,
    workspace_id: String,
    name: String,
    agent_id: Option<String>,
    launch_options: Option<crate::models::LaunchOptions>,
    env_shells: Option<Vec<crate::models::EnvShellDef>>,
) -> Result<Session, String> {
    let guard = db.lock().map_err(|e| e.to_string())?;

    let repo_path = guard
        .workspace_repo_path(&workspace_id)
        .map_err(|e| e.to_string())?
        .ok_or("workspace not found")?;

    let is_first = guard
        .session_count_for_workspace(&workspace_id)
        .map_err(|e| e.to_string())?
        == 0;

    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let session_id = format!("{}-{ts}", &workspace_id[..6.min(workspace_id.len())]);

    // Picker priority: explicit caller arg > config-resolved > CC fallback.
    // Today the frontend passes no agent_id, so this resolves to CC unless the
    // user has set config.default_agent. Picker UI lands once another adapter
    // is registered (opencode-adapter, pi-adapter, codex-adapter).
    let agent_id = agent_id
        .as_deref()
        .and_then(|s| AgentId::new(s).ok())
        .unwrap_or_else(AgentId::claude_code);
    let agent_capabilities = capabilities_for_agent_id(&agents, agent_id.as_str());

    // Non-git workspaces can't have worktrees — every session shares the
    // workspace cwd (parallel sessions step on each other; the sidebar
    // badge communicates the trade-off).
    let session = if is_first || !crate::worktree::is_git_repo(&repo_path) {
        // No slow work on this path — keep the read (is_first) and the write
        // under one continuous guard; there's no worktree race to guard
        // against here, so dropping would only add overhead.
        let session = build_new_session(
            session_id,
            name,
            workspace_id,
            None,
            None,
            ts,
            &agent_id,
            agent_capabilities,
            launch_options,
            env_shells,
        );
        guard.create_session(&session).map_err(|e| e.to_string())?;
        session
    } else {
        // is_first is already false here, and re-deriving it after the drop
        // cannot flip it back — dropping the guard for create_worktree (the
        // only slow step) needs no re-validation dance.
        drop(guard);
        let slug = derive_worktree_slug(&name, ts);
        let wt_path =
            crate::worktree::create_worktree(&repo_path, &slug).map_err(|e| e.to_string())?;
        let branch = format!("nergal/{slug}");
        let session = build_new_session(
            session_id,
            name,
            workspace_id,
            Some(wt_path),
            Some(branch),
            ts,
            &agent_id,
            agent_capabilities,
            launch_options,
            env_shells,
        );
        let guard = db.lock().map_err(|e| e.to_string())?;
        guard.create_session(&session).map_err(|e| e.to_string())?;
        session
    };

    // Populate the agent_id cache BEFORE the PTY spawn so the SessionStart
    // hook never races the cache. Until the session-creation flow exposes a
    // picker (commit 11), every new session is a CC session by default.
    agents.register_session(&session.id, agent_id.clone());

    extend_plan_watcher_for_session(&agents, &plan_watcher, &session, &repo_path);
    Ok(session)
}

#[tauri::command]
pub async fn delete_session(
    db: State<'_, SharedDb>,
    agents: State<'_, AgentRuntimeState>,
    session_id: String,
) -> Result<(), String> {
    // Resolve adapter for stop_event_pump before tearing down the cache /
    // worktree / DB row, since stop_event_pump may need the adapter's per-
    // session state to be still intact (e.g. OpenCode supervisor stop kills
    // the running `opencode serve` child).
    if let Some(agent_id) = agents.resolve(&session_id)
        && let Some(adapter) = agents.registry.get(&agent_id)
        && let Err(e) = adapter.stop_event_pump(&session_id).await
    {
        tracing::warn!(
            session_id = %session_id,
            agent = %agent_id,
            error = %e,
            "adapter.stop_event_pump failed; session teardown continues",
        );
    }

    // Scoped guard: extract everything the (potentially slow) worktree removal
    // and the footer/MOC file I/O need, then let the guard drop at block end
    // so all of it runs with the DB unlocked for every other caller.
    let (worktree_cleanup, footer_job, moc_job) = {
        let db = db.lock().map_err(|e| e.to_string())?;

        // Get session + workspace for worktree cleanup
        let session = db.find_session(&session_id).map_err(|e| e.to_string())?;

        // Synchronous because the detached runner can't help here: it runs after
        // the delete, and its MOC git diff needs the worktree still present.
        // claim_finalization dedups against the PTY-EOF trigger firing as the PTY
        // tears down, so the footer isn't appended twice.
        let mut footer_job = None;
        let mut moc_job = None;
        if let Some(s) = &session
            && let Ok(cfg) =
                crate::obsidian::config::resolve(&s.workspace_id, |w| db.get_obsidian_config(w))
            && crate::obsidian::post_session::claim_finalization(&session_id)
        {
            let footer_tasks_done =
                crate::hooks::server::gather_footer_tasks_done(&db, &cfg, &session_id);
            let moc_inputs = if cfg.moc_path.as_deref().filter(|p| !p.is_empty()).is_some() {
                crate::obsidian::moc::MocBuilder::gather(&session_id, &cfg, &db)
                    .ok()
                    .flatten()
            } else {
                None
            };
            footer_job = footer_tasks_done.map(|tasks_done| (cfg.clone(), tasks_done));
            moc_job = moc_inputs.map(|inputs| (inputs, cfg));
        }

        let mut cleanup = None;
        if let Some(session) = &session
            && let Some(wt_path) = &session.worktree_path
            && let Some(repo_path) = db
                .workspace_repo_path(&session.workspace_id)
                .map_err(|e| e.to_string())?
        {
            cleanup = Some((repo_path, wt_path.clone()));
        }
        (cleanup, footer_job, moc_job)
    };

    if let Some((cfg, tasks_done)) = footer_job {
        crate::hooks::server::write_footer(&cfg, &session_id, tasks_done);
    }
    if let Some((inputs, cfg)) = moc_job
        && let Ok(Some(moc_path)) = inputs.render_and_write(&cfg)
    {
        let _ = crate::obsidian::moc::BacklinkUpdater::propagate(&moc_path, &cfg);
    }

    if let Some((repo_path, wt_path)) = worktree_cleanup
        && let Err(e) = crate::worktree::remove_worktree(&repo_path, &wt_path)
    {
        tracing::warn!(
            session_id = %session_id,
            worktree = %wt_path.display(),
            error = %e,
            "worktree cleanup failed; session delete continues",
        );
    }

    // Fresh guard for the finalize phase: re-validate since the row may have
    // been removed by a concurrent delete while this one was unlocked above.
    let db = db.lock().map_err(|e| e.to_string())?;
    if db
        .find_session(&session_id)
        .map_err(|e| e.to_string())?
        .is_none()
    {
        return Ok(());
    }
    agents.forget_session(&session_id);
    db.delete_session(&session_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn rename_session(
    db: State<'_, SharedDb>,
    session_id: String,
    name: String,
) -> Result<(), String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    db.rename_session(&session_id, &name)
        .map_err(|e| e.to_string())
}

#[derive(Clone, serde::Serialize)]
pub struct MergeResult {
    pub success: bool,
    pub conflict: bool,
    pub message: String,
}

#[tauri::command]
pub fn merge_session(
    db: State<'_, SharedDb>,
    session_id: String,
    target_branch: String,
) -> Result<MergeResult, String> {
    let (branch, repo_path) = {
        let db = db.lock().map_err(|e| e.to_string())?;

        let Some(session) = db.find_session(&session_id).map_err(|e| e.to_string())? else {
            return Err("session not found".into());
        };

        let Some(branch) = session.worktree_branch else {
            return Err("session has no worktree branch".into());
        };

        let repo_path = db
            .workspace_repo_path(&session.workspace_id)
            .map_err(|e| e.to_string())?
            .ok_or("workspace not found")?;
        (branch, repo_path)
    }; // guard dropped here

    // Squash merge (stays on target after success)
    let commit_message = format!("squash merge {} into {}", branch, target_branch);
    if let Err(e) =
        crate::worktree::squash_merge(&repo_path, &branch, &target_branch, &commit_message)
    {
        let msg = e.to_string();
        let is_conflict = msg.starts_with("conflict:");
        return Ok(MergeResult {
            success: false,
            conflict: is_conflict,
            message: if is_conflict {
                msg.strip_prefix("conflict:").unwrap_or(&msg).to_string()
            } else {
                msg
            },
        });
    }

    Ok(MergeResult {
        success: true,
        conflict: false,
        message: format!("Squash-merged into {target_branch}"),
    })
}

/// Result of total session cleanup, surfaced to the frontend so the UI can
/// distinguish "all artifacts wiped" from "wiped most, kept the branch
/// because it was checked out elsewhere" without losing the warning thread.
/// `archived_plans_path` is `Some` when at least one plan file was copied
/// into the archive — used by the toast to point users at where their
/// session-scoped plans now live.
#[derive(Clone, serde::Serialize)]
pub struct CleanupResult {
    pub deleted: bool,
    pub warnings: Vec<String>,
    pub archived_plans_path: Option<String>,
}

/// Total deletion of a session's persisted state. Sequence:
/// 1. Archive plans from the worktree to the main repo's archive dir
///    (so they survive the worktree deletion that follows).
/// 2. Remove the worktree directory.
/// 3. Delete the branch.
/// 4. Delete the DB row.
///
/// Each step is independently best-effort: failure of one artifact is
/// logged as a warning and does NOT block deletion of the others. Returns
/// aggregated warnings so the frontend can decide whether to show a
/// non-blocking toast.
///
/// Transcript files (`~/.claude/projects/<encoded-cwd>/*.jsonl`) are owned
/// by the Claude Code CLI itself, not nergal. We do not delete them — the
/// CLI manages its own transcript lifecycle and we'd be overstepping.
#[tauri::command]
pub fn cleanup_merged_session(
    db: State<'_, SharedDb>,
    session_id: String,
) -> Result<CleanupResult, String> {
    // Scoped guard: extract session + repo_path, then drop before the
    // archive/remove_worktree/delete_branch I/O below.
    let (session, repo_path) = {
        let db = db.lock().map_err(|e| e.to_string())?;
        let Some(session) = db.find_session(&session_id).map_err(|e| e.to_string())? else {
            return Err("session not found".into());
        };
        let repo_path = db
            .workspace_repo_path(&session.workspace_id)
            .map_err(|e| e.to_string())?
            .ok_or("workspace not found")?;
        (session, repo_path)
    };

    let mut warnings: Vec<String> = Vec::new();
    let mut archived_plans_path: Option<String> = None;

    // Step 1: archive plans BEFORE removing the worktree. Source is
    // `<worktree>/.claude/plans/*.md` (Claude writes plans per-cwd, and
    // the worktree IS the cwd for this session).
    if let Some(ref wt_path) = session.worktree_path {
        let plans_src = wt_path.join(".claude").join("plans");
        match archive_plans(&plans_src, &repo_path, &session_id) {
            Ok(Some(dest)) => archived_plans_path = Some(dest.display().to_string()),
            Ok(None) => {} // no plans to archive — silent
            Err(e) => {
                let msg = format!("plan archive: {e}");
                tracing::warn!("{msg}");
                warnings.push(msg);
            }
        }
    }

    // Step 2: remove the worktree directory.
    if let Some(ref wt_path) = session.worktree_path
        && let Err(e) = crate::worktree::remove_worktree(&repo_path, wt_path)
    {
        let msg = format!("worktree remove: {e}");
        tracing::warn!("{msg}");
        warnings.push(msg);
    }

    // Step 3: delete the branch.
    if let Some(ref branch) = session.worktree_branch
        && let Err(e) = crate::worktree::delete_branch(&repo_path, branch)
    {
        let msg = format!("branch delete: {e}");
        tracing::warn!("{msg}");
        warnings.push(msg);
    }

    // Step 4: delete the DB row. Re-acquire and re-validate — the row may
    // have been removed by a concurrent delete while this one was unlocked.
    let db = db.lock().map_err(|e| e.to_string())?;
    if db
        .find_session(&session_id)
        .map_err(|e| e.to_string())?
        .is_some()
        && let Err(e) = db.delete_session(&session_id)
    {
        let msg = format!("db delete: {e}");
        tracing::warn!("{msg}");
        warnings.push(msg);
    }

    Ok(CleanupResult {
        deleted: true,
        warnings,
        archived_plans_path,
    })
}

/// Copy `*.md` files from `<worktree>/.claude/plans/` into
/// `<main_repo>/.claude/plans/archive/YYYY-MM/<session_id>/`. Returns the
/// destination dir on success (Some when any files were copied, None when
/// the source dir was missing or empty). Appends `-N` to the destination
/// dir name if a collision exists. Failures inside the copy are bubbled
/// up to the caller as Err so they can be surfaced as warnings without
/// blocking the rest of cleanup.
fn archive_plans(
    plans_src: &std::path::Path,
    main_repo: &std::path::Path,
    session_id: &str,
) -> anyhow::Result<Option<std::path::PathBuf>> {
    use anyhow::Context;

    if !plans_src.exists() {
        return Ok(None);
    }

    let entries: Vec<_> = std::fs::read_dir(plans_src)
        .with_context(|| format!("read_dir {}", plans_src.display()))?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map(|ext| ext == "md").unwrap_or(false))
        .collect();

    if entries.is_empty() {
        return Ok(None);
    }

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // Format YYYY-MM from epoch seconds — avoid pulling chrono just for this.
    let month = epoch_secs_to_year_month(now);
    let archive_root = main_repo
        .join(".claude")
        .join("plans")
        .join("archive")
        .join(&month);

    // Collision-safe destination: if `<archive_root>/<session_id>` exists,
    // append `-1`, `-2`, ... until a free path is found.
    let mut dest = archive_root.join(session_id);
    let mut suffix = 1;
    while dest.exists() {
        dest = archive_root.join(format!("{session_id}-{suffix}"));
        suffix += 1;
    }

    std::fs::create_dir_all(&dest).with_context(|| format!("create_dir_all {}", dest.display()))?;

    for entry in entries {
        let src_path = entry.path();
        let file_name = src_path
            .file_name()
            .ok_or_else(|| anyhow::anyhow!("no file name"))?;
        let dest_path = dest.join(file_name);
        std::fs::copy(&src_path, &dest_path)
            .with_context(|| format!("copy {} → {}", src_path.display(), dest_path.display()))?;
    }

    Ok(Some(dest))
}

/// Convert epoch seconds to a `YYYY-MM` string. Pure date math (Gregorian)
/// good for the next thousand years — sufficient for an archive folder name.
fn epoch_secs_to_year_month(secs: u64) -> String {
    // Days since 1970-01-01.
    let days = (secs / 86_400) as i64;
    // Algorithm: shift epoch to 0000-03-01 (which begins a 400-year cycle).
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if m <= 2 { y + 1 } else { y };
    format!("{year:04}-{m:02}")
}

#[tauri::command]
pub fn update_session_env_shells(
    db: State<'_, SharedDb>,
    session_id: String,
    env_shells: Vec<crate::models::EnvShellDef>,
) -> Result<(), String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    db.update_session_env_shells(&session_id, &env_shells)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_workspace_env_shell_suggestions(
    db: State<'_, SharedDb>,
    workspace_id: String,
) -> Result<Vec<crate::models::EnvShellDef>, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    db.get_workspace_env_shell_suggestions(&workspace_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_workspace_env_shell_suggestions(
    db: State<'_, SharedDb>,
    workspace_id: String,
    suggestions: Vec<crate::models::EnvShellDef>,
) -> Result<(), String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let cleaned: Vec<crate::models::EnvShellDef> = suggestions
        .into_iter()
        .filter(|s| !s.command.trim().is_empty())
        .collect();
    db.set_workspace_env_shell_suggestions(&workspace_id, &cleaned)
        .map_err(|e| e.to_string())
}

// -- Agent registry commands --

#[derive(serde::Serialize)]
pub struct AvailableAgent {
    pub id: String,
    pub display_name: String,
    pub installed: bool,
    pub binary_path: Option<String>,
    pub config_path: Option<String>,
    pub version: Option<String>,
    pub capabilities: Vec<String>,
    /// Kebab-case wire form of the adapter's supported permission presets
    /// (`["default", "plan", …]`). Drives the launch-options UI in the
    /// agent picker.
    pub permission_presets: Vec<String>,
    /// Whether the adapter maps `allow_skip_in_cycle` to a real flag (CC
    /// `--allow-dangerously-skip-permissions`).
    pub allow_skip_cycle_supported: bool,
}

/// Returns the registered adapters with their current detection status. Used
/// by the session-creation modal's agent picker; until adapters beyond CC
/// land, this list has a single entry.
#[tauri::command]
pub async fn list_available_agents(
    agents: State<'_, AgentRuntimeState>,
) -> Result<Vec<AvailableAgent>, String> {
    let detections = agents.registry.scan().await;
    let mut out = Vec::with_capacity(detections.len());
    for (id, det) in detections {
        let adapter = match agents.registry.get(&id) {
            Some(a) => a,
            None => continue,
        };
        let cap_value = serde_json::to_value(adapter.capabilities().flags).unwrap_or_default();
        let capabilities: Vec<String> = serde_json::from_value(cap_value).unwrap_or_default();
        let permission_presets: Vec<String> = adapter
            .permission_presets()
            .iter()
            .filter_map(|p| {
                serde_json::to_value(p)
                    .ok()
                    .and_then(|v| v.as_str().map(String::from))
            })
            .collect();
        out.push(AvailableAgent {
            id: id.as_str().to_string(),
            display_name: adapter.display_name().to_string(),
            installed: det.installed,
            binary_path: det.binary_path.map(|p| p.display().to_string()),
            config_path: det.config_path.map(|p| p.display().to_string()),
            version: det.version,
            capabilities,
            permission_presets,
            allow_skip_cycle_supported: adapter.supports_allow_skip_cycle(),
        });
    }
    Ok(out)
}

/// Push nergal's active palette to every adapter that advertises
/// `THEME_SYNC`. Invoked from the frontend `applyTheme` flow after the DOM
/// `data-theme` mutation commits. Failures are logged inside the registry
/// dispatcher — the command always returns `Ok(())` so the UI never surfaces
/// theme-sync errors to the user.
#[tauri::command]
pub async fn apply_theme_to_agents(
    agents: State<'_, AgentRuntimeState>,
    palette: ThemePalette,
) -> Result<(), String> {
    agents.registry.apply_theme_to_all(palette).await;
    Ok(())
}

/// Resolve the default agent for a project, applying the documented priority:
/// `config.agent_overrides[project] > config.default_agent > CC fallback`.
/// The picker UI calls this on open to pre-select the right entry.
#[tauri::command]
pub fn resolve_default_agent(project_path: String) -> Result<String, String> {
    let cfg = crate::config::Config::load();
    Ok(cfg
        .resolve_agent_for_project(std::path::Path::new(&project_path))
        .unwrap_or_else(|| AgentId::claude_code().as_str().to_string()))
}
