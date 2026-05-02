//! Tauri command handlers for the scratchpad feature.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use serde::Serialize;
use tauri::{AppHandle, State};

use super::watcher::ScratchpadWatcher;
use super::{
    OwnWriteHashes, ScratchTab, cleanup_orphan_tmps, create_note, default_scratchpad_dir,
    ensure_dir, list_notes, read_note, restore_from_trash, soft_delete, write_note,
};
use crate::config::Config;
use crate::db::SharedDb;

/// Tauri state owning the scratchpad runtime: current root path, the live
/// watcher (replaced on path change), and the per-file own-write hash ring
/// buffer shared with the watcher.
pub struct ScratchpadState {
    pub root: Mutex<PathBuf>,
    pub own_writes: Arc<OwnWriteHashes>,
    pub watcher: Mutex<Option<ScratchpadWatcher>>,
}

impl ScratchpadState {
    pub fn new(initial_root: PathBuf) -> Self {
        Self {
            root: Mutex::new(initial_root),
            own_writes: Arc::new(OwnWriteHashes::default()),
            watcher: Mutex::new(None),
        }
    }

    /// Resolve the current root path under the mutex.
    pub fn current_root(&self) -> PathBuf {
        self.root.lock().expect("scratchpad root mutex").clone()
    }

    /// Replace the active watcher with a new one bound to `new_root`.
    /// Pre-existing own-write hashes are dropped because the new path has
    /// a different file set.
    pub fn rewatch(&self, new_root: PathBuf, app: AppHandle) -> anyhow::Result<()> {
        {
            let mut guard = self.watcher.lock().expect("scratchpad watcher mutex");
            *guard = None;
        }
        self.own_writes.clear();
        let watcher = ScratchpadWatcher::spawn(new_root.clone(), self.own_writes.clone(), app)?;
        *self.watcher.lock().expect("scratchpad watcher mutex") = Some(watcher);
        *self.root.lock().expect("scratchpad root mutex") = new_root;
        Ok(())
    }
}

#[derive(Debug, Serialize)]
pub struct ScratchpadGeometry {
    pub geometry_json: String,
    pub opacity: f64,
}

#[tauri::command]
pub fn scratchpad_get_path(state: State<'_, ScratchpadState>) -> Result<String, String> {
    Ok(state.current_root().display().to_string())
}

#[tauri::command]
pub fn scratchpad_default_path() -> String {
    default_scratchpad_dir().display().to_string()
}

#[tauri::command]
pub fn scratchpad_set_path(
    new_path: String,
    state: State<'_, ScratchpadState>,
    app: AppHandle,
) -> Result<String, String> {
    let path = PathBuf::from(new_path);
    let canonical = ensure_dir(&path).map_err(|e| e.to_string())?;
    state
        .rewatch(canonical.clone(), app)
        .map_err(|e| e.to_string())?;
    let mut config = Config::load();
    config.scratchpad_path = Some(canonical.clone());
    if let Err(e) = config.save() {
        tracing::warn!("scratchpad_set_path: failed to persist to config: {e}");
    }
    Ok(canonical.display().to_string())
}

#[tauri::command]
pub fn scratchpad_list_tabs(state: State<'_, ScratchpadState>) -> Result<Vec<ScratchTab>, String> {
    let root = state.current_root();
    list_notes(&root).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn scratchpad_read_tab(
    tab_id: String,
    state: State<'_, ScratchpadState>,
) -> Result<String, String> {
    let root = state.current_root();
    read_note(&root, &tab_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn scratchpad_write_tab(
    tab_id: String,
    content: String,
    state: State<'_, ScratchpadState>,
) -> Result<(), String> {
    let root = state.current_root();
    let hash = write_note(&root, &tab_id, &content).map_err(|e| e.to_string())?;
    state.own_writes.record(&tab_id, hash);
    Ok(())
}

#[tauri::command]
pub fn scratchpad_create_tab(state: State<'_, ScratchpadState>) -> Result<String, String> {
    let root = state.current_root();
    create_note(&root).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn scratchpad_close_tab(
    tab_id: String,
    state: State<'_, ScratchpadState>,
) -> Result<(), String> {
    let root = state.current_root();
    soft_delete(&root, &tab_id).map_err(|e| e.to_string())?;
    state.own_writes.forget(&tab_id);
    Ok(())
}

#[tauri::command]
pub fn scratchpad_cleanup_tmps(state: State<'_, ScratchpadState>) -> Result<usize, String> {
    let root = state.current_root();
    cleanup_orphan_tmps(&root).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn scratchpad_restore_tab(
    tab_id: String,
    state: State<'_, ScratchpadState>,
) -> Result<bool, String> {
    let root = state.current_root();
    restore_from_trash(&root, &tab_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn scratchpad_get_geometry(
    panel_id: String,
    db: State<'_, SharedDb>,
) -> Result<Option<ScratchpadGeometry>, String> {
    let guard = db.lock().map_err(|_| "db poisoned".to_string())?;
    let row = guard
        .get_panel_geometry(&panel_id)
        .map_err(|e| e.to_string())?;
    Ok(row.map(|(geometry_json, opacity)| ScratchpadGeometry {
        geometry_json,
        opacity,
    }))
}

#[tauri::command]
pub fn scratchpad_set_geometry(
    panel_id: String,
    geometry_json: String,
    opacity: f64,
    db: State<'_, SharedDb>,
) -> Result<(), String> {
    let guard = db.lock().map_err(|_| "db poisoned".to_string())?;
    guard
        .set_panel_geometry(&panel_id, &geometry_json, opacity)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn scratchpad_reveal_in_file_manager(state: State<'_, ScratchpadState>) -> Result<(), String> {
    let root = state.current_root();
    // `Command::new` with `.arg()` — never `bash -c` — so the path can't
    // be interpreted as a shell expression even if it contains spaces or
    // metacharacters.
    std::process::Command::new("xdg-open")
        .arg(&root)
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}
