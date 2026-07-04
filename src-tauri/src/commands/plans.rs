use std::path::PathBuf;

use tauri::State;

use crate::agents::AgentId;
use crate::agents::claude_code::plan::PlanManager;
use crate::agents::state::AgentRuntimeState;
use crate::agents::{PlanCapability, PlanCapabilityWire};
use crate::db::SharedDb;
use crate::hooks::state::HookState;
use crate::models::Session;
use crate::plan_state::SharedPlanState;

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

/// Persists in-place edits to disk if the plan was modified, mirroring
/// `save_plan`'s propagation so a failed write aborts the caller before a
/// decision is sent for the still-stale-on-disk content.
fn save_plan_edits_if_dirty(runtime: &mut PlanManager) -> Result<(), String> {
    if let Some(plan) = &runtime.current_plan
        && plan.content != plan.original
    {
        runtime
            .save_edits(plan.content.clone())
            .map_err(|e| e.to_string())?;
    }
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
        save_plan_edits_if_dirty(runtime)?;
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

    let payload = serde_json::to_string(&decision).map_err(|e| e.to_string())?;

    // Unix: write the decision into the FIFO the hook CLI is reading.
    #[cfg(unix)]
    std::fs::write(&decision_path, &payload)
        .map_err(|e| format!("writing decision to FIFO: {e}"))?;

    // Windows: connect to the CLI's gate pipe at submit time (single
    // invocation, no held state — Decision 6) and write the decision. The
    // sync client verifies the pipe owner is us + retries ERROR_PIPE_BUSY /
    // ERROR_FILE_NOT_FOUND. Dropping the stream closes the pipe → the gate's
    // read sees EOF.
    #[cfg(windows)]
    {
        use std::io::Write;
        let mut stream = crate::platform::sync_connect(std::path::Path::new(&decision_path))
            .map_err(|e| format!("connecting to plan-review gate pipe: {e}"))?;
        stream
            .write_all(payload.as_bytes())
            .map_err(|e| format!("writing decision to gate pipe: {e}"))?;
        stream.flush().map_err(|e| e.to_string())?;
    }

    #[cfg(not(any(unix, windows)))]
    std::fs::write(&decision_path, &payload).map_err(|e| e.to_string())?;

    Ok(())
}

#[cfg(test)]
mod plan_edit_save_tests {
    use super::{PlanManager, save_plan_edits_if_dirty};

    fn dirty_plan(plans_dir: std::path::PathBuf, plan_path: std::path::PathBuf) -> PlanManager {
        let mut mgr = PlanManager::new(plans_dir);
        mgr.set_plan(plan_path, "original".to_string());
        if let Some(plan) = mgr.current_plan.as_mut() {
            plan.content = "edited".to_string();
        }
        mgr
    }

    #[test]
    fn unedited_plan_is_not_written() {
        let dir = tempfile::tempdir().unwrap();
        let plan_path = dir.path().join("plan.md");
        let mut mgr = PlanManager::new(dir.path().to_path_buf());
        mgr.set_plan(plan_path.clone(), "same".to_string());

        assert!(save_plan_edits_if_dirty(&mut mgr).is_ok());
        assert!(!plan_path.exists(), "no write when content == original");
    }

    #[test]
    fn edited_plan_saves_successfully() {
        let dir = tempfile::tempdir().unwrap();
        let plan_path = dir.path().join("plan.md");
        let mut mgr = dirty_plan(dir.path().to_path_buf(), plan_path.clone());

        assert!(save_plan_edits_if_dirty(&mut mgr).is_ok());
        assert_eq!(std::fs::read_to_string(&plan_path).unwrap(), "edited");
    }

    /// Proves the `submit_plan_decision` contract at its actual failure seam:
    /// `?` on this call means a save failure returns before the decision
    /// payload is ever built, so a stale-on-disk plan can never be reported
    /// as approved. A full command-level test would need a live Tauri
    /// `AppHandle`/`State`, which this crate has no mocking setup for
    /// (`tauri::test` isn't enabled in Cargo.toml, and no other command test
    /// in the codebase constructs one) — this is the closest real seam.
    #[test]
    fn edited_plan_save_failure_is_propagated_not_swallowed() {
        // Nonexistent parent dir makes the write fail (NotFound), standing in
        // for a disk-full or permissions failure without needing root.
        let dir = tempfile::tempdir().unwrap();
        let missing_parent = dir.path().join("does-not-exist");
        let plan_path = missing_parent.join("plan.md");
        let mut mgr = dirty_plan(dir.path().to_path_buf(), plan_path);

        let result = save_plan_edits_if_dirty(&mut mgr);
        assert!(
            result.is_err(),
            "save failure must surface as Err, not be discarded"
        );
    }
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
    plans.sort_by_key(|p| std::cmp::Reverse(p.modified));
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
