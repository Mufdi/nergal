use tauri::State;

use super::shared::{resolve_session_branch, resolve_session_cwd};
use crate::db::SharedDb;
use crate::platform_spawn::NoWindow;

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
pub fn list_branches(db: State<'_, SharedDb>, workspace_id: String) -> Result<Vec<String>, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let repo_path = db
        .workspace_repo_path(&workspace_id)
        .map_err(|e| e.to_string())?
        .ok_or("workspace not found")?;
    crate::worktree::list_branches(&repo_path).map_err(|e| e.to_string())
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
        // Read the live HEAD, not the cached DB value: when the agent creates or
        // switches a branch inside the worktree, `worktree_branch` goes stale and
        // the status bar / git panel keep showing the old branch (BUG-04). Fall
        // back to the cached name only if the live read fails.
        let branch = crate::worktree::current_branch(wt_path).unwrap_or_else(|_| {
            session
                .worktree_branch
                .clone()
                .unwrap_or_else(|| "unknown".into())
        });
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
pub fn git_push(db: State<'_, SharedDb>, session_id: String) -> Result<bool, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let cwd = resolve_session_cwd(&db, &session_id)?;
    let branch = resolve_session_branch(&db, &session_id)?;
    crate::worktree::push(&cwd, &branch).map_err(|e| e.to_string())
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
        .no_window()
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
