use tauri::State;

use crate::db::SharedDb;
use crate::hooks::state::HookState;

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
