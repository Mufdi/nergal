use std::path::PathBuf;

use tauri::{AppHandle, Emitter, State};
use tauri_plugin_opener::OpenerExt;

use crate::db::SharedDb;

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

/// Reject any URI that doesn't match our allowlist BEFORE calling the opener —
/// the Rust `app.opener().open_url()` call bypasses the plugin's ACL scope
/// check, so this prefix gate is the sole security boundary (Decision 6/7).
pub fn is_allowed_scheme(uri: &str) -> bool {
    uri.starts_with("obsidian://") || uri.starts_with("nergal://")
}

#[tauri::command]
pub fn obsidian_open_uri(app: tauri::AppHandle, uri: String) -> Result<(), String> {
    if !is_allowed_scheme(&uri) {
        return Err(format!("refusing to open unknown scheme: {uri}"));
    }
    app.opener()
        .open_url(&uri, None::<&str>)
        .map_err(|e| e.to_string())
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
            // Inherit ONLY the shared vault (root + name) — the new workspace
            // lives in the same vault. Channels (quick capture / templates /
            // search subdir / logs / MOC) are per-workspace and start blank, so
            // a sibling's specific channels don't bleed in.
            let mut inherited = crate::obsidian::config::ObsidianConfig {
                vault_root: donor.vault_root,
                vault_name: donor.vault_name,
                session_log_path: None,
                quick_capture_path: None,
                moc_path: None,
                templates_path: None,
                backlinks_enabled: donor.backlinks_enabled,
                render_wikilinks: donor.render_wikilinks,
                search_subdir: None,
                // Standing context is workspace-specific — don't inherit a
                // sibling's default pins.
                default_pinned_note_paths: Vec::new(),
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
        // Wikilink path-qualifiers are '/'-separated (Obsidian convention); match
        // the OS separator to it so a Windows '\' rel-path still compares equal.
        let rel_noext = rel
            .with_extension("")
            .to_string_lossy()
            .to_lowercase()
            .replace('\\', "/");
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
        // Resolved hit is an internal fs path — native `\` on Windows; normalize.
        assert!(hit.replace('\\', "/").ends_with("B/Target.md"), "got {hit}");
    }

    #[test]
    fn resolve_hit_by_relative_path() {
        let dir = tempfile::tempdir().unwrap();
        write_note(dir.path(), "Projects/Beta.md", "b");
        let hit = resolve_note_in_vault(dir.path(), "Projects/Beta").unwrap();
        assert!(
            hit.replace('\\', "/").ends_with("Projects/Beta.md"),
            "got {hit}"
        );
    }

    #[test]
    fn resolve_miss_is_none() {
        let dir = tempfile::tempdir().unwrap();
        write_note(dir.path(), "Note.md", "x");
        assert!(resolve_note_in_vault(dir.path(), "Nonexistent").is_none());
    }
}
