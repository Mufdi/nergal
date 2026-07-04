use std::path::PathBuf;

use tauri::State;

use crate::agents::claude_code::cost::{self, CostSummary};
use crate::db::SharedDb;
use crate::tasks::Task;

// -- Task commands --

#[tauri::command]
pub fn get_tasks(session_id: String, db: State<'_, SharedDb>) -> Result<Vec<Task>, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    db.get_visible_tasks(&session_id).map_err(|e| e.to_string())
}

/// Tombstone all completed tasks of a session (status -> 'deleted'). The clear
/// must persist, otherwise the next `get_tasks` hydration re-adds them (BUG-12).
/// 'deleted' rows are excluded from `get_visible_tasks`, so the TodoWrite hook
/// (which seeds its store from visible tasks) never resurrects them.
#[tauri::command]
pub fn clear_completed_tasks(session_id: String, db: State<'_, SharedDb>) -> Result<(), String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    db.mark_completed_tasks_deleted(&session_id)
        .map_err(|e| e.to_string())
}

/// Tombstone a single task (status -> 'deleted'); same persistence rationale as
/// `clear_completed_tasks`.
#[tauri::command]
pub fn delete_task(
    session_id: String,
    task_id: String,
    db: State<'_, SharedDb>,
) -> Result<(), String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    db.mark_task_deleted(&session_id, &task_id)
        .map_err(|e| e.to_string())
}

/// Force-tombstone every task of a session (delete-all, incl. ghosts). Same
/// persistence rationale as `clear_completed_tasks`.
#[tauri::command]
pub fn delete_all_tasks(session_id: String, db: State<'_, SharedDb>) -> Result<(), String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    db.mark_all_tasks_deleted(&session_id)
        .map_err(|e| e.to_string())
}

// -- Cost command --

#[tauri::command]
pub fn get_cost(transcript_path: String) -> Result<CostSummary, String> {
    let path = PathBuf::from(transcript_path);
    Ok(cost::parse_cost_from_transcript(&path))
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
