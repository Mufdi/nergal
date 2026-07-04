use std::path::PathBuf;

use tauri::State;

use super::shared::{resolve_openspec_dir, resolve_session_cwd};
use crate::db::SharedDb;

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
        // `which` crate, not `Command::new("which")`: cross-platform (no
        // dependency on a `which`/`where.exe` binary in PATH) and a single
        // lookup instead of two shell-outs.
        match which::which(&path) {
            Ok(resolved) => {
                return PathValidation {
                    exists: true,
                    is_dir: false,
                    is_file: true,
                    is_executable: true,
                    resolved_path: Some(resolved.to_string_lossy().into_owned()),
                    error: None,
                };
            }
            Err(_) => {
                return PathValidation {
                    exists: false,
                    is_dir: false,
                    is_file: false,
                    is_executable: false,
                    resolved_path: None,
                    error: Some(format!("'{path}' not found in PATH")),
                };
            }
        }
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
            let is_executable = {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    meta.permissions().mode() & 0o111 != 0
                }
                #[cfg(not(unix))]
                {
                    is_file
                }
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

/// Find the first available command candidate via the `which` crate.
fn find_available_command(candidates: &[&str]) -> Option<String> {
    candidates
        .iter()
        .find(|cmd| which::which(cmd).is_ok())
        .map(|cmd| cmd.to_string())
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
    // would otherwise resolve against nergal's launch dir and the editor
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
    let target = crate::fs_guard::resolve_within_base(&cwd, &path)?;

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
    hits.sort_by_key(|a| a.path.to_lowercase());
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
    let file_path = crate::fs_guard::resolve_within_base(&cwd, &path)?;
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
    let file_path = crate::fs_guard::resolve_within_base(&cwd, &path)?;
    std::fs::write(&file_path, &content).map_err(|e| e.to_string())?;
    Ok(file_path.to_string_lossy().to_string())
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
                match vault_root {
                    Some(root) => {
                        // The saved subdir keeps whatever case the user typed;
                        // on a case-sensitive FS a mismatch yields a non-existent
                        // dir and the engine silently returns nothing. Case-correct
                        // the join, and fall back to the whole vault if the scoped
                        // dir still doesn't exist (mistyped subdir → search all,
                        // not empty).
                        let joined = root.join(sub);
                        let resolved = std::path::PathBuf::from(
                            crate::obsidian::config::resolve_case_insensitive(
                                &joined.to_string_lossy(),
                            ),
                        );
                        if resolved.is_dir() {
                            Some(resolved)
                        } else {
                            Some(root)
                        }
                    }
                    None => None,
                }
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
