use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use tauri::State;

use crate::claude::cost::{self, CostSummary};
use crate::claude::plan::SharedPlanManager;
use crate::config::Config;
use crate::hooks::state::HookState;
use crate::tasks::{Task, TaskStore};

/// Shared task store per session, managed as Tauri state.
pub type SharedTaskStore = Arc<Mutex<TaskStore>>;

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
pub fn get_tasks(state: State<'_, SharedTaskStore>) -> Result<Vec<Task>, String> {
    let store = state.lock().map_err(|e| e.to_string())?;
    Ok(store.visible_tasks().cloned().collect())
}

// -- Plan commands --

#[tauri::command]
pub fn get_plan(state: State<'_, SharedPlanManager>) -> Result<Option<PlanResponse>, String> {
    let manager = state.lock().map_err(|e| e.to_string())?;
    let Some(plan) = &manager.current_plan else {
        return Ok(None);
    };
    Ok(Some(PlanResponse {
        path: plan.path.clone(),
        content: plan.content.clone(),
        has_edits: plan.has_edits(),
    }))
}

#[derive(Clone, serde::Serialize)]
pub struct PlanResponse {
    pub path: PathBuf,
    pub content: String,
    pub has_edits: bool,
}

#[tauri::command]
pub fn save_plan(state: State<'_, SharedPlanManager>, content: String) -> Result<String, String> {
    let mut manager = state.lock().map_err(|e| e.to_string())?;
    let path = manager.save_edits(content).map_err(|e| e.to_string())?;
    Ok(path.display().to_string())
}

#[tauri::command]
pub fn diff_plan(state: State<'_, SharedPlanManager>) -> Result<Option<String>, String> {
    let manager = state.lock().map_err(|e| e.to_string())?;
    let Some(plan) = &manager.current_plan else {
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
pub fn approve_plan(state: State<'_, SharedPlanManager>) -> Result<(), String> {
    let mut manager = state.lock().map_err(|e| e.to_string())?;
    if let Some(plan) = &mut manager.current_plan {
        plan.original = plan.content.clone();
    }
    Ok(())
}

#[tauri::command]
pub fn reject_plan(state: State<'_, SharedPlanManager>) -> Result<(), String> {
    let manager = state.lock().map_err(|e| e.to_string())?;
    let Some(plan) = &manager.current_plan else {
        return Ok(());
    };

    let plan_path = plan.path.clone();

    // Write pending edit so UserPromptSubmit hook injects re-read instruction
    let mut hook_state = HookState::read().map_err(|e| e.to_string())?;
    hook_state.pending_plan_edit = Some(plan_path);
    hook_state.write().map_err(|e| e.to_string())?;

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
