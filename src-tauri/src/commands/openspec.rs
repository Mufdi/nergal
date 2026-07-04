use tauri::State;

use super::shared::resolve_openspec_dir;
use crate::db::SharedDb;

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
        crate::fs_guard::resolve_within_base(&openspec_dir, &artifact_path)?
    } else {
        let changes_dir = openspec_dir.join("changes");
        // Try active first, then archive
        let active_dir = crate::fs_guard::resolve_within_base(&changes_dir, &change_name)?;
        let change_dir = if active_dir.exists() {
            active_dir
        } else {
            let archive_dir = changes_dir.join("archive");
            crate::fs_guard::resolve_within_base(&archive_dir, &change_name)?
        };
        crate::fs_guard::resolve_within_base(&change_dir, &artifact_path)?
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
    let changes_dir = openspec_dir.join("changes");
    let change_dir = crate::fs_guard::resolve_within_base(&changes_dir, &change_name)?;

    if !change_dir.exists() {
        return Err("change not found or is archived".into());
    }

    let file_path = crate::fs_guard::resolve_within_base(&change_dir, &artifact_path)?;

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

/// The plans-dir override for a workspace, plus the computed default
/// (auto-resolved `plansDirectory` for the workspace's repo path) so the
/// settings field can show what Nergal resolved. `configured` is None when
/// the workspace uses the auto-detected default.
#[derive(serde::Serialize)]
pub struct PlansDirInfo {
    pub configured: Option<String>,
    pub default_dir: String,
}

#[tauri::command]
pub fn get_workspace_plans_dir(
    db: State<'_, SharedDb>,
    workspace_id: String,
) -> Result<PlansDirInfo, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let repo_path = db
        .workspace_repo_path(&workspace_id)
        .map_err(|e| e.to_string())?
        .ok_or("workspace not found")?;
    let configured = db
        .get_workspace_plans_dir(&workspace_id)
        .map_err(|e| e.to_string())?;
    Ok(PlansDirInfo {
        configured,
        default_dir: crate::agents::claude_code::resolve_cc_plans_directory(&repo_path)
            .display()
            .to_string(),
    })
}

/// Set (empty/whitespace → clear to default) the workspace plans-dir override.
#[tauri::command]
pub fn set_workspace_plans_dir(
    db: State<'_, SharedDb>,
    workspace_id: String,
    plans_dir: Option<String>,
) -> Result<(), String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    db.set_workspace_plans_dir(&workspace_id, plans_dir.as_deref())
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
