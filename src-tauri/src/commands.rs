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
pub fn approve_plan(session_id: String, state: State<'_, SharedPlanState>) -> Result<(), String> {
    let mut mgr = state.lock().map_err(|e| e.to_string())?;
    let runtime = mgr.get_or_create(&session_id);
    if let Some(plan) = &mut runtime.current_plan {
        plan.original = plan.content.clone();
    }
    Ok(())
}

#[tauri::command]
pub fn reject_plan(session_id: String, state: State<'_, SharedPlanState>) -> Result<(), String> {
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

/// Writes user answers to the FIFO, unblocking the ask-user CLI.
/// `answers` is a JSON string mapping question text to selected answer.
#[tauri::command]
pub fn submit_ask_answer(decision_path: String, answers: String) -> Result<(), String> {
    let answers_value: serde_json::Value =
        serde_json::from_str(&answers).map_err(|e| format!("parsing answers JSON: {e}"))?;
    let response = serde_json::json!({ "answers": answers_value });
    std::fs::write(
        &decision_path,
        serde_json::to_string(&response).map_err(|e| e.to_string())?,
    )
    .map_err(|e| format!("writing answers to FIFO: {e}"))?;
    Ok(())
}

/// Writes approval/denial decision to the FIFO, unblocking the plan-review CLI.
#[tauri::command]
pub fn submit_plan_decision(
    session_id: String,
    decision_path: String,
    approved: bool,
    feedback: Option<String>,
    state: State<'_, SharedPlanState>,
) -> Result<(), String> {
    // If plan was edited, save to disk first
    if let Ok(mut mgr) = state.lock() {
        let runtime = mgr.get_or_create(&session_id);
        if let Some(plan) = &runtime.current_plan {
            if plan.content != plan.original {
                let _ = runtime.save_edits(plan.content.clone());
            }
        }
    }

    let decision = if approved {
        serde_json::json!({ "approved": true })
    } else {
        let msg = feedback.unwrap_or_else(|| "Plan changes requested".to_string());
        let deny_msg = format!(
            "YOUR PLAN WAS NOT APPROVED.\n\n\
             You MUST revise the plan to address ALL of the feedback below before calling ExitPlanMode again.\n\n\
             Rules:\n\
             - Do not resubmit the same plan unchanged.\n\
             - Do NOT change the plan title (first # heading) unless the user explicitly asks you to.\n\n\
             {msg}"
        );
        serde_json::json!({ "approved": false, "message": deny_msg })
    };

    std::fs::write(
        &decision_path,
        serde_json::to_string(&decision).map_err(|e| e.to_string())?,
    )
    .map_err(|e| format!("writing decision to FIFO: {e}"))?;

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
    let plan = runtime.current_plan.as_ref().ok_or("plan was not loaded")?;
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

// -- Annotation commands --

#[tauri::command]
pub fn save_annotation(
    id: String,
    session_id: String,
    ann_type: String,
    target: String,
    content: String,
    start_meta: String,
    end_meta: String,
    db: State<'_, SharedDb>,
) -> Result<(), String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    db.save_annotation(
        &id,
        &session_id,
        &ann_type,
        &target,
        &content,
        &start_meta,
        &end_meta,
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_annotations(
    session_id: String,
    db: State<'_, SharedDb>,
) -> Result<Vec<crate::db::AnnotationRow>, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    db.get_annotations(&session_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_annotation(id: String, db: State<'_, SharedDb>) -> Result<(), String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    db.delete_annotation(&id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn clear_annotations(session_id: String, db: State<'_, SharedDb>) -> Result<(), String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    db.clear_annotations(&session_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_pending_annotations(feedback: String) -> Result<(), String> {
    if feedback.is_empty() {
        return Ok(());
    }
    HookState::set_pending_annotations(feedback).map_err(|e| e.to_string())
}

// -- Spec annotation commands --

#[tauri::command]
pub fn save_spec_annotation(
    id: String,
    spec_key: String,
    ann_type: String,
    target: String,
    content: String,
    start_meta: String,
    end_meta: String,
    db: State<'_, SharedDb>,
) -> Result<(), String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    db.save_spec_annotation(
        &id,
        &spec_key,
        &ann_type,
        &target,
        &content,
        &start_meta,
        &end_meta,
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_spec_annotations(
    spec_key: String,
    db: State<'_, SharedDb>,
) -> Result<Vec<crate::db::SpecAnnotationRow>, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    db.get_spec_annotations(&spec_key)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_spec_annotation(id: String, db: State<'_, SharedDb>) -> Result<(), String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    db.delete_spec_annotation(&id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn clear_spec_annotations(spec_key: String, db: State<'_, SharedDb>) -> Result<(), String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    db.clear_spec_annotations(&spec_key)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn count_spec_annotations_by_prefix(
    prefix: String,
    db: State<'_, SharedDb>,
) -> Result<Vec<(String, i64)>, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let like = format!("{}%", prefix.replace('%', "\\%"));
    db.count_spec_annotations_by_prefix(&like)
        .map_err(|e| e.to_string())
}

// -- Buddy commands --

#[derive(Clone, serde::Serialize)]
pub struct BuddyData {
    soul: Option<serde_json::Value>,
    user_id: Option<String>,
    access_token: Option<String>,
}

#[tauri::command]
pub fn get_buddy() -> Result<BuddyData, String> {
    let home = dirs::home_dir().ok_or("cannot determine home directory")?;

    // Read soul from ~/.claude.json
    let claude_json = home.join(".claude.json");
    let (soul, user_id) = match std::fs::read_to_string(&claude_json) {
        Ok(contents) => {
            let json: serde_json::Value =
                serde_json::from_str(&contents).map_err(|e| e.to_string())?;
            let soul = json.get("companion").cloned();
            let user_id = json
                .get("userID")
                .and_then(|v| v.as_str())
                .map(String::from);
            (soul, user_id)
        }
        Err(_) => (None, None),
    };

    // Read access token from ~/.claude/.credentials.json for OAuth profile fetch
    let creds_json = home.join(".claude").join(".credentials.json");
    let access_token = std::fs::read_to_string(&creds_json)
        .ok()
        .and_then(|contents| serde_json::from_str::<serde_json::Value>(&contents).ok())
        .and_then(|json| {
            json.get("claudeAiOauth")
                .and_then(|oauth| oauth.get("accessToken"))
                .and_then(|v| v.as_str())
                .map(String::from)
        });

    Ok(BuddyData {
        soul,
        user_id,
        access_token,
    })
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
    if let Err(e) =
        crate::worktree::squash_merge(&repo_path, branch, &target_branch, &commit_message)
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

/// Removes the session's worktree + branch and marks the session completed.
/// Called explicitly after the user confirms cleanup (not auto after merge).
#[tauri::command]
pub fn cleanup_merged_session(db: State<'_, SharedDb>, session_id: String) -> Result<(), String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let Some(session) = db.find_session(&session_id).map_err(|e| e.to_string())? else {
        return Err("session not found".into());
    };
    let repo_path = db
        .workspace_repo_path(&session.workspace_id)
        .map_err(|e| e.to_string())?
        .ok_or("workspace not found")?;
    if let Some(ref wt_path) = session.worktree_path {
        let _ = crate::worktree::remove_worktree(&repo_path, wt_path);
    }
    if let Some(ref branch) = session.worktree_branch {
        let _ = crate::worktree::delete_branch(&repo_path, branch);
    }
    let _ = db.update_session_status(&session_id, "completed");
    let _ = db.clear_session_worktree(&session_id);
    let _ = db.clear_merge_target(&session_id);
    Ok(())
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

    let diff_text = crate::worktree::file_diff(&cwd, &file_path).map_err(|e| e.to_string())?;

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

// -- OpenSpec commands --

/// A single OpenSpec capability spec entry.
#[derive(Clone, serde::Serialize)]
pub struct SpecEntry {
    pub name: String,
    pub path: String,
}

/// An OpenSpec change with its artifacts.
#[derive(Clone, serde::Serialize)]
pub struct OpenSpecChange {
    pub name: String,
    pub status: String,
    pub created: String,
    pub artifacts: Vec<String>,
    pub specs: Vec<SpecEntry>,
}

/// Resolve session working directory.
fn resolve_session_cwd(db: &crate::db::Database, session_id: &str) -> Result<PathBuf, String> {
    let Some(session) = db
        .find_session(session_id)
        .map_err(|e: anyhow::Error| e.to_string())?
    else {
        return Err("session not found".into());
    };
    if let Some(ref wt) = session.worktree_path {
        Ok(wt.clone())
    } else {
        let path: Option<PathBuf> = db
            .workspace_repo_path(&session.workspace_id)
            .map_err(|e: anyhow::Error| e.to_string())?;
        path.ok_or_else(|| "workspace not found".into())
    }
}

fn scan_change_dir(dir: &std::path::Path, status: &str) -> Option<OpenSpecChange> {
    if !dir.is_dir() {
        return None;
    }
    let name = dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();

    // Skip hidden dirs
    if name.starts_with('.') {
        return None;
    }

    let created = dir
        .join(".openspec.yaml")
        .exists()
        .then(|| {
            std::fs::read_to_string(dir.join(".openspec.yaml"))
                .ok()
                .and_then(|s| {
                    s.lines()
                        .find(|l| l.starts_with("created:"))
                        .map(|l| l.trim_start_matches("created:").trim().to_string())
                })
                .unwrap_or_default()
        })
        .unwrap_or_default();

    // Scan all .md files in the change directory as artifacts
    let mut artifacts = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries {
            let Ok(entry) = entry else { continue };
            let path = entry.path();
            if path.is_file() {
                if let Some(ext) = path.extension() {
                    if ext == "md" {
                        if let Some(name) = path.file_stem().and_then(|s| s.to_str()) {
                            artifacts.push(name.to_string());
                        }
                    }
                }
            }
        }
    }
    // Stable ordering: proposal first, then design, implementation, tasks, rest alphabetically
    let priority = |name: &str| -> usize {
        match name {
            "proposal" => 0,
            "design" => 1,
            "implementation" => 2,
            "tasks" => 3,
            _ => 4,
        }
    };
    artifacts.sort_by(|a, b| priority(a).cmp(&priority(b)).then_with(|| a.cmp(b)));

    let mut specs = Vec::new();
    let specs_dir = dir.join("specs");
    if specs_dir.is_dir() {
        if let Ok(entries) = std::fs::read_dir(&specs_dir) {
            for entry in entries {
                let Ok(entry) = entry else { continue };
                let path = entry.path();
                if path.is_dir() && path.join("spec.md").exists() {
                    let spec_name = path
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    let spec_path = format!("specs/{spec_name}/spec.md");
                    specs.push(SpecEntry {
                        name: spec_name,
                        path: spec_path,
                    });
                }
            }
        }
    }
    specs.sort_by(|a, b| a.name.cmp(&b.name));

    Some(OpenSpecChange {
        name,
        status: status.to_string(),
        created,
        artifacts,
        specs,
    })
}

/// List all OpenSpec changes (active + archived) for a session's project.
#[tauri::command]
pub fn list_openspec_changes(
    db: State<'_, SharedDb>,
    session_id: String,
) -> Result<Vec<OpenSpecChange>, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let cwd = resolve_session_cwd(&db, &session_id)?;
    let changes_dir = cwd.join("openspec").join("changes");

    if !changes_dir.exists() {
        return Ok(vec![]);
    }

    let mut changes = Vec::new();

    // Scan active changes (direct children of changes/)
    if let Ok(entries) = std::fs::read_dir(&changes_dir) {
        for entry in entries {
            let Ok(entry) = entry else { continue };
            let path = entry.path();
            if path.file_name().map(|n| n == "archive").unwrap_or(false) {
                continue;
            }
            if let Some(change) = scan_change_dir(&path, "active") {
                changes.push(change);
            }
        }
    }

    // Scan archived changes
    let archive_dir = changes_dir.join("archive");
    if archive_dir.is_dir() {
        if let Ok(entries) = std::fs::read_dir(&archive_dir) {
            for entry in entries {
                let Ok(entry) = entry else { continue };
                if let Some(change) = scan_change_dir(&entry.path(), "archived") {
                    changes.push(change);
                }
            }
        }
    }

    // Also scan master specs as a virtual "specs" entry
    let master_dir = cwd.join("openspec").join("specs");
    if master_dir.is_dir() {
        let mut master_specs = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&master_dir) {
            for entry in entries {
                let Ok(entry) = entry else { continue };
                let path = entry.path();
                if path.is_dir() && path.join("spec.md").exists() {
                    let spec_name = path
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    master_specs.push(SpecEntry {
                        name: spec_name.clone(),
                        path: format!("specs/{spec_name}/spec.md"),
                    });
                }
            }
        }
        master_specs.sort_by(|a, b| a.name.cmp(&b.name));
        if !master_specs.is_empty() {
            changes.push(OpenSpecChange {
                name: "_master".to_string(),
                status: "master".to_string(),
                created: String::new(),
                artifacts: Vec::new(),
                specs: master_specs,
            });
        }
    }

    // Active first, then archived, then master; within each group, sort by name
    changes.sort_by(|a, b| a.status.cmp(&b.status).then(a.name.cmp(&b.name)));

    Ok(changes)
}

/// Read a specific artifact file from an OpenSpec change.
#[tauri::command]
pub fn read_openspec_artifact(
    db: State<'_, SharedDb>,
    session_id: String,
    change_name: String,
    artifact_path: String,
) -> Result<String, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let cwd = resolve_session_cwd(&db, &session_id)?;
    let openspec_dir = cwd.join("openspec");

    // Master specs live at openspec/specs/
    let file_path = if change_name == "_master" {
        openspec_dir.join(&artifact_path)
    } else {
        let changes_dir = openspec_dir.join("changes");
        // Try active first, then archive
        let change_dir = changes_dir.join(&change_name);
        if change_dir.exists() {
            change_dir.join(&artifact_path)
        } else {
            changes_dir
                .join("archive")
                .join(&change_name)
                .join(&artifact_path)
        }
    };

    std::fs::read_to_string(&file_path)
        .map_err(|e| format!("failed to read {}: {e}", file_path.display()))
}

/// Write content to an artifact file in an active OpenSpec change.
/// Rejects writes to archived changes and master specs.
#[tauri::command]
pub fn write_openspec_artifact(
    db: State<'_, SharedDb>,
    session_id: String,
    change_name: String,
    artifact_path: String,
    content: String,
) -> Result<(), String> {
    if change_name == "_master" {
        return Err("master specs are read-only".into());
    }

    let db = db.lock().map_err(|e| e.to_string())?;
    let cwd = resolve_session_cwd(&db, &session_id)?;
    let change_dir = cwd.join("openspec").join("changes").join(&change_name);

    if !change_dir.exists() {
        return Err("change not found or is archived".into());
    }

    let file_path = change_dir.join(&artifact_path);

    // Ensure parent directory exists (for new spec files)
    if let Some(parent) = file_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("failed to create directory: {e}"))?;
    }

    std::fs::write(&file_path, &content)
        .map_err(|e| format!("failed to write {}: {e}", file_path.display()))
}

// -- Editor commands --

/// (id, display_name, &[command_candidates])
/// First candidate found in PATH wins.
const KNOWN_EDITORS: &[(&str, &str, &[&str])] = &[
    ("zed", "Zed", &["zed"]),
    ("code", "VS Code", &["code"]),
    ("cursor", "Cursor", &["cursor"]),
    ("windsurf", "Windsurf", &["windsurf"]),
    ("antigravity", "Antigravity", &["antigravity"]),
    ("webstorm", "WebStorm", &["webstorm"]),
    ("phpstorm", "PhpStorm", &["phpstorm"]),
    (
        "pycharm",
        "PyCharm",
        &["pycharm", "pycharm-professional", "pycharm-community"],
    ),
    (
        "idea",
        "IntelliJ IDEA",
        &["idea", "intellij-idea-ultimate", "intellij-idea-community"],
    ),
    ("clion", "CLion", &["clion"]),
    ("goland", "GoLand", &["goland"]),
    ("rustrover", "RustRover", &["rustrover", "rust-rover"]),
    ("rider", "Rider", &["rider"]),
    ("subl", "Sublime Text", &["subl"]),
    ("nvim", "Neovim", &["nvim"]),
    ("vim", "Vim", &["vim"]),
];

/// Info about an editor detected on the system.
#[derive(Clone, serde::Serialize)]
pub struct EditorInfo {
    pub id: String,
    pub name: String,
    pub command: String,
    pub available: bool,
}

/// Find the first available command candidate via `which`.
fn find_available_command(candidates: &[&str]) -> Option<String> {
    for cmd in candidates {
        let ok = std::process::Command::new("which")
            .arg(cmd)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        if ok {
            return Some(cmd.to_string());
        }
    }
    None
}

/// Detect which editors are available on the system via `which`.
#[tauri::command]
pub fn detect_editors() -> Vec<EditorInfo> {
    KNOWN_EDITORS
        .iter()
        .map(|(id, name, candidates)| {
            let resolved = find_available_command(candidates);
            EditorInfo {
                id: id.to_string(),
                name: name.to_string(),
                command: resolved.clone().unwrap_or_default(),
                available: resolved.is_some(),
            }
        })
        .collect()
}

/// Open a session's working directory (or a specific file) in an editor.
///
/// If `file_path` is given (absolute), opens that file.
/// If `spec_change_name` + `spec_artifact_path` are given, resolves via openspec dir.
/// Otherwise opens just the project directory.
#[tauri::command]
pub fn open_in_editor(
    db: State<'_, SharedDb>,
    session_id: String,
    editor_id: String,
    file_path: Option<String>,
    spec_change_name: Option<String>,
    spec_artifact_path: Option<String>,
) -> Result<(), String> {
    let candidates = KNOWN_EDITORS
        .iter()
        .find(|(id, _, _)| *id == editor_id)
        .map(|(_, _, c)| *c)
        .ok_or_else(|| format!("unknown editor: {editor_id}"))?;

    let cmd = find_available_command(candidates)
        .ok_or_else(|| format!("editor {editor_id} not found in PATH"))?;

    let db = db.lock().map_err(|e| e.to_string())?;
    let cwd = resolve_session_cwd(&db, &session_id)?;

    // Resolve the file to open
    let resolved_file = if let Some(ref fp) = file_path {
        Some(fp.clone())
    } else if let (Some(change), Some(artifact)) = (&spec_change_name, &spec_artifact_path) {
        // Resolve openspec artifact to absolute path
        let changes_dir = cwd.join("openspec").join("changes");
        let change_dir = changes_dir.join(change);
        let path = if change_dir.exists() {
            change_dir.join(artifact)
        } else {
            changes_dir.join("archive").join(change).join(artifact)
        };
        if path.exists() {
            Some(path.to_string_lossy().into_owned())
        } else {
            None
        }
    } else {
        None
    };

    let cwd_str = cwd.to_string_lossy();

    tracing::info!("open_in_editor: cmd={cmd} cwd={cwd_str} file={resolved_file:?}");

    let mut command = std::process::Command::new(&cmd);
    command.arg(cwd_str.as_ref());

    if let Some(ref fp) = resolved_file {
        command.arg(fp);
    }

    command
        .spawn()
        .map_err(|e| format!("failed to open {cmd}: {e}"))?;

    Ok(())
}

// -- Git info command --

/// Git status information for a session's working directory.
#[derive(Clone, serde::Serialize)]
pub struct GitInfo {
    pub branch: String,
    pub dirty: bool,
    pub ahead: u32,
    pub lines_added: u32,
    pub lines_removed: u32,
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
        let stat = crate::worktree::diff_shortstat(std::path::Path::new(wt_path)).unwrap_or(
            crate::worktree::DiffShortstat {
                lines_added: 0,
                lines_removed: 0,
            },
        );

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
            return Ok(GitInfo {
                branch,
                dirty,
                ahead: 0,
                lines_added: stat.lines_added,
                lines_removed: stat.lines_removed,
            });
        };

        let ahead = crate::worktree::commits_ahead_count(wt_path, main_branch).unwrap_or(0);

        Ok(GitInfo {
            branch,
            dirty,
            ahead,
            lines_added: stat.lines_added,
            lines_removed: stat.lines_removed,
        })
    } else {
        let repo_path = db
            .workspace_repo_path(&session.workspace_id)
            .map_err(|e| e.to_string())?
            .ok_or("workspace not found")?;

        let branch =
            crate::worktree::current_branch(&repo_path).unwrap_or_else(|_| "unknown".into());
        let dirty = crate::worktree::is_worktree_dirty(&repo_path).unwrap_or(false);
        let stat =
            crate::worktree::diff_shortstat(&repo_path).unwrap_or(crate::worktree::DiffShortstat {
                lines_added: 0,
                lines_removed: 0,
            });

        Ok(GitInfo {
            branch,
            dirty,
            ahead: 0,
            lines_added: stat.lines_added,
            lines_removed: stat.lines_removed,
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
        return Ok(WorktreeStatus {
            dirty: false,
            commits_ahead: false,
        });
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
        return Ok(WorktreeStatus {
            dirty,
            commits_ahead: false,
        });
    };

    let commits_ahead = crate::worktree::has_commits_ahead(wt_path, main_branch).unwrap_or(false);

    Ok(WorktreeStatus {
        dirty,
        commits_ahead,
    })
}

// -- Git panel commands --

/// Full git status with staged, unstaged, and untracked files.
#[derive(Clone, serde::Serialize)]
pub struct GitFullStatus {
    pub staged: Vec<crate::worktree::ChangedFile>,
    pub unstaged: Vec<crate::worktree::ChangedFile>,
    pub untracked: Vec<String>,
}

#[tauri::command]
pub fn get_git_status(
    db: State<'_, SharedDb>,
    session_id: String,
) -> Result<GitFullStatus, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let cwd = resolve_session_cwd(&db, &session_id)?;

    let staged = crate::worktree::staged_files(&cwd).map_err(|e| e.to_string())?;
    let unstaged = crate::worktree::unstaged_files(&cwd).map_err(|e| e.to_string())?;
    let untracked = crate::worktree::untracked_files(&cwd).map_err(|e| e.to_string())?;

    Ok(GitFullStatus {
        staged,
        unstaged,
        untracked,
    })
}

#[tauri::command]
pub fn git_stage_file(
    db: State<'_, SharedDb>,
    session_id: String,
    path: String,
) -> Result<(), String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let cwd = resolve_session_cwd(&db, &session_id)?;
    crate::worktree::stage_file(&cwd, &path).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn git_unstage_file(
    db: State<'_, SharedDb>,
    session_id: String,
    path: String,
) -> Result<(), String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let cwd = resolve_session_cwd(&db, &session_id)?;
    crate::worktree::unstage_file(&cwd, &path).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn git_stage_all(db: State<'_, SharedDb>, session_id: String) -> Result<(), String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let cwd = resolve_session_cwd(&db, &session_id)?;
    crate::worktree::stage_all(&cwd).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn git_unstage_all(db: State<'_, SharedDb>, session_id: String) -> Result<(), String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let cwd = resolve_session_cwd(&db, &session_id)?;
    crate::worktree::unstage_all(&cwd).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn git_commit(
    db: State<'_, SharedDb>,
    session_id: String,
    message: String,
) -> Result<String, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let cwd = resolve_session_cwd(&db, &session_id)?;
    crate::worktree::commit(&cwd, &message).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_recent_commits(
    db: State<'_, SharedDb>,
    session_id: String,
    count: u32,
) -> Result<Vec<crate::worktree::CommitEntry>, String> {
    let db = db.lock().map_err(|e| e.to_string())?;

    let Some(session) = db
        .find_session(&session_id)
        .map_err(|e: anyhow::Error| e.to_string())?
    else {
        return Err("session not found".into());
    };

    let cwd = resolve_session_cwd(&db, &session_id)?;

    // For worktree sessions, show only session commits (main..HEAD)
    let range = if session.worktree_path.is_some() {
        let repo_path = db
            .workspace_repo_path(&session.workspace_id)
            .map_err(|e: anyhow::Error| e.to_string())?
            .ok_or("workspace not found")?;
        let branches = crate::worktree::list_branches(&repo_path).map_err(|e| e.to_string())?;
        if branches.iter().any(|b| b == "main") {
            Some("main..HEAD".to_string())
        } else if branches.iter().any(|b| b == "master") {
            Some("master..HEAD".to_string())
        } else {
            None
        }
    } else {
        None
    };

    crate::worktree::recent_commits(&cwd, count, range.as_deref()).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_pr_status(
    db: State<'_, SharedDb>,
    session_id: String,
) -> Result<Option<crate::worktree::PrInfo>, String> {
    let db = db.lock().map_err(|e| e.to_string())?;

    let Some(session) = db
        .find_session(&session_id)
        .map_err(|e: anyhow::Error| e.to_string())?
    else {
        return Err("session not found".into());
    };

    let Some(ref branch) = session.worktree_branch else {
        return Ok(None);
    };

    let cwd = resolve_session_cwd(&db, &session_id)?;
    crate::worktree::pr_status(&cwd, branch).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn create_pr(
    db: State<'_, SharedDb>,
    session_id: String,
    title: String,
    body: String,
) -> Result<crate::worktree::PrInfo, String> {
    let db = db.lock().map_err(|e| e.to_string())?;

    let Some(session) = db
        .find_session(&session_id)
        .map_err(|e: anyhow::Error| e.to_string())?
    else {
        return Err("session not found".into());
    };

    let Some(ref branch) = session.worktree_branch else {
        return Err("session has no worktree branch".into());
    };

    let cwd = resolve_session_cwd(&db, &session_id)?;

    let repo_path = db
        .workspace_repo_path(&session.workspace_id)
        .map_err(|e: anyhow::Error| e.to_string())?
        .ok_or("workspace not found")?;

    let branches = crate::worktree::list_branches(&repo_path).map_err(|e| e.to_string())?;
    let base = if branches.iter().any(|b| b == "main") {
        "main"
    } else {
        "master"
    };

    crate::worktree::create_pr(&cwd, branch, base, &title, &body).map_err(|e| e.to_string())
}

// ── Ship flow: push, ship, PR preview, CI checks, conflicts ──

fn resolve_session_base(db: &crate::db::Database, session_id: &str) -> Result<String, String> {
    let Some(session) = db
        .find_session(session_id)
        .map_err(|e: anyhow::Error| e.to_string())?
    else {
        return Err("session not found".into());
    };
    let repo_path = db
        .workspace_repo_path(&session.workspace_id)
        .map_err(|e: anyhow::Error| e.to_string())?
        .ok_or("workspace not found")?;
    let branches = crate::worktree::list_branches(&repo_path).map_err(|e| e.to_string())?;
    Ok(if branches.iter().any(|b| b == "main") {
        "main".into()
    } else {
        "master".into()
    })
}

fn resolve_session_branch(db: &crate::db::Database, session_id: &str) -> Result<String, String> {
    let Some(session) = db
        .find_session(session_id)
        .map_err(|e: anyhow::Error| e.to_string())?
    else {
        return Err("session not found".into());
    };
    if let Some(ref b) = session.worktree_branch {
        Ok(b.clone())
    } else {
        let cwd = resolve_session_cwd(db, session_id)?;
        crate::worktree::current_branch(&cwd).map_err(|e| e.to_string())
    }
}

#[tauri::command]
pub fn pull_target_into_session(
    db: State<'_, SharedDb>,
    session_id: String,
    target: String,
) -> Result<Vec<String>, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let cwd = resolve_session_cwd(&db, &session_id)?;
    crate::worktree::pull_target_into_worktree(&cwd, &target).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn complete_pending_merge(
    db: State<'_, SharedDb>,
    session_id: String,
) -> Result<String, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let cwd = resolve_session_cwd(&db, &session_id)?;
    crate::worktree::complete_pending_merge(&cwd).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn has_pending_merge(db: State<'_, SharedDb>, session_id: String) -> Result<bool, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let cwd = resolve_session_cwd(&db, &session_id)?;
    Ok(crate::worktree::has_pending_merge(&cwd))
}

#[tauri::command]
pub fn enable_pr_auto_merge(
    db: State<'_, SharedDb>,
    session_id: String,
    pr_number: u32,
) -> Result<(), String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let cwd = resolve_session_cwd(&db, &session_id)?;
    crate::worktree::enable_pr_auto_merge(&cwd, pr_number).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn git_push(db: State<'_, SharedDb>, session_id: String) -> Result<bool, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let cwd = resolve_session_cwd(&db, &session_id)?;
    let branch = resolve_session_branch(&db, &session_id)?;
    crate::worktree::push(&cwd, &branch).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn gh_available() -> bool {
    crate::worktree::gh_available()
}

#[tauri::command]
pub fn get_pr_preview_data(
    db: State<'_, SharedDb>,
    session_id: String,
) -> Result<crate::worktree::PrPreviewData, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let cwd = resolve_session_cwd(&db, &session_id)?;
    let base = resolve_session_base(&db, &session_id)?;
    crate::worktree::pr_preview_data(&cwd, &base, "HEAD").map_err(|e| e.to_string())
}

#[tauri::command]
pub fn poll_pr_checks(
    db: State<'_, SharedDb>,
    session_id: String,
) -> Result<Option<crate::worktree::PrChecks>, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let cwd = resolve_session_cwd(&db, &session_id)?;
    let branch = resolve_session_branch(&db, &session_id)?;
    let Some(pr) = crate::worktree::pr_status(&cwd, &branch).map_err(|e| e.to_string())? else {
        return Ok(None);
    };
    if pr.state != "OPEN" {
        return Ok(None);
    }
    crate::worktree::pr_checks(&cwd, pr.number)
        .map(Some)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_conflicted_files(
    db: State<'_, SharedDb>,
    session_id: String,
) -> Result<Vec<String>, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let cwd = resolve_session_cwd(&db, &session_id)?;
    crate::worktree::conflicted_files(&cwd).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_file_conflict_versions(
    db: State<'_, SharedDb>,
    session_id: String,
    path: String,
) -> Result<crate::worktree::ConflictVersions, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let cwd = resolve_session_cwd(&db, &session_id)?;
    crate::worktree::file_conflict_versions(&cwd, &path).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn save_conflict_resolution(
    db: State<'_, SharedDb>,
    session_id: String,
    path: String,
    merged: String,
) -> Result<Vec<String>, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let cwd = resolve_session_cwd(&db, &session_id)?;
    let abs = cwd.join(&path);
    std::fs::write(&abs, merged).map_err(|e| format!("failed to write: {e}"))?;
    crate::worktree::stage_file(&cwd, &path).map_err(|e| e.to_string())?;
    crate::worktree::conflicted_files(&cwd).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn build_conflict_prompt(
    db: State<'_, SharedDb>,
    session_id: String,
    path: String,
    ours: String,
    theirs: String,
    original_merged: String,
    intent: Option<String>,
) -> Result<String, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let cwd = resolve_session_cwd(&db, &session_id)?;
    let branch = crate::worktree::current_branch(&cwd).unwrap_or_else(|_| "HEAD".into());

    let status_out = std::process::Command::new("git")
        .args(["status", "--short"])
        .current_dir(&cwd)
        .output();
    let status = status_out
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default();

    let intent_section = intent
        .as_deref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| format!("\n\nMy intent for this resolution:\n{s}\n"))
        .unwrap_or_default();

    let prompt = format!(
        "Resolve the merge conflict in `{path}` (worktree: `{cwd}`, branch: `{branch}`).\n\n\
         We ran `git merge --no-ff --no-commit <target>` which left MERGE_HEAD pending.\n\n\
         `git status --short`:\n```\n{status}```\n\n\
         --- ours (HEAD version) ---\n```\n{ours}\n```\n\n\
         --- theirs (incoming version) ---\n```\n{theirs}\n```\n\n\
         --- original working copy with conflict markers ---\n```\n{original_merged}\n```\
         {intent_section}\n\n\
         Steps:\n\
         1. Decide on the correct resolution, honoring my intent if stated.\n\
         2. Write the resolved contents back to `{path}` via the Edit tool.\n\
         3. Stage it with `git add {path}`.\n\
         4. If no other conflicts remain, finish the merge with `git commit --no-edit`.\n",
        cwd = cwd.display(),
    );
    Ok(prompt)
}

#[tauri::command]
pub fn enqueue_conflict_context(
    db: State<'_, SharedDb>,
    session_id: String,
    path: String,
    ours: String,
    theirs: String,
    merged: String,
    instruction: String,
) -> Result<String, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let cwd = resolve_session_cwd(&db, &session_id)?;
    let branch = crate::worktree::current_branch(&cwd).unwrap_or_else(|_| "HEAD".into());

    let status_out = std::process::Command::new("git")
        .args(["status", "--short"])
        .current_dir(&cwd)
        .output();
    let status = status_out
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default();

    let feedback = format!(
        "You are resolving a merge conflict in `{path}` inside the worktree at `{cwd}`.\n\
         Current branch: `{branch}`. We ran `git merge --no-ff --no-commit <target>` which left MERGE_HEAD pending.\n\
         {instruction}\n\n\
         `git status --short`:\n```\n{status}```\n\n\
         --- ours (HEAD version) ---\n```\n{ours}\n```\n\n\
         --- theirs (incoming version) ---\n```\n{theirs}\n```\n\n\
         --- current working copy with conflict markers ---\n```\n{merged}\n```\n\n\
         Next steps:\n\
         1. Decide on the correct resolution.\n\
         2. Write the resolved contents back to `{path}` via the Edit tool.\n\
         3. Stage it with `git add {path}`.\n\
         4. If no other conflicts remain, finish the merge with `git commit --no-edit`.\n",
        cwd = cwd.display(),
    );
    HookState::set_pending_annotations(feedback).map_err(|e| e.to_string())?;

    // Return a short seed prompt the frontend will write to the terminal.
    Ok(format!("Resolve the merge conflict in {path}"))
}

#[tauri::command]
pub fn git_ship(
    app: tauri::AppHandle,
    db: State<'_, SharedDb>,
    session_id: String,
    message: Option<String>,
    pr_title: String,
    pr_body: String,
    auto_merge: Option<bool>,
) -> Result<crate::worktree::ShipResult, String> {
    use tauri::Emitter;
    let db = db.lock().map_err(|e| e.to_string())?;
    let cwd = resolve_session_cwd(&db, &session_id)?;
    let branch = resolve_session_branch(&db, &session_id)?;
    let base = resolve_session_base(&db, &session_id)?;
    let sid = session_id.clone();
    let app_clone = app.clone();
    let on_stage = move |stage: crate::worktree::ShipStage, ok: bool| {
        let _ = app_clone.emit(
            "ship:progress",
            serde_json::json!({
                "session_id": sid,
                "stage": stage.as_str(),
                "ok": ok,
            }),
        );
    };
    let result = crate::worktree::ship(
        &cwd,
        &branch,
        &base,
        message.as_deref(),
        &pr_title,
        &pr_body,
        on_stage,
    )
    .map_err(|e| e.to_string())?;

    if auto_merge.unwrap_or(false) && result.pr_info.number > 0 {
        let _ = app.emit(
            "ship:progress",
            serde_json::json!({ "session_id": session_id, "stage": "auto-merge", "ok": true }),
        );
        if let Err(e) = crate::worktree::enable_pr_auto_merge(&cwd, result.pr_info.number) {
            let _ = app.emit(
                "ship:progress",
                serde_json::json!({ "session_id": session_id, "stage": "auto-merge", "ok": false, "error": e.to_string() }),
            );
            return Err(format!("PR created but auto-merge failed: {e}"));
        }
    }

    Ok(result)
}

// ── Git: commit files ──

#[tauri::command]
pub fn get_commit_files(
    session_id: String,
    hash: String,
    db: State<'_, SharedDb>,
) -> Result<Vec<String>, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let cwd = resolve_session_cwd(&db, &session_id)?;

    let output = std::process::Command::new("git")
        .args(["diff-tree", "--no-commit-id", "-r", "--name-only", &hash])
        .current_dir(&cwd)
        .output()
        .map_err(|e| e.to_string())?;

    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }

    let files = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| {
            // Return absolute path for DiffView compatibility
            cwd.join(l).to_string_lossy().to_string()
        })
        .collect();

    Ok(files)
}

// ── File Browser ──

#[derive(serde::Serialize)]
pub struct DirEntry {
    name: String,
    is_dir: bool,
    path: String,
}

#[tauri::command]
pub fn list_directory(
    session_id: String,
    path: String,
    db: State<'_, SharedDb>,
) -> Result<Vec<DirEntry>, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let cwd = resolve_session_cwd(&db, &session_id)?;
    let target = if path == "." {
        cwd.clone()
    } else {
        cwd.join(&path)
    };

    let mut entries = Vec::new();
    let read_dir = std::fs::read_dir(&target).map_err(|e| e.to_string())?;

    for entry in read_dir {
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        let metadata = entry.metadata().map_err(|e| e.to_string())?;
        let rel_path = if path == "." {
            name.clone()
        } else {
            format!("{}/{}", path, name)
        };
        entries.push(DirEntry {
            name,
            is_dir: metadata.is_dir(),
            path: rel_path,
        });
    }

    entries.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then(a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });

    Ok(entries)
}

#[tauri::command]
pub fn read_file_content(
    session_id: String,
    path: String,
    db: State<'_, SharedDb>,
) -> Result<String, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let cwd = resolve_session_cwd(&db, &session_id)?;
    let file_path = cwd.join(&path);
    std::fs::read_to_string(&file_path).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn write_file_content(
    session_id: String,
    path: String,
    content: String,
    db: State<'_, SharedDb>,
) -> Result<String, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let cwd = resolve_session_cwd(&db, &session_id)?;
    let file_path = cwd.join(&path);
    std::fs::write(&file_path, &content).map_err(|e| e.to_string())?;
    Ok(file_path.to_string_lossy().to_string())
}
