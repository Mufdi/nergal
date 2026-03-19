use std::path::PathBuf;

use tauri::State;

use crate::claude::cost::{self, CostSummary};
use crate::config::Config;
use crate::db::SharedDb;
use crate::hooks::state::HookState;
use crate::models::{Session, SessionStatus, Workspace};
use crate::plan_state::SharedPlanState;
use crate::tasks::Task;

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

fn scan_plans_dir(dir: &std::path::Path) -> Vec<PlanSummary> {
    if !dir.exists() {
        return vec![];
    }
    let mut plans = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return vec![];
    };
    for entry in entries {
        let Ok(entry) = entry else { continue };
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
    plans
}

#[tauri::command]
pub fn list_plans(state: State<'_, SharedPlanState>) -> Result<Vec<PlanSummary>, String> {
    let mgr = state.lock().map_err(|e| e.to_string())?;
    Ok(scan_plans_dir(&mgr.plans_dir()))
}

#[tauri::command]
pub fn list_session_plans(
    db: State<'_, SharedDb>,
    session_id: String,
) -> Result<Vec<PlanSummary>, String> {
    let db = db.lock().map_err(|e| e.to_string())?;

    let Some(session) = db.find_session(&session_id).map_err(|e| e.to_string())? else {
        return Ok(vec![]);
    };

    let repo_path = db
        .workspace_repo_path(&session.workspace_id)
        .map_err(|e| e.to_string())?;

    let cwd = if let Some(ref wt) = session.worktree_path {
        wt.clone()
    } else if let Some(ref rp) = repo_path {
        rp.clone()
    } else {
        return Ok(vec![]);
    };

    let project_plans_dir = cwd.join(".claude").join("plans");
    Ok(scan_plans_dir(&project_plans_dir))
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

// -- Notification command --

#[tauri::command]
pub fn send_notification(title: String, body: String) -> Result<(), String> {
    std::process::Command::new("notify-send")
        .arg("--app-name=cluihud")
        .arg("--expire-time=4000")
        .arg("--urgency=normal")
        .arg(&title)
        .arg(&body)
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
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

    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let (worktree_path, worktree_branch) = if is_first {
        (None, None)
    } else {
        let normalized = strip_diacritics(&name);
        let slug: String = normalized
            .to_lowercase()
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
            .collect::<String>()
            .split('-')
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("-");
        let slug = format!("{slug}-{ts}");
        let wt_path =
            crate::worktree::create_worktree(&repo_path, &slug).map_err(|e| e.to_string())?;
        let branch = format!("cluihud/{slug}");
        (Some(wt_path), Some(branch))
    };
    let session_id = format!("{}-{ts}", &workspace_id[..6.min(workspace_id.len())]);

    let session = Session {
        id: session_id,
        name,
        workspace_id,
        worktree_path,
        worktree_branch,
        merge_target: None,
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

#[tauri::command]
pub fn list_branches(db: State<'_, SharedDb>, workspace_id: String) -> Result<Vec<String>, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let repo_path = db
        .workspace_repo_path(&workspace_id)
        .map_err(|e| e.to_string())?
        .ok_or("workspace not found")?;
    crate::worktree::list_branches(&repo_path).map_err(|e| e.to_string())
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
    let db = db.lock().map_err(|e| e.to_string())?;

    let Some(session) = db.find_session(&session_id).map_err(|e| e.to_string())? else {
        return Err("session not found".into());
    };

    let Some(ref branch) = session.worktree_branch else {
        return Err("session has no worktree branch".into());
    };

    let repo_path = db
        .workspace_repo_path(&session.workspace_id)
        .map_err(|e| e.to_string())?
        .ok_or("workspace not found")?;

    // Squash merge (stays on target after success)
    let commit_message = format!("squash merge {} into {}", branch, target_branch);
    if let Err(e) = crate::worktree::squash_merge(&repo_path, branch, &target_branch, &commit_message) {
        let msg = e.to_string();
        let is_conflict = msg.starts_with("conflict:");
        return Ok(MergeResult {
            success: false,
            conflict: is_conflict,
            message: if is_conflict { msg.strip_prefix("conflict:").unwrap_or(&msg).to_string() } else { msg },
        });
    }

    // Remove worktree
    if let Some(ref wt_path) = session.worktree_path {
        let _ = crate::worktree::remove_worktree(&repo_path, wt_path);
    }

    // Delete branch
    let _ = crate::worktree::delete_branch(&repo_path, branch);

    // Update session status
    let _ = db.update_session_status(&session_id, "completed");
    let _ = db.clear_session_worktree(&session_id);
    let _ = db.clear_merge_target(&session_id);

    Ok(MergeResult {
        success: true,
        conflict: false,
        message: format!("Squash-merged into {target_branch}"),
    })
}

#[derive(Clone, serde::Serialize)]
pub struct TranscriptEntry {
    pub role: String,
    pub content: String,
}

fn extract_content(val: &serde_json::Value) -> String {
    if let Some(s) = val.as_str() {
        return s.to_string();
    }
    if let Some(arr) = val.as_array() {
        let mut parts = Vec::new();
        for item in arr {
            if let Some(text) = item.get("text").and_then(|t| t.as_str()) {
                parts.push(text.to_string());
            }
        }
        return parts.join("\n");
    }
    String::new()
}

#[tauri::command]
pub fn get_transcript(session_id: String) -> Result<Vec<TranscriptEntry>, String> {
    let projects_dir = dirs::home_dir()
        .ok_or("no home dir")?
        .join(".claude")
        .join("projects");

    if !projects_dir.exists() {
        return Ok(vec![]);
    }

    // Find the transcript file matching the session_id
    for project_entry in std::fs::read_dir(&projects_dir).map_err(|e| e.to_string())? {
        let project_entry = project_entry.map_err(|e| e.to_string())?;
        let project_path = project_entry.path();
        if !project_path.is_dir() {
            continue;
        }

        let transcript_path = project_path.join(format!("{session_id}.jsonl"));
        if !transcript_path.exists() {
            continue;
        }

        let file = std::fs::File::open(&transcript_path).map_err(|e| e.to_string())?;
        let reader = std::io::BufReader::new(file);
        use std::io::BufRead;

        let mut entries = Vec::new();
        for line in reader.lines() {
            let Ok(line) = line else { continue };
            let Ok(val) = serde_json::from_str::<serde_json::Value>(&line) else {
                continue;
            };

            let msg_type = val.get("type").and_then(|v| v.as_str()).unwrap_or("");
            if msg_type != "assistant" && msg_type != "human" {
                continue;
            }

            let role = msg_type.to_string();
            let content = if let Some(msg) = val.get("message") {
                if let Some(c) = msg.get("content") {
                    extract_content(c)
                } else {
                    continue;
                }
            } else {
                continue;
            };

            if content.is_empty() {
                continue;
            }

            entries.push(TranscriptEntry { role, content });
        }

        return Ok(entries);
    }

    Ok(vec![])
}

// -- Diff command --

/// Response payload for file diff queries.
#[derive(Clone, serde::Serialize)]
pub struct DiffResponse {
    pub file_path: String,
    pub diff_text: String,
    pub is_new: bool,
}

/// Return the unified diff for a single file in a session's working directory.
#[tauri::command]
pub fn get_file_diff(
    db: State<'_, SharedDb>,
    session_id: String,
    file_path: String,
) -> Result<DiffResponse, String> {
    let db = db.lock().map_err(|e| e.to_string())?;

    let Some(session) = db.find_session(&session_id).map_err(|e| e.to_string())? else {
        return Err("session not found".into());
    };

    let cwd = if let Some(ref wt) = session.worktree_path {
        wt.clone()
    } else {
        db.workspace_repo_path(&session.workspace_id)
            .map_err(|e| e.to_string())?
            .ok_or("workspace not found")?
    };

    let diff_text =
        crate::worktree::file_diff(&cwd, &file_path).map_err(|e| e.to_string())?;

    let is_new = diff_text.contains("new file mode") || diff_text.contains("/dev/null");

    Ok(DiffResponse {
        file_path,
        diff_text,
        is_new,
    })
}

// -- Changed files command --

/// Return the list of files changed in a session's working directory.
#[tauri::command]
pub fn get_session_changed_files(
    db: State<'_, SharedDb>,
    session_id: String,
) -> Result<Vec<crate::worktree::ChangedFile>, String> {
    let db = db.lock().map_err(|e| e.to_string())?;

    let Some(session) = db.find_session(&session_id).map_err(|e| e.to_string())? else {
        return Err("session not found".into());
    };

    let cwd = if let Some(ref wt) = session.worktree_path {
        wt.clone()
    } else {
        db.workspace_repo_path(&session.workspace_id)
            .map_err(|e| e.to_string())?
            .ok_or("workspace not found")?
    };

    crate::worktree::changed_files(&cwd).map_err(|e| e.to_string())
}

// -- Git info command --

/// Git status information for a session's working directory.
#[derive(Clone, serde::Serialize)]
pub struct GitInfo {
    pub branch: String,
    pub dirty: bool,
    pub ahead: u32,
}

/// Return branch name, dirty state, and commits-ahead count for a session.
#[tauri::command]
pub fn get_session_git_info(
    db: State<'_, SharedDb>,
    session_id: String,
) -> Result<GitInfo, String> {
    let db = db.lock().map_err(|e| e.to_string())?;

    let Some(session) = db.find_session(&session_id).map_err(|e| e.to_string())? else {
        return Err("session not found".into());
    };

    if let Some(ref wt_path) = session.worktree_path {
        let branch = session
            .worktree_branch
            .clone()
            .unwrap_or_else(|| "unknown".into());
        let dirty = crate::worktree::is_worktree_dirty(wt_path).unwrap_or(false);

        let repo_path = db
            .workspace_repo_path(&session.workspace_id)
            .map_err(|e| e.to_string())?
            .ok_or("workspace not found")?;

        let branches =
            crate::worktree::list_branches(&repo_path).map_err(|e| e.to_string())?;
        let main_branch = if branches.iter().any(|b| b == "main") {
            "main"
        } else if branches.iter().any(|b| b == "master") {
            "master"
        } else {
            return Ok(GitInfo {
                branch,
                dirty,
                ahead: 0,
            });
        };

        let ahead =
            crate::worktree::commits_ahead_count(wt_path, main_branch).unwrap_or(0);

        Ok(GitInfo {
            branch,
            dirty,
            ahead,
        })
    } else {
        let repo_path = db
            .workspace_repo_path(&session.workspace_id)
            .map_err(|e| e.to_string())?
            .ok_or("workspace not found")?;

        let branch =
            crate::worktree::current_branch(&repo_path).unwrap_or_else(|_| "unknown".into());
        let dirty = crate::worktree::is_worktree_dirty(&repo_path).unwrap_or(false);

        Ok(GitInfo {
            branch,
            dirty,
            ahead: 0,
        })
    }
}

/// Worktree change status for conditional button display.
#[derive(Clone, serde::Serialize)]
pub struct WorktreeStatus {
    /// Uncommitted changes exist (show commit button)
    pub dirty: bool,
    /// Commits ahead of main branch (show merge button)
    pub commits_ahead: bool,
}

/// Check a session's worktree for dirty state and commits ahead.
#[tauri::command]
pub fn check_session_has_commits(
    db: State<'_, SharedDb>,
    session_id: String,
) -> Result<WorktreeStatus, String> {
    let db = db.lock().map_err(|e| e.to_string())?;

    let Some(session) = db.find_session(&session_id).map_err(|e| e.to_string())? else {
        return Err("session not found".into());
    };

    let Some(ref wt_path) = session.worktree_path else {
        return Ok(WorktreeStatus { dirty: false, commits_ahead: false });
    };

    let dirty = crate::worktree::is_worktree_dirty(wt_path).unwrap_or(false);

    let repo_path = db
        .workspace_repo_path(&session.workspace_id)
        .map_err(|e| e.to_string())?
        .ok_or("workspace not found")?;

    let branches = crate::worktree::list_branches(&repo_path).map_err(|e| e.to_string())?;
    let main_branch = if branches.iter().any(|b| b == "main") {
        "main"
    } else if branches.iter().any(|b| b == "master") {
        "master"
    } else {
        return Ok(WorktreeStatus { dirty, commits_ahead: false });
    };

    let commits_ahead = crate::worktree::has_commits_ahead(wt_path, main_branch).unwrap_or(false);

    Ok(WorktreeStatus { dirty, commits_ahead })
}
