use std::path::PathBuf;

use tauri::{AppHandle, Emitter, State};

use crate::agents::AgentId;
use crate::agents::ThemePalette;
use crate::agents::claude_code::cost::{self, CostSummary};
use crate::agents::state::AgentRuntimeState;
use crate::agents::{PlanCapability, PlanCapabilityWire};
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

// -- Config commands --

#[tauri::command]
pub fn get_config() -> Result<Config, String> {
    Ok(Config::load())
}

#[tauri::command]
pub fn save_config(config: Config) -> Result<(), String> {
    config.save().map_err(|e| e.to_string())
}

/// Validate that a configured path resolves to something usable.
/// `kind` accepts: `dir` (must exist + be directory), `file` (must exist),
/// `executable` (PATH lookup OR absolute path that's executable).
#[derive(Clone, serde::Serialize)]
pub struct PathValidation {
    pub exists: bool,
    pub is_dir: bool,
    pub is_file: bool,
    pub is_executable: bool,
    pub resolved_path: Option<String>,
    pub error: Option<String>,
}

#[tauri::command]
pub fn validate_path(path: String, kind: String, case_insensitive: Option<bool>) -> PathValidation {
    fn expand_home(input: &str) -> PathBuf {
        if let Some(rest) = input.strip_prefix("~/")
            && let Some(home) = dirs::home_dir()
        {
            return home.join(rest);
        }
        PathBuf::from(input)
    }

    if path.trim().is_empty() {
        return PathValidation {
            exists: false,
            is_dir: false,
            is_file: false,
            is_executable: false,
            resolved_path: None,
            error: Some("path is empty".into()),
        };
    }

    if kind == "executable" && !path.contains('/') {
        let ok = std::process::Command::new("which")
            .arg(&path)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        if ok {
            let resolved = std::process::Command::new("which")
                .arg(&path)
                .output()
                .ok()
                .and_then(|o| String::from_utf8(o.stdout).ok())
                .map(|s| s.trim().to_string());
            return PathValidation {
                exists: true,
                is_dir: false,
                is_file: true,
                is_executable: true,
                resolved_path: resolved,
                error: None,
            };
        }
        return PathValidation {
            exists: false,
            is_dir: false,
            is_file: false,
            is_executable: false,
            resolved_path: None,
            error: Some(format!("'{path}' not found in PATH")),
        };
    }

    let mut resolved = expand_home(&path);
    // Mirrors the save-time normalization of Obsidian path fields, so live
    // validation doesn't reject a path that Apply would accept.
    if case_insensitive == Some(true) && std::fs::metadata(&resolved).is_err() {
        resolved = PathBuf::from(crate::obsidian::config::resolve_case_insensitive(
            &resolved.to_string_lossy(),
        ));
    }
    let metadata = std::fs::metadata(&resolved);
    match metadata {
        Ok(meta) => {
            let is_dir = meta.is_dir();
            let is_file = meta.is_file();
            let is_executable = if cfg!(unix) {
                use std::os::unix::fs::PermissionsExt;
                meta.permissions().mode() & 0o111 != 0
            } else {
                is_file
            };
            PathValidation {
                exists: true,
                is_dir,
                is_file,
                is_executable,
                resolved_path: Some(resolved.display().to_string()),
                error: None,
            }
        }
        Err(e) => PathValidation {
            exists: false,
            is_dir: false,
            is_file: false,
            is_executable: false,
            resolved_path: Some(resolved.display().to_string()),
            error: Some(e.to_string()),
        },
    }
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

#[tauri::command]
pub fn submit_ask_answer(
    decision_path: String,
    answers: String,
    feedback: Option<String>,
) -> Result<(), String> {
    let answers_value: serde_json::Value =
        serde_json::from_str(&answers).map_err(|e| format!("parsing answers JSON: {e}"))?;
    let mut response = serde_json::json!({ "answers": answers_value });
    if let Some(text) = feedback
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        && let Some(obj) = response.as_object_mut()
    {
        obj.insert(
            "feedback".to_string(),
            serde_json::Value::String(text.to_string()),
        );
    }
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
        if let Some(plan) = &runtime.current_plan
            && plan.content != plan.original
        {
            let _ = runtime.save_edits(plan.content.clone());
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

#[derive(Clone, serde::Serialize)]
#[serde(tag = "capability")]
pub enum SessionPlansResponse {
    FileBased {
        dir: String,
        plans: Vec<PlanSummary>,
    },
    NotApplicable {
        plans: Vec<PlanSummary>,
    },
}

fn session_cwd_opt(db: &crate::db::Database, session: &Session) -> Option<PathBuf> {
    if let Some(wt) = session.worktree_path.clone() {
        return Some(wt);
    }
    db.workspace_repo_path(&session.workspace_id).ok().flatten()
}

fn resolve_session_plan_capability(
    db_state: &State<'_, SharedDb>,
    agent_state: &State<'_, AgentRuntimeState>,
    session_id: &str,
) -> Result<Option<(Session, PlanCapability)>, String> {
    let db = db_state.lock().map_err(|e| e.to_string())?;
    let Some(session) = db.find_session(session_id).map_err(|e| e.to_string())? else {
        return Ok(None);
    };
    let Some(cwd) = session_cwd_opt(&db, &session) else {
        return Ok(None);
    };
    let agent_id = AgentId::new(&session.agent_id).map_err(|e| e.to_string())?;
    let Some(adapter) = agent_state.registry.get(&agent_id) else {
        return Ok(None);
    };
    let capability = adapter.plan_capability(&session, &cwd);
    Ok(Some((session, capability)))
}

#[tauri::command]
pub fn list_session_plans(
    db: State<'_, SharedDb>,
    agent_state: State<'_, AgentRuntimeState>,
    session_id: String,
) -> Result<SessionPlansResponse, String> {
    let Some((_session, capability)) =
        resolve_session_plan_capability(&db, &agent_state, &session_id)?
    else {
        return Ok(SessionPlansResponse::NotApplicable { plans: vec![] });
    };
    Ok(match capability {
        PlanCapability::FileBased { dir, .. } => SessionPlansResponse::FileBased {
            plans: scan_plans_dir(&dir),
            dir: dir.display().to_string(),
        },
        PlanCapability::NotApplicable => SessionPlansResponse::NotApplicable { plans: vec![] },
    })
}

#[tauri::command]
pub fn get_session_plan_capability(
    db: State<'_, SharedDb>,
    agent_state: State<'_, AgentRuntimeState>,
    session_id: String,
) -> Result<PlanCapabilityWire, String> {
    let Some((_, capability)) = resolve_session_plan_capability(&db, &agent_state, &session_id)?
    else {
        return Ok(PlanCapabilityWire::NotApplicable);
    };
    Ok(capability.into())
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
#[allow(clippy::too_many_arguments)] // Tauri command surface — collapsing to a struct breaks the JS call shape.
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
#[allow(clippy::too_many_arguments)] // Tauri command surface — collapsing to a struct breaks the JS call shape.
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

#[tauri::command]
pub fn init_git_repo(db: State<'_, SharedDb>, workspace_id: String) -> Result<(), String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let repo_path = db
        .workspace_repo_path(&workspace_id)
        .map_err(|e| e.to_string())?
        .ok_or("workspace not found")?;
    if crate::worktree::is_git_repo(&repo_path) {
        return Ok(());
    }
    crate::worktree::init_repo(&repo_path).map_err(|e| e.to_string())
}

/// Lets a deep-link open a file in a project that isn't a Nergal workspace yet:
/// the git root is the natural workspace root. None when the path is outside
/// any git repo — deep links don't auto-create non-git workspaces.
#[tauri::command]
pub fn resolve_repo_root(path: String) -> Option<String> {
    let start = std::path::PathBuf::from(&path);
    let mut dir = if start.is_file() {
        start.parent()?.to_path_buf()
    } else {
        start
    };
    loop {
        if dir.join(".git").exists() {
            return Some(dir.to_string_lossy().into_owned());
        }
        if !dir.pop() {
            return None;
        }
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
    let db = db.lock().map_err(|e| e.to_string())?;

    // Get workspace data for worktree cleanup before deletion
    let workspaces = db.get_workspaces().map_err(|e| e.to_string())?;
    let ws = workspaces.iter().find(|w| w.id == workspace_id);

    if let Some(ws) = ws {
        // Snapshot each session synchronously before its worktree + the
        // workspace row disappear (the detached runner runs too late).
        if let Ok(cfg) =
            crate::obsidian::config::resolve(&workspace_id, |w| db.get_obsidian_config(w))
            && cfg.moc_path.as_deref().filter(|p| !p.is_empty()).is_some()
        {
            for session in &ws.sessions {
                if let Ok(Some(moc_path)) =
                    crate::obsidian::moc::MocBuilder::build(&session.id, &cfg, &db)
                {
                    let _ = crate::obsidian::moc::BacklinkUpdater::propagate(&moc_path, &cfg);
                }
            }
        }
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

    // Non-git workspaces can't have worktrees — every session shares the
    // workspace cwd (parallel sessions step on each other; the sidebar
    // badge communicates the trade-off).
    let (worktree_path, worktree_branch) = if is_first || !crate::worktree::is_git_repo(&repo_path)
    {
        (None, None)
    } else {
        let slug = derive_worktree_slug(&name, ts);
        let wt_path =
            crate::worktree::create_worktree(&repo_path, &slug).map_err(|e| e.to_string())?;
        let branch = format!("cluihud/{slug}");
        (Some(wt_path), Some(branch))
    };
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
    };

    db.create_session(&session).map_err(|e| e.to_string())?;
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

    let db = db.lock().map_err(|e| e.to_string())?;

    // Get session + workspace for worktree cleanup
    let session = db.find_session(&session_id).map_err(|e| e.to_string())?;

    // Synchronous because the detached runner can't help here: it runs after
    // the delete, and its MOC git diff needs the worktree still present.
    // claim_finalization dedups against the PTY-EOF trigger firing as the PTY
    // tears down, so the footer isn't appended twice.
    if let Some(s) = &session
        && let Ok(cfg) =
            crate::obsidian::config::resolve(&s.workspace_id, |w| db.get_obsidian_config(w))
        && crate::obsidian::post_session::claim_finalization(&session_id)
    {
        crate::hooks::server::write_session_log_footer(&db, &cfg, &session_id);
        if cfg.moc_path.as_deref().filter(|p| !p.is_empty()).is_some()
            && let Ok(Some(moc_path)) =
                crate::obsidian::moc::MocBuilder::build(&session_id, &cfg, &db)
        {
            let _ = crate::obsidian::moc::BacklinkUpdater::propagate(&moc_path, &cfg);
        }
    }

    if let Some(session) = &session
        && let Some(wt_path) = &session.worktree_path
        && let Some(repo_path) = db
            .workspace_repo_path(&session.workspace_id)
            .map_err(|e| e.to_string())?
        && let Err(e) = crate::worktree::remove_worktree(&repo_path, wt_path)
    {
        tracing::warn!(
            session_id = %session_id,
            worktree = %wt_path.display(),
            error = %e,
            "worktree cleanup failed; session delete continues",
        );
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

#[tauri::command]
pub fn list_branches(db: State<'_, SharedDb>, workspace_id: String) -> Result<Vec<String>, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let repo_path = db
        .workspace_repo_path(&workspace_id)
        .map_err(|e| e.to_string())?
        .ok_or("workspace not found")?;
    crate::worktree::list_branches(&repo_path).map_err(|e| e.to_string())
}

/// Summary of a single PR rendered in the GitPanel PRs sidebar list.
#[derive(Clone, serde::Serialize)]
pub struct PrSummary {
    pub number: u32,
    pub title: String,
    pub state: String,
    pub url: String,
    pub base_ref_name: String,
    pub head_ref_name: String,
    pub updated_at: String,
}

/// List the workspace's pull requests via `gh pr list`. Ordered with OPEN
/// first (by `updatedAt` desc), then MERGED/CLOSED. Capped at 20.
#[tauri::command]
pub fn list_prs(db: State<'_, SharedDb>, workspace_id: String) -> Result<Vec<PrSummary>, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let repo_path = db
        .workspace_repo_path(&workspace_id)
        .map_err(|e| e.to_string())?
        .ok_or("workspace not found")?;

    let output = std::process::Command::new("gh")
        .args([
            "pr",
            "list",
            "--state",
            "all",
            "--limit",
            "20",
            "--json",
            "number,title,state,url,baseRefName,headRefName,updatedAt",
        ])
        .current_dir(&repo_path)
        .output()
        .map_err(|e| format!("failed to invoke gh: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("gh pr list failed: {stderr}"));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let raw: Vec<serde_json::Value> = serde_json::from_str(&stdout).unwrap_or_default();

    let mut prs: Vec<PrSummary> = raw
        .into_iter()
        .filter_map(|v| {
            Some(PrSummary {
                number: v.get("number")?.as_u64()? as u32,
                title: v.get("title")?.as_str()?.to_owned(),
                state: v.get("state")?.as_str()?.to_owned(),
                url: v.get("url")?.as_str()?.to_owned(),
                base_ref_name: v.get("baseRefName")?.as_str()?.to_owned(),
                head_ref_name: v.get("headRefName")?.as_str()?.to_owned(),
                updated_at: v.get("updatedAt")?.as_str()?.to_owned(),
            })
        })
        .collect();

    // OPEN first, then everything else; within each bucket, newest update first.
    prs.sort_by(|a, b| {
        let a_open = a.state == "OPEN";
        let b_open = b.state == "OPEN";
        b_open
            .cmp(&a_open)
            .then_with(|| b.updated_at.cmp(&a.updated_at))
    });

    Ok(prs)
}

/// Fetch the PR's unified diff via `gh pr diff <num>`. Returned text is
/// parsed by the frontend into chunks for rendering and chunk-by-chunk
/// navigation. Errors from `gh` are surfaced verbatim so the inline error
/// path in the PR Viewer can show them.
#[tauri::command]
pub fn get_pr_diff(
    db: State<'_, SharedDb>,
    workspace_id: String,
    pr_number: u32,
) -> Result<String, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let repo_path = db
        .workspace_repo_path(&workspace_id)
        .map_err(|e| e.to_string())?
        .ok_or("workspace not found")?;

    let output = std::process::Command::new("gh")
        .args(["pr", "diff", &pr_number.to_string()])
        .current_dir(&repo_path)
        .output()
        .map_err(|e| format!("failed to invoke gh: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("gh pr diff failed: {stderr}"));
    }

    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// CI checks for an arbitrary PR (workspace-scoped, not session-scoped). Used
/// by the PR Viewer header where the active session may not be the one that
/// owns the PR. Returns `None` when `gh pr checks` produces no parsable
/// output rather than surfacing the error: a missing CI block in the header
/// is benign and the user can retry by reopening the tab.
#[tauri::command]
pub fn get_pr_checks(
    db: State<'_, SharedDb>,
    workspace_id: String,
    pr_number: u32,
) -> Result<Option<crate::worktree::PrChecks>, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let repo_path = db
        .workspace_repo_path(&workspace_id)
        .map_err(|e| e.to_string())?
        .ok_or("workspace not found")?;
    Ok(crate::worktree::pr_checks(&repo_path, pr_number).ok())
}

/// Merge a PR via `gh pr merge <number>`. `strategy` is one of `squash`
/// (default, matches the project's PR convention), `merge`, or `rebase`.
/// Workspace-scoped (not session-scoped) so the PR Viewer can drive merges
/// for any PR the user clicks into, regardless of which session is active.
/// On `mergeable=false` (typically a conflict), the returned error string
/// contains `mergeable=false` so the frontend can switch to opening the
/// conflicts tab instead of just toasting the failure.
#[tauri::command]
pub fn gh_pr_merge(
    db: State<'_, SharedDb>,
    workspace_id: String,
    pr_number: u32,
    strategy: Option<String>,
) -> Result<(), String> {
    let strategy = strategy.unwrap_or_else(|| "squash".to_string());
    if !matches!(strategy.as_str(), "squash" | "merge" | "rebase") {
        return Err(format!("unknown merge strategy: {strategy}"));
    }

    let db = db.lock().map_err(|e| e.to_string())?;
    let cwd = db
        .workspace_repo_path(&workspace_id)
        .map_err(|e| e.to_string())?
        .ok_or("workspace not found")?;
    let strategy_flag = format!("--{strategy}");
    let pr_arg = pr_number.to_string();

    let output = std::process::Command::new("gh")
        .args(["pr", "merge", &pr_arg, &strategy_flag])
        .current_dir(&cwd)
        .output()
        .map_err(|e| format!("failed to invoke gh: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // Marker the frontend can grep for to route to the conflicts tab
        // instead of a generic error toast.
        if stderr.contains("not mergeable") || stderr.contains("merge conflict") {
            return Err(format!("mergeable=false: {stderr}"));
        }
        return Err(format!("gh pr merge failed: {stderr}"));
    }

    Ok(())
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
/// by the Claude Code CLI itself, not cluihud. We do not delete them — the
/// CLI manages its own transcript lifecycle and we'd be overstepping.
#[tauri::command]
pub fn cleanup_merged_session(
    db: State<'_, SharedDb>,
    session_id: String,
) -> Result<CleanupResult, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let Some(session) = db.find_session(&session_id).map_err(|e| e.to_string())? else {
        return Err("session not found".into());
    };
    let repo_path = db
        .workspace_repo_path(&session.workspace_id)
        .map_err(|e| e.to_string())?
        .ok_or("workspace not found")?;

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

    // Step 4: delete the DB row.
    if let Err(e) = db.delete_session(&session_id) {
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

/// Resolve a session's OpenSpec directory: the workspace override if set,
/// else `<session cwd>/openspec`. The override lets specs live outside the
/// code repo so the repo stays clean (per-workspace, migration `012`).
fn resolve_openspec_dir(db: &crate::db::Database, session_id: &str) -> Result<PathBuf, String> {
    let cwd = resolve_session_cwd(db, session_id)?;
    let workspace_id = db
        .find_session(session_id)
        .map_err(|e: anyhow::Error| e.to_string())?
        .map(|s| s.workspace_id);
    if let Some(wid) = workspace_id
        && let Some(dir) = db
            .get_workspace_openspec_dir(&wid)
            .map_err(|e: anyhow::Error| e.to_string())?
    {
        // Resolve case-insensitively so a path typed with the wrong case still
        // resolves on Linux's case-sensitive fs (same fix as Obsidian paths).
        let expanded = crate::obsidian::config::expand_home(&dir);
        return Ok(PathBuf::from(
            crate::obsidian::config::resolve_case_insensitive(&expanded),
        ));
    }
    Ok(cwd.join("openspec"))
}

fn scan_change_dir(dir: &std::path::Path, status: &str) -> Option<OpenSpecChange> {
    if !dir.is_dir() {
        return None;
    }
    let name = dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();

    if name.starts_with('.') {
        return None;
    }

    let created = if dir.join(".openspec.yaml").exists() {
        {
            std::fs::read_to_string(dir.join(".openspec.yaml"))
                .ok()
                .and_then(|s| {
                    s.lines()
                        .find(|l| l.starts_with("created:"))
                        .map(|l| l.trim_start_matches("created:").trim().to_string())
                })
                .unwrap_or_default()
        }
    } else {
        Default::default()
    };

    // Scan all .md files in the change directory as artifacts
    let mut artifacts = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries {
            let Ok(entry) = entry else { continue };
            let path = entry.path();
            if path.is_file()
                && let Some(ext) = path.extension()
                && ext == "md"
                && let Some(name) = path.file_stem().and_then(|s| s.to_str())
            {
                artifacts.push(name.to_string());
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
    if specs_dir.is_dir()
        && let Ok(entries) = std::fs::read_dir(&specs_dir)
    {
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
    let openspec_dir = resolve_openspec_dir(&db, &session_id)?;
    let changes_dir = openspec_dir.join("changes");

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
    if archive_dir.is_dir()
        && let Ok(entries) = std::fs::read_dir(&archive_dir)
    {
        for entry in entries {
            let Ok(entry) = entry else { continue };
            if let Some(change) = scan_change_dir(&entry.path(), "archived") {
                changes.push(change);
            }
        }
    }

    // Also scan master specs as a virtual "specs" entry
    let master_dir = openspec_dir.join("specs");
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
    let openspec_dir = resolve_openspec_dir(&db, &session_id)?;

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
    let openspec_dir = resolve_openspec_dir(&db, &session_id)?;
    let change_dir = openspec_dir.join("changes").join(&change_name);

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

/// The OpenSpec dir override for a workspace, plus the computed default
/// (`<repo>/openspec`) so the settings field can prefill it. `configured` is
/// None when the workspace uses the default.
#[derive(serde::Serialize)]
pub struct OpenSpecDirInfo {
    pub configured: Option<String>,
    pub default_dir: String,
}

#[tauri::command]
pub fn get_workspace_openspec_dir(
    db: State<'_, SharedDb>,
    workspace_id: String,
) -> Result<OpenSpecDirInfo, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let repo_path = db
        .workspace_repo_path(&workspace_id)
        .map_err(|e| e.to_string())?
        .ok_or("workspace not found")?;
    let configured = db
        .get_workspace_openspec_dir(&workspace_id)
        .map_err(|e| e.to_string())?;
    Ok(OpenSpecDirInfo {
        configured,
        default_dir: repo_path.join("openspec").display().to_string(),
    })
}

/// Set (empty/whitespace → clear to default) the workspace OpenSpec override.
#[tauri::command]
pub fn set_workspace_openspec_dir(
    db: State<'_, SharedDb>,
    workspace_id: String,
    openspec_dir: Option<String>,
) -> Result<(), String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    db.set_workspace_openspec_dir(&workspace_id, openspec_dir.as_deref())
        .map_err(|e| e.to_string())
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

/// Re-target the OpenSpec file watcher at a session's resolved openspec dir so
/// `openspec:changed` fires for external edits too. The frontend calls this
/// when the active session changes and right after the override is saved.
#[tauri::command]
pub fn watch_openspec_for_session(
    db: State<'_, SharedDb>,
    watcher: State<'_, crate::openspec::SharedOpenSpecWatcher>,
    session_id: String,
) -> Result<(), String> {
    let dir = {
        let db = db.lock().map_err(|e| e.to_string())?;
        resolve_openspec_dir(&db, &session_id)?
    };
    watcher
        .lock()
        .map_err(|e| e.to_string())?
        .retarget(&dir)
        .map_err(|e| e.to_string())
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
        let changes_dir = resolve_openspec_dir(&db, &session_id)?.join("changes");
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
    // Spawn cwd matches the session — relative file paths from list_directory
    // would otherwise resolve against cluihud's launch dir and the editor
    // would report "failed to load".
    command.current_dir(&cwd);
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
pub fn git_rename_branch(
    db: State<'_, SharedDb>,
    session_id: String,
    new_name: String,
) -> Result<(), String> {
    let trimmed = new_name.trim();
    if trimmed.is_empty() {
        return Err("branch name is empty".into());
    }
    let db = db.lock().map_err(|e| e.to_string())?;
    let Some(session) = db.find_session(&session_id).map_err(|e| e.to_string())? else {
        return Err("session not found".into());
    };
    let cwd = resolve_session_cwd(&db, &session_id)?;
    crate::worktree::rename_current_branch(&cwd, trimmed).map_err(|e| e.to_string())?;
    // get_session_git_info, ship and cleanup all read worktree_branch from
    // the DB — without this update the UI reverts to the old name and
    // cleanup later deletes the wrong branch.
    if session.worktree_branch.is_some() {
        db.update_worktree_branch(&session_id, trimmed)
            .map_err(|e| e.to_string())?;
    }
    Ok(())
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
pub fn git_stash_list(
    db: State<'_, SharedDb>,
    session_id: String,
) -> Result<Vec<crate::worktree::StashEntry>, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let cwd = resolve_session_cwd(&db, &session_id)?;
    crate::worktree::stash_list(&cwd).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn git_stash_create(
    db: State<'_, SharedDb>,
    session_id: String,
    message: String,
) -> Result<(), String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let cwd = resolve_session_cwd(&db, &session_id)?;
    crate::worktree::stash_create(&cwd, &message).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn git_stash_apply(
    db: State<'_, SharedDb>,
    session_id: String,
    index: u32,
) -> Result<crate::worktree::StashApplyOutcome, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let cwd = resolve_session_cwd(&db, &session_id)?;
    crate::worktree::stash_apply(&cwd, index).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn git_stash_pop(
    db: State<'_, SharedDb>,
    session_id: String,
    index: u32,
) -> Result<crate::worktree::StashApplyOutcome, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let cwd = resolve_session_cwd(&db, &session_id)?;
    crate::worktree::stash_pop(&cwd, index).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn git_stash_drop(
    db: State<'_, SharedDb>,
    session_id: String,
    index: u32,
) -> Result<(), String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let cwd = resolve_session_cwd(&db, &session_id)?;
    crate::worktree::stash_drop(&cwd, index).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn git_stash_show(
    db: State<'_, SharedDb>,
    session_id: String,
    index: u32,
) -> Result<Vec<String>, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let cwd = resolve_session_cwd(&db, &session_id)?;
    crate::worktree::stash_show(&cwd, index).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn git_stash_branch(
    db: State<'_, SharedDb>,
    session_id: String,
    index: u32,
    branch_name: String,
) -> Result<(), String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let cwd = resolve_session_cwd(&db, &session_id)?;
    crate::worktree::stash_branch(&cwd, index, &branch_name).map_err(|e| e.to_string())
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

    let cwd = resolve_session_cwd(&db, &session_id)?;
    // Sessions without a worktree (e.g., working directly on main or a
    // user-created feature branch) still benefit from PR detection — fall
    // back to whatever branch the cwd is currently on.
    let branch_owned;
    let branch: &str = match session.worktree_branch.as_deref() {
        Some(b) => b,
        None => {
            branch_owned = crate::worktree::current_branch(&cwd).map_err(|e| e.to_string())?;
            &branch_owned
        }
    };
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
#[allow(clippy::too_many_arguments)] // Tauri command surface — collapsing to a struct breaks the JS call shape.
pub fn git_ship(
    app: tauri::AppHandle,
    db: State<'_, SharedDb>,
    session_id: String,
    message: Option<String>,
    pr_title: String,
    pr_body: String,
    auto_merge: Option<bool>,
    target_branch: Option<String>,
) -> Result<crate::worktree::ShipResult, String> {
    use tauri::Emitter;
    let db = db.lock().map_err(|e| e.to_string())?;
    let cwd = resolve_session_cwd(&db, &session_id)?;
    let branch = resolve_session_branch(&db, &session_id)?;
    // Frontend override (PR target picker on Step 2) wins over the
    // session's resolved base when supplied; otherwise fall back to
    // the workspace default.
    let base = match target_branch
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some(override_base) => override_base.to_string(),
        None => resolve_session_base(&db, &session_id)?,
    };
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

const SEARCH_SKIP_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    "dist",
    "build",
    ".next",
    ".turbo",
    ".cache",
    "vendor",
    "__pycache__",
    ".venv",
    "venv",
    ".idea",
    ".vscode",
];
const SEARCH_MAX_RESULTS: usize = 500;

#[tauri::command]
pub fn search_files(
    session_id: String,
    query: String,
    db: State<'_, SharedDb>,
) -> Result<Vec<DirEntry>, String> {
    let query_lc = query.trim().to_lowercase();
    if query_lc.is_empty() {
        return Ok(Vec::new());
    }
    let db = db.lock().map_err(|e| e.to_string())?;
    let cwd = resolve_session_cwd(&db, &session_id)?;
    let mut hits = Vec::new();
    let mut stack = vec![cwd.clone()];
    while let Some(dir) = stack.pop() {
        let read = match std::fs::read_dir(&dir) {
            Ok(r) => r,
            Err(_) => continue,
        };
        for entry in read.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') && name != ".env" {
                continue;
            }
            let is_dir = entry.metadata().map(|m| m.is_dir()).unwrap_or(false);
            let entry_path = entry.path();
            if is_dir {
                if SEARCH_SKIP_DIRS.contains(&name.as_str()) {
                    continue;
                }
                stack.push(entry_path);
                continue;
            }
            if name.to_lowercase().contains(&query_lc) {
                let rel = entry_path
                    .strip_prefix(&cwd)
                    .unwrap_or(&entry_path)
                    .to_string_lossy()
                    .into_owned();
                hits.push(DirEntry {
                    name,
                    is_dir: false,
                    path: rel,
                });
                if hits.len() >= SEARCH_MAX_RESULTS {
                    return Ok(hits);
                }
            }
        }
    }
    hits.sort_by(|a, b| a.path.to_lowercase().cmp(&b.path.to_lowercase()));
    Ok(hits)
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

/// Push cluihud's active palette to every adapter that advertises
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

// obsidian-bridge change.

#[tauri::command]
pub fn get_obsidian_config(
    db: State<'_, SharedDb>,
    workspace_id: String,
) -> Result<crate::obsidian::config::ResolvedObsidianConfig, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    crate::obsidian::config::resolve(&workspace_id, |wid| db.get_obsidian_config(wid))
        .map_err(|e| e.to_string())
}

#[derive(serde::Serialize, Clone)]
struct ObsidianConfigChangedEvent {
    workspace_id: String,
    config: crate::obsidian::config::ResolvedObsidianConfig,
}

#[tauri::command]
pub fn save_obsidian_config(
    app: AppHandle,
    db: State<'_, SharedDb>,
    workspace_id: String,
    cfg: crate::obsidian::config::ObsidianConfig,
) -> Result<crate::obsidian::config::ResolvedObsidianConfig, String> {
    let mut cfg = cfg;
    crate::obsidian::config::normalize_file_channels(&mut cfg);
    let db = db.lock().map_err(|e| e.to_string())?;
    db.upsert_obsidian_config(&workspace_id, &cfg)
        .map_err(|e| e.to_string())?;
    let resolved =
        crate::obsidian::config::resolve(&workspace_id, |wid| db.get_obsidian_config(wid))
            .map_err(|e| e.to_string())?;
    let _ = app.emit(
        "obsidian:config-changed",
        ObsidianConfigChangedEvent {
            workspace_id: workspace_id.clone(),
            config: resolved.clone(),
        },
    );
    Ok(resolved)
}

#[tauri::command]
pub fn obsidian_enabled(db: State<'_, SharedDb>, workspace_id: String) -> Result<bool, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let resolved =
        crate::obsidian::config::resolve(&workspace_id, |wid| db.get_obsidian_config(wid))
            .map_err(|e| e.to_string())?;
    Ok(resolved.vault_root.is_some())
}

// xdg-open bypasses tauri-plugin-shell's hardcoded URL regex, which rejects
// custom schemes by default. We validate the prefix in our own code so the
// command never spawns xdg-open with anything outside our scheme allowlist.
#[tauri::command]
pub fn obsidian_open_uri(uri: String) -> Result<(), String> {
    if !uri.starts_with("obsidian://") && !uri.starts_with("cluihud://") {
        return Err(format!("refusing to open unknown scheme: {uri}"));
    }
    std::process::Command::new("xdg-open")
        .arg(&uri)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("xdg-open: {e}"))?;
    Ok(())
}

#[tauri::command]
pub fn obsidian_build_uri(
    db: State<'_, SharedDb>,
    workspace_id: String,
    path: String,
    heading: Option<String>,
    block: Option<String>,
) -> Result<String, String> {
    let resolved = {
        let db = db.lock().map_err(|e| e.to_string())?;
        crate::obsidian::config::resolve(&workspace_id, |wid| db.get_obsidian_config(wid))
            .map_err(|e| e.to_string())?
    };
    if resolved.vault_root.is_none() {
        return Err("Obsidian integration not configured".into());
    }
    let abs = std::path::PathBuf::from(&path);
    crate::obsidian::paths::to_obsidian_uri(&resolved, &abs, heading.as_deref(), block.as_deref())
        .ok_or_else(|| "Path is outside the configured vault".to_string())
}

#[derive(serde::Serialize)]
pub struct ProjectNoteResult {
    pub path: String,
    pub created: bool,
}

#[derive(serde::Serialize)]
pub struct PreBootstrap {
    pub vault_root: String,
    pub expected_path: String,
    pub inherited: bool,
}

// Single-shot backend probe so the Sidebar doesn't have to juggle Jotai atom
// timing (the active workspace's config may not be loaded yet when the user
// clicks Add Workspace immediately after launch). Returns None if no vault
// root can be sourced from anywhere — modal stays hidden.
#[tauri::command]
pub fn obsidian_pre_bootstrap(
    db: State<'_, SharedDb>,
    workspace_id: String,
) -> Result<Option<PreBootstrap>, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let own = crate::obsidian::config::resolve(&workspace_id, |wid| db.get_obsidian_config(wid))
        .map_err(|e| e.to_string())?;
    let workspaces = db.get_workspaces().map_err(|e| e.to_string())?;
    let workspace_name = workspaces
        .iter()
        .find(|w| w.id == workspace_id)
        .map(|w| w.name.clone())
        .ok_or_else(|| "workspace not found".to_string())?;

    if let Some(root) = own.vault_root.clone() {
        return Ok(Some(PreBootstrap {
            expected_path: project_index_for(&root, &workspace_name),
            vault_root: root,
            inherited: false,
        }));
    }

    for w in &workspaces {
        if w.id == workspace_id {
            continue;
        }
        let resolved = crate::obsidian::config::resolve(&w.id, |wid| db.get_obsidian_config(wid))
            .map_err(|e| e.to_string())?;
        if let Some(root) = resolved.vault_root.clone() {
            return Ok(Some(PreBootstrap {
                expected_path: project_index_for(&root, &workspace_name),
                vault_root: root,
                inherited: true,
            }));
        }
    }

    Ok(None)
}

// Same scan as obsidian_pre_bootstrap but returns the full donor config so
// the create step can persist all transferable fields (vault_name + quick
// capture + templates + toggles), not just vault_root.
fn find_donor_cfg(
    db: &crate::db::Database,
    skip_workspace_id: &str,
) -> Result<Option<crate::obsidian::config::ObsidianConfig>, String> {
    let workspaces = db.get_workspaces().map_err(|e| e.to_string())?;
    for w in workspaces {
        if w.id == skip_workspace_id {
            continue;
        }
        let resolved = crate::obsidian::config::resolve(&w.id, |wid| db.get_obsidian_config(wid))
            .map_err(|e| e.to_string())?;
        if resolved.vault_root.is_some() {
            return Ok(Some(resolved));
        }
    }
    Ok(None)
}

fn project_index_for(vault_root: &str, workspace_name: &str) -> String {
    let slug = workspace_name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' || c == '.' {
                c
            } else {
                '-'
            }
        })
        .collect::<String>();
    let slug = slug.trim_matches('-');
    // Strip trailing slashes from the stored vault_root so the join doesn't
    // produce `/vault//Projects/...` for rows that predate normalize_file_channels.
    let root = vault_root.trim_end_matches('/');
    format!("{root}/Projects/{slug}/index.md")
}

#[tauri::command]
pub fn obsidian_create_project_note(
    app: AppHandle,
    db: State<'_, SharedDb>,
    workspace_id: String,
    target_path: String,
    suggested_layout: bool,
) -> Result<ProjectNoteResult, String> {
    let (workspace_name, workspace_path) = {
        let db = db.lock().map_err(|e| e.to_string())?;
        let workspaces = db.get_workspaces().map_err(|e| e.to_string())?;
        let ws = workspaces
            .into_iter()
            .find(|w| w.id == workspace_id)
            .ok_or_else(|| "workspace not found".to_string())?;
        (ws.name, ws.repo_path)
    };

    // Persist inheritance now (the pre-bootstrap probe is read-only). When the
    // new workspace has no own row yet, copy transferable fields from any
    // sibling workspace that has Obsidian configured.
    let mut config_dirty = false;
    {
        let db = db.lock().map_err(|e| e.to_string())?;
        let own_row = db
            .get_obsidian_config(&workspace_id)
            .map_err(|e| e.to_string())?;
        if own_row.is_none()
            && let Some(donor) = find_donor_cfg(&db, &workspace_id)?
        {
            let mut inherited = crate::obsidian::config::ObsidianConfig {
                vault_root: donor.vault_root,
                vault_name: donor.vault_name,
                session_log_path: None,
                quick_capture_path: donor.quick_capture_path,
                moc_path: None,
                templates_path: donor.templates_path,
                backlinks_enabled: donor.backlinks_enabled,
                render_wikilinks: donor.render_wikilinks,
                search_subdir: donor.search_subdir,
            };
            crate::obsidian::config::normalize_file_channels(&mut inherited);
            db.upsert_obsidian_config(&workspace_id, &inherited)
                .map_err(|e| e.to_string())?;
            config_dirty = true;
        }
    }

    let resolved = {
        let db = db.lock().map_err(|e| e.to_string())?;
        crate::obsidian::config::resolve(&workspace_id, |wid| db.get_obsidian_config(wid))
            .map_err(|e| e.to_string())?
    };

    let expanded = crate::obsidian::config::expand_home(&target_path);
    let target = std::path::Path::new(&expanded);
    let outcome = crate::obsidian::bootstrap::create_project_note_at(
        target,
        &workspace_name,
        &workspace_path,
    )
    .map_err(|e| e.to_string())?;
    if suggested_layout {
        let (log_path, moc_path) =
            crate::obsidian::bootstrap::suggested_layout_paths(&resolved, &workspace_name)
                .map_err(|e| e.to_string())?;
        let mut next = resolved.clone();
        next.session_log_path = Some(log_path);
        next.moc_path = Some(moc_path);
        let db = db.lock().map_err(|e| e.to_string())?;
        db.upsert_obsidian_config(&workspace_id, &next)
            .map_err(|e| e.to_string())?;
        config_dirty = true;
    }
    if config_dirty {
        let final_resolved = {
            let db = db.lock().map_err(|e| e.to_string())?;
            crate::obsidian::config::resolve(&workspace_id, |wid| db.get_obsidian_config(wid))
                .map_err(|e| e.to_string())?
        };
        let _ = app.emit(
            "obsidian:config-changed",
            ObsidianConfigChangedEvent {
                workspace_id: workspace_id.clone(),
                config: final_resolved.clone(),
            },
        );
    }
    Ok(ProjectNoteResult {
        path: outcome.path.display().to_string(),
        created: outcome.created,
    })
}

#[tauri::command]
pub fn obsidian_watch_templates(
    app: AppHandle,
    db: State<'_, SharedDb>,
    state: State<'_, crate::obsidian::templates_watcher::TemplatesWatcherState>,
    workspace_id: String,
) -> Result<Vec<crate::obsidian::templates::Template>, String> {
    let resolved = {
        let db = db.lock().map_err(|e| e.to_string())?;
        crate::obsidian::config::resolve(&workspace_id, |wid| db.get_obsidian_config(wid))
            .map_err(|e| e.to_string())?
    };
    let dir = resolved
        .templates_path
        .as_deref()
        .filter(|s| !s.is_empty())
        .map(std::path::PathBuf::from);
    state.rewatch(dir.clone(), app).map_err(|e| e.to_string())?;
    crate::obsidian::templates::list_templates(&resolved).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn obsidian_quick_capture(
    app: AppHandle,
    db: State<'_, SharedDb>,
    workspace_id: String,
    text: String,
) -> Result<String, String> {
    let (resolved, repo_path) = {
        let db = db.lock().map_err(|e| e.to_string())?;
        let resolved =
            crate::obsidian::config::resolve(&workspace_id, |wid| db.get_obsidian_config(wid))
                .map_err(|e| e.to_string())?;
        let repo_path = db
            .workspace_repo_path(&workspace_id)
            .ok()
            .flatten()
            .map(|p| p.to_string_lossy().into_owned());
        (resolved, repo_path)
    };
    let written = crate::obsidian::channels::QuickCaptureWriter::append(
        &resolved,
        &text,
        None,
        repo_path.as_deref(),
    )
    .map_err(|e| e.to_string())?
    .ok_or_else(|| "quick_capture_path not configured for this workspace".to_string())?;
    let path_str = written.display().to_string();
    let _ = app.emit("obsidian:capture-saved", &path_str);
    Ok(path_str)
}

#[tauri::command]
pub fn drain_pending_deeplinks(
    state: tauri::State<'_, crate::PendingDeepLinks>,
) -> Result<Vec<String>, String> {
    let mut buf = state.0.lock().map_err(|e| e.to_string())?;
    Ok(std::mem::take(&mut *buf))
}

/// Global search engine (obsidian-bridge M2). `active_workspace_id` resolves
/// the Vault/OpenSpec scopes; WorkspaceFiles carries its own id in the query.
#[tauri::command]
pub async fn search(
    db: State<'_, SharedDb>,
    query: crate::search::SearchQuery,
    active_workspace_id: Option<String>,
    // Narrows only the Vault scope, never the other scopes.
    vault_subdir: Option<String>,
) -> Result<Vec<crate::search::SearchHit>, String> {
    let ctx = {
        let db = db.lock().map_err(|e| e.to_string())?;

        let mut workspace_paths = std::collections::HashMap::new();
        let mut openspec_dir = None;
        for ws in db.get_workspaces().map_err(|e| e.to_string())? {
            if Some(&ws.id) == active_workspace_id.as_ref() {
                // Honor the per-workspace override (specs living outside the
                // repo); fall back to <repo>/openspec.
                let candidate = db
                    .get_workspace_openspec_dir(&ws.id)
                    .ok()
                    .flatten()
                    .map(|d| {
                        let expanded = crate::obsidian::config::expand_home(&d);
                        std::path::PathBuf::from(crate::obsidian::config::resolve_case_insensitive(
                            &expanded,
                        ))
                    })
                    .unwrap_or_else(|| ws.repo_path.join("openspec"));
                if candidate.is_dir() {
                    openspec_dir = Some(candidate);
                }
            }
            workspace_paths.insert(ws.id, ws.repo_path);
        }

        let vault_root = active_workspace_id
            .as_deref()
            .and_then(|wid| {
                crate::obsidian::config::resolve(wid, |w| db.get_obsidian_config(w)).ok()
            })
            .and_then(|cfg| cfg.vault_root)
            .filter(|v| !v.is_empty())
            .map(|v| std::path::PathBuf::from(crate::obsidian::config::expand_home(&v)));

        // Reject `..` components so the toggle can't climb out of the vault.
        let vault_root = match vault_subdir.as_deref().map(str::trim) {
            Some(sub) if !sub.is_empty() && !sub.split('/').any(|c| c == "..") => {
                vault_root.map(|r| r.join(sub))
            }
            _ => vault_root,
        };

        crate::search::SearchContext {
            vault_root,
            transcripts_dir: Some(crate::config::Config::load().transcripts_directory),
            openspec_dir,
            workspace_paths,
        }
    };

    tauri::async_runtime::spawn_blocking(move || {
        crate::search::SearchEngine::search(&query, &ctx).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

// ── Pinned vault notes (obsidian-context-injection #3/#H) ──

/// Read the full pin union from the DB and rewatch it (N2 hot reload). Logged,
/// never fatal — a watcher hiccup must not fail the pin/unpin itself.
fn rebuild_pinned_watcher(
    db: &SharedDb,
    watcher: &crate::obsidian::pinned_notes_watcher::PinnedNotesWatcherState,
    app: &AppHandle,
) {
    let pins = db.lock().ok().and_then(|g| g.all_pinned_notes().ok());
    if let Some(pins) = pins
        && let Err(e) = watcher.rebuild(&pins, app.clone())
    {
        tracing::warn!("pinned-notes watcher rebuild failed: {e}");
    }
}

/// Resolve the vault_root configured for the workspace owning `session_id`.
pub(crate) fn vault_root_for_session(
    db: &crate::db::Database,
    session_id: &str,
) -> Option<PathBuf> {
    let session = db.find_session(session_id).ok().flatten()?;
    let cfg =
        crate::obsidian::config::resolve(&session.workspace_id, |w| db.get_obsidian_config(w))
            .ok()?;
    cfg.vault_root
        .filter(|v| !v.trim().is_empty())
        .map(PathBuf::from)
}

/// Pin a vault note to a session; its body seeds the agent context on the next
/// spawn/resume. Returns the updated pin list, rewatches the pin union for hot
/// reload, and emits `vault:pins-changed`. Rejects paths outside the workspace
/// vault — a pinned path is read at spawn, so the guard belongs at write time.
#[tauri::command]
pub fn pin_vault_note(
    app: AppHandle,
    db: State<'_, SharedDb>,
    watcher: State<'_, crate::obsidian::pinned_notes_watcher::PinnedNotesWatcherState>,
    session_id: String,
    path: String,
) -> Result<Vec<String>, String> {
    let paths = {
        let db = db.lock().map_err(|e| e.to_string())?;
        if let Some(root) = vault_root_for_session(&db, &session_id)
            && !crate::obsidian::pinned_notes::is_within_vault(&root, &path)
        {
            return Err("note is outside the configured vault".to_string());
        }
        db.add_pinned_note(&session_id, &path)
            .map_err(|e| e.to_string())?;
        db.get_pinned_notes(&session_id)
            .map_err(|e| e.to_string())?
    };
    rebuild_pinned_watcher(&db, &watcher, &app);
    Ok(paths)
}

#[tauri::command]
pub fn unpin_vault_note(
    app: AppHandle,
    db: State<'_, SharedDb>,
    watcher: State<'_, crate::obsidian::pinned_notes_watcher::PinnedNotesWatcherState>,
    session_id: String,
    path: String,
) -> Result<Vec<String>, String> {
    let paths = {
        let db = db.lock().map_err(|e| e.to_string())?;
        db.remove_pinned_note(&session_id, &path)
            .map_err(|e| e.to_string())?;
        db.get_pinned_notes(&session_id)
            .map_err(|e| e.to_string())?
    };
    rebuild_pinned_watcher(&db, &watcher, &app);
    Ok(paths)
}

#[tauri::command]
pub fn list_pinned_notes(
    db: State<'_, SharedDb>,
    session_id: String,
) -> Result<Vec<String>, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    db.get_pinned_notes(&session_id).map_err(|e| e.to_string())
}

/// Wire string of the active adapter's context-injection tier for a session
/// (`append_system_prompt_file` | `prompt_preamble` | `unsupported`), so the
/// pin chip can phrase an honest tooltip per agent.
#[tauri::command]
pub fn get_context_injection_tier(
    agents: State<'_, AgentRuntimeState>,
    db: State<'_, SharedDb>,
    session_id: String,
) -> Result<String, String> {
    let agent_id = agents
        .resolve(&session_id)
        .or_else(|| {
            db.lock()
                .ok()
                .and_then(|g| g.find_session(&session_id).ok().flatten())
                .and_then(|s| AgentId::new(&s.agent_id).ok())
        })
        .unwrap_or_else(AgentId::claude_code);
    let tier = agents
        .registry
        .get(&agent_id)
        .map(|a| a.context_injection())
        .unwrap_or(crate::agents::ContextInjection::Unsupported);
    Ok(tier.as_wire().to_string())
}

// ── Vault note reading (#P Obsidian panel) ──

fn resolve_vault_root(db: &SharedDb, workspace_id: &str) -> Result<PathBuf, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let cfg = crate::obsidian::config::resolve(workspace_id, |w| db.get_obsidian_config(w))
        .map_err(|e| e.to_string())?;
    cfg.vault_root
        .filter(|s| !s.trim().is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| "no vault configured".to_string())
}

/// Read a note body, rejecting any path that escapes `vault_root` after
/// canonicalization (path-traversal guard). Pure core for `read_vault_note`.
fn read_note_guarded(vault_root: &std::path::Path, path: &str) -> Result<String, String> {
    let canon_root = std::fs::canonicalize(vault_root).map_err(|e| e.to_string())?;
    let canon_path = std::fs::canonicalize(path).map_err(|e| e.to_string())?;
    if !canon_path.starts_with(&canon_root) {
        return Err("note is outside the configured vault".to_string());
    }
    std::fs::read_to_string(&canon_path).map_err(|e| e.to_string())
}

/// Resolve a wikilink target to the first matching `.md` under `vault_root`:
/// a vault-relative path match (when the name is path-qualified) else a
/// case-insensitive filename-stem match. Pure core for `resolve_vault_note`.
fn resolve_note_in_vault(vault_root: &std::path::Path, name: &str) -> Option<String> {
    let cleaned = name.trim().trim_start_matches('/');
    let cleaned = cleaned.strip_suffix(".md").unwrap_or(cleaned);
    let want_path = cleaned.to_lowercase();
    let want_stem = std::path::Path::new(cleaned)
        .file_stem()
        .map(|s| s.to_string_lossy().to_lowercase())
        .unwrap_or_else(|| want_path.clone());
    // An exact vault-relative path match wins over a bare filename-stem match,
    // and must beat walk order — otherwise a stem hit in an earlier folder
    // shadows the path-qualified note the wikilink actually names.
    let mut stem_fallback: Option<String> = None;
    for entry in walkdir::WalkDir::new(vault_root)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if !entry.file_type().is_file() {
            continue;
        }
        let p = entry.path();
        if p.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        let rel = p.strip_prefix(vault_root).unwrap_or(p);
        let rel_noext = rel.with_extension("").to_string_lossy().to_lowercase();
        if rel_noext == want_path {
            return Some(p.display().to_string());
        }
        if stem_fallback.is_none()
            && p.file_stem()
                .map(|s| s.to_string_lossy().to_lowercase())
                .as_deref()
                == Some(want_stem.as_str())
        {
            stem_fallback = Some(p.display().to_string());
        }
    }
    stem_fallback
}

/// Read a vault note's body for the #P panel. Guarded under the workspace's
/// vault_root — do NOT use `read_file_content` (it is cwd/workspace-relative).
#[tauri::command]
pub fn read_vault_note(
    db: State<'_, SharedDb>,
    workspace_id: String,
    path: String,
) -> Result<String, String> {
    let vault_root = resolve_vault_root(&db, &workspace_id)?;
    read_note_guarded(&vault_root, &path)
}

/// Resolve a wikilink `[[name]]` to an absolute path under vault_root (vault-
/// wide, like Obsidian), or `None` when unresolved (caller falls back to
/// opening Obsidian to create it).
#[tauri::command]
pub fn resolve_vault_note(
    db: State<'_, SharedDb>,
    workspace_id: String,
    name: String,
) -> Result<Option<String>, String> {
    let vault_root = resolve_vault_root(&db, &workspace_id)?;
    Ok(resolve_note_in_vault(&vault_root, &name))
}

#[cfg(test)]
mod vault_read_tests {
    use super::{read_note_guarded, resolve_note_in_vault};
    use std::io::Write;
    use std::path::Path;

    fn write_note(dir: &Path, rel: &str, body: &str) {
        let path = dir.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(body.as_bytes()).unwrap();
    }

    #[test]
    fn read_inside_vault_ok() {
        let dir = tempfile::tempdir().unwrap();
        write_note(dir.path(), "Note.md", "body text");
        let p = dir.path().join("Note.md").display().to_string();
        assert_eq!(read_note_guarded(dir.path(), &p).unwrap(), "body text");
    }

    #[test]
    fn read_outside_vault_rejected() {
        let vault = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        write_note(outside.path(), "Secret.md", "nope");
        let p = outside.path().join("Secret.md").display().to_string();
        assert!(read_note_guarded(vault.path(), &p).is_err());
    }

    #[test]
    fn resolve_hit_by_stem_case_insensitive() {
        let dir = tempfile::tempdir().unwrap();
        write_note(dir.path(), "Projects/Alpha.md", "a");
        assert!(resolve_note_in_vault(dir.path(), "alpha").is_some());
        assert!(resolve_note_in_vault(dir.path(), "ALPHA").is_some());
    }

    #[test]
    fn resolve_path_qualified_beats_stem_in_other_folder() {
        let dir = tempfile::tempdir().unwrap();
        write_note(dir.path(), "A/Target.md", "a");
        write_note(dir.path(), "B/Target.md", "b");
        let hit = resolve_note_in_vault(dir.path(), "B/Target").unwrap();
        assert!(hit.ends_with("B/Target.md"), "got {hit}");
    }

    #[test]
    fn resolve_hit_by_relative_path() {
        let dir = tempfile::tempdir().unwrap();
        write_note(dir.path(), "Projects/Beta.md", "b");
        let hit = resolve_note_in_vault(dir.path(), "Projects/Beta").unwrap();
        assert!(hit.ends_with("Projects/Beta.md"));
    }

    #[test]
    fn resolve_miss_is_none() {
        let dir = tempfile::tempdir().unwrap();
        write_note(dir.path(), "Note.md", "x");
        assert!(resolve_note_in_vault(dir.path(), "Nonexistent").is_none());
    }
}
