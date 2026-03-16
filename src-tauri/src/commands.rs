use std::path::PathBuf;

use tauri::State;

use crate::claude::cost::{self, CostSummary};
use crate::config::Config;
use crate::db::SharedDb;
use crate::hooks::state::HookState;
use crate::models::{Session, SessionStatus, Workspace};
use crate::plan_state::SharedPlanState;
use crate::tasks::Task;

// -- Config commands --

#[tauri::command]
pub fn get_config() -> Result<Config, String> {
    Ok(Config::load())
}

#[tauri::command]
pub fn save_config(config: Config) -> Result<(), String> {
    config.save().map_err(|e| e.to_string())
}

// -- Task commands --

#[tauri::command]
pub fn get_tasks(session_id: String, db: State<'_, SharedDb>) -> Result<Vec<Task>, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    db.get_visible_tasks(&session_id).map_err(|e| e.to_string())
}

// -- Plan commands --

#[derive(Clone, serde::Serialize)]
pub struct PlanResponse {
    pub path: PathBuf,
    pub content: String,
    pub has_edits: bool,
}

#[tauri::command]
pub fn get_plan(
    session_id: String,
    state: State<'_, SharedPlanState>,
) -> Result<Option<PlanResponse>, String> {
    let mut mgr = state.lock().map_err(|e| e.to_string())?;
    let runtime = mgr.get_or_create(&session_id);
    let Some(plan) = &runtime.current_plan else {
        return Ok(None);
    };
    Ok(Some(PlanResponse {
        path: plan.path.clone(),
        content: plan.content.clone(),
        has_edits: plan.has_edits(),
    }))
}

#[tauri::command]
pub fn save_plan(
    session_id: String,
    content: String,
    state: State<'_, SharedPlanState>,
) -> Result<String, String> {
    let mut mgr = state.lock().map_err(|e| e.to_string())?;
    let runtime = mgr.get_or_create(&session_id);
    let path = runtime.save_edits(content).map_err(|e| e.to_string())?;
    Ok(path.display().to_string())
}

#[tauri::command]
pub fn diff_plan(
    session_id: String,
    state: State<'_, SharedPlanState>,
) -> Result<Option<String>, String> {
    let mut mgr = state.lock().map_err(|e| e.to_string())?;
    let runtime = mgr.get_or_create(&session_id);
    let Some(plan) = &runtime.current_plan else {
        return Ok(None);
    };
    if !plan.has_edits() {
        return Ok(None);
    }
    let diff = similar::TextDiff::from_lines(&plan.original, &plan.content);
    let unified = diff
        .unified_diff()
        .context_radius(3)
        .header("original", "edited")
        .to_string();
    Ok(Some(unified))
}

#[tauri::command]
pub fn approve_plan(
    session_id: String,
    state: State<'_, SharedPlanState>,
) -> Result<(), String> {
    let mut mgr = state.lock().map_err(|e| e.to_string())?;
    let runtime = mgr.get_or_create(&session_id);
    if let Some(plan) = &mut runtime.current_plan {
        plan.original = plan.content.clone();
    }
    Ok(())
}

#[tauri::command]
pub fn reject_plan(
    session_id: String,
    state: State<'_, SharedPlanState>,
) -> Result<(), String> {
    let mut mgr = state.lock().map_err(|e| e.to_string())?;
    let runtime = mgr.get_or_create(&session_id);
    let Some(plan) = &runtime.current_plan else {
        return Ok(());
    };
    let plan_path = plan.path.clone();
    let mut hook_state = HookState::read().map_err(|e| e.to_string())?;
    hook_state.pending_plan_edit = Some(plan_path);
    hook_state.write().map_err(|e| e.to_string())?;
    Ok(())
}

// -- Plan list command --

#[derive(Clone, serde::Serialize)]
pub struct PlanSummary {
    pub name: String,
    pub path: PathBuf,
    pub modified: u64,
}

#[tauri::command]
pub fn list_plans(state: State<'_, SharedPlanState>) -> Result<Vec<PlanSummary>, String> {
    let mgr = state.lock().map_err(|e| e.to_string())?;
    let dir = mgr.plans_dir();
    if !dir.exists() {
        return Ok(vec![]);
    }

    let mut plans = Vec::new();
    for entry in std::fs::read_dir(dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        let Some(ext) = path.extension() else {
            continue;
        };
        if ext != "md" {
            continue;
        }
        let name = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let modified = entry
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);
        plans.push(PlanSummary {
            name,
            path,
            modified,
        });
    }

    plans.sort_by(|a, b| b.modified.cmp(&a.modified));
    Ok(plans)
}

#[tauri::command]
pub fn load_plan(
    session_id: String,
    path: String,
    state: State<'_, SharedPlanState>,
) -> Result<PlanResponse, String> {
    let mut mgr = state.lock().map_err(|e| e.to_string())?;
    let runtime = mgr.get_or_create(&session_id);
    let plan_path = PathBuf::from(path);
    runtime.load_plan(&plan_path).map_err(|e| e.to_string())?;
    let plan = runtime
        .current_plan
        .as_ref()
        .ok_or("plan was not loaded")?;
    Ok(PlanResponse {
        path: plan.path.clone(),
        content: plan.content.clone(),
        has_edits: plan.has_edits(),
    })
}

// -- Session list command (Claude CLI sessions, not workspace sessions) --

#[derive(Clone, serde::Serialize)]
pub struct SessionSummary {
    pub id: String,
    pub name: String,
    pub cwd: String,
    pub slug: String,
    pub modified: u64,
}

#[tauri::command]
pub fn list_sessions() -> Result<Vec<SessionSummary>, String> {
    let projects_dir = dirs::home_dir()
        .ok_or("no home dir")?
        .join(".claude")
        .join("projects");

    if !projects_dir.exists() {
        return Ok(vec![]);
    }

    let mut sessions = Vec::new();

    for project_entry in std::fs::read_dir(&projects_dir).map_err(|e| e.to_string())? {
        let project_entry = project_entry.map_err(|e| e.to_string())?;
        let project_path = project_entry.path();
        if !project_path.is_dir() {
            continue;
        }

        let project_name = project_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let cwd = project_name.replacen('-', "/", 1).replace('-', "/");

        for entry in std::fs::read_dir(&project_path).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path();

            let Some(ext) = path.extension() else {
                continue;
            };
            if ext != "jsonl" {
                continue;
            }

            let session_id = path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();

            let modified = entry
                .metadata()
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);

            let mut slug = String::new();
            let mut custom_title = String::new();

            if let Ok(file) = std::fs::File::open(&path) {
                let reader = std::io::BufReader::new(file);
                use std::io::BufRead;
                for line in reader.lines().take(200) {
                    let Ok(line) = line else { continue };
                    let Ok(val) = serde_json::from_str::<serde_json::Value>(&line) else {
                        continue;
                    };

                    if slug.is_empty()
                        && let Some(s) = val.get("slug").and_then(|v| v.as_str())
                    {
                        slug = s.to_string();
                    }

                    if val.get("type").and_then(|v| v.as_str()) == Some("custom-title")
                        && let Some(title) = val.get("customTitle").and_then(|v| v.as_str())
                    {
                        custom_title = title.to_string();
                    }
                }
            }

            let name = if !custom_title.is_empty() {
                custom_title
            } else if !slug.is_empty() {
                slug.clone()
            } else {
                session_id[..8.min(session_id.len())].to_string()
            };

            sessions.push(SessionSummary {
                id: session_id,
                name,
                cwd: cwd.clone(),
                slug,
                modified,
            });
        }
    }

    sessions.sort_by(|a, b| b.modified.cmp(&a.modified));
    Ok(sessions)
}

// -- Setup command --

#[tauri::command]
pub fn setup_hooks() -> Result<String, String> {
    crate::setup::run().map_err(|e| e.to_string())?;
    Ok("Hooks configured successfully".into())
}

// -- Cost command --

#[tauri::command]
pub fn get_cost(transcript_path: String) -> Result<CostSummary, String> {
    let path = PathBuf::from(transcript_path);
    Ok(cost::parse_cost_from_transcript(&path))
}

// -- Workspace commands --

#[tauri::command]
pub fn create_workspace(db: State<'_, SharedDb>, repo_path: String) -> Result<Workspace, String> {
    let path = PathBuf::from(&repo_path);
    if !crate::worktree::is_git_repo(&path) {
        return Err("Not a git repository".into());
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
        repo_path: path,
        sessions: Vec::new(),
        created_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
    })
}

#[tauri::command]
pub fn get_workspaces(db: State<'_, SharedDb>) -> Result<Vec<Workspace>, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    db.get_workspaces().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_workspace(db: State<'_, SharedDb>, workspace_id: String) -> Result<(), String> {
    let db = db.lock().map_err(|e| e.to_string())?;

    // Get workspace data for worktree cleanup before deletion
    let workspaces = db.get_workspaces().map_err(|e| e.to_string())?;
    let ws = workspaces.iter().find(|w| w.id == workspace_id);

    if let Some(ws) = ws {
        for session in &ws.sessions {
            if let Some(wt) = &session.worktree_path {
                let _ = crate::worktree::remove_worktree(&ws.repo_path, wt);
            }
        }
    }

    db.delete_workspace(&workspace_id)
        .map_err(|e| e.to_string())
}

// -- Session commands --

#[tauri::command]
pub fn create_session(
    db: State<'_, SharedDb>,
    workspace_id: String,
    name: String,
) -> Result<Session, String> {
    let db = db.lock().map_err(|e| e.to_string())?;

    let repo_path = db
        .workspace_repo_path(&workspace_id)
        .map_err(|e| e.to_string())?
        .ok_or("workspace not found")?;

    let is_first = db
        .session_count_for_workspace(&workspace_id)
        .map_err(|e| e.to_string())?
        == 0;

    let (worktree_path, worktree_branch) = if is_first {
        (None, None)
    } else {
        let slug: String = name
            .to_lowercase()
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
            .collect::<String>()
            .split('-')
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("-");
        let wt_path =
            crate::worktree::create_worktree(&repo_path, &slug).map_err(|e| e.to_string())?;
        let branch = format!("cluihud/{slug}");
        (Some(wt_path), Some(branch))
    };

    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let session_id = format!("{}-{ts}", &workspace_id[..6.min(workspace_id.len())]);

    let session = Session {
        id: session_id,
        name,
        workspace_id,
        worktree_path,
        worktree_branch,
        status: SessionStatus::Idle,
        created_at: ts,
        updated_at: ts,
    };

    db.create_session(&session).map_err(|e| e.to_string())?;
    Ok(session)
}

#[tauri::command]
pub fn delete_session(db: State<'_, SharedDb>, session_id: String) -> Result<(), String> {
    let db = db.lock().map_err(|e| e.to_string())?;

    // Get session + workspace for worktree cleanup
    let session = db.find_session(&session_id).map_err(|e| e.to_string())?;

    if let Some(session) = &session
        && let Some(wt_path) = &session.worktree_path
        && let Some(repo_path) = db
            .workspace_repo_path(&session.workspace_id)
            .map_err(|e| e.to_string())?
    {
        let _ = crate::worktree::remove_worktree(&repo_path, wt_path);
    }

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
