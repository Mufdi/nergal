use std::path::PathBuf;

// ── Ship flow: push, ship, PR preview, CI checks, conflicts ──

pub(crate) fn resolve_session_base(
    db: &crate::db::Database,
    session_id: &str,
) -> Result<String, String> {
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

pub(crate) fn resolve_session_branch(
    db: &crate::db::Database,
    session_id: &str,
) -> Result<String, String> {
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

/// Resolve session working directory.
pub(crate) fn resolve_session_cwd(
    db: &crate::db::Database,
    session_id: &str,
) -> Result<PathBuf, String> {
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
pub(crate) fn resolve_openspec_dir(
    db: &crate::db::Database,
    session_id: &str,
) -> Result<PathBuf, String> {
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
