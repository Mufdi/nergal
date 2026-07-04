use tauri::State;

use super::shared::{resolve_session_base, resolve_session_branch, resolve_session_cwd};
use crate::db::SharedDb;
use crate::platform_spawn::NoWindow;

/// Summary of a single PR rendered in the GitPanel PRs sidebar list.
#[derive(Clone, serde::Serialize)]
pub struct PrSummary {
    pub number: u32,
    pub title: String,
    pub state: String,
    pub url: String,
    pub base_ref_name: String,
    pub head_ref_name: String,
    pub updated_at: String,
}

/// List the workspace's pull requests via `gh pr list`. Ordered with OPEN
/// first (by `updatedAt` desc), then MERGED/CLOSED. Capped at 20.
#[tauri::command]
pub fn list_prs(db: State<'_, SharedDb>, workspace_id: String) -> Result<Vec<PrSummary>, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let repo_path = db
        .workspace_repo_path(&workspace_id)
        .map_err(|e| e.to_string())?
        .ok_or("workspace not found")?;

    let output = std::process::Command::new("gh")
        .no_window()
        .args([
            "pr",
            "list",
            "--state",
            "all",
            "--limit",
            "20",
            "--json",
            "number,title,state,url,baseRefName,headRefName,updatedAt",
        ])
        .current_dir(&repo_path)
        .output()
        .map_err(|e| format!("failed to invoke gh: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("gh pr list failed: {stderr}"));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let raw: Vec<serde_json::Value> = serde_json::from_str(&stdout).unwrap_or_default();

    let mut prs: Vec<PrSummary> = raw
        .into_iter()
        .filter_map(|v| {
            Some(PrSummary {
                number: v.get("number")?.as_u64()? as u32,
                title: v.get("title")?.as_str()?.to_owned(),
                state: v.get("state")?.as_str()?.to_owned(),
                url: v.get("url")?.as_str()?.to_owned(),
                base_ref_name: v.get("baseRefName")?.as_str()?.to_owned(),
                head_ref_name: v.get("headRefName")?.as_str()?.to_owned(),
                updated_at: v.get("updatedAt")?.as_str()?.to_owned(),
            })
        })
        .collect();

    // OPEN first, then everything else; within each bucket, newest update first.
    prs.sort_by(|a, b| {
        let a_open = a.state == "OPEN";
        let b_open = b.state == "OPEN";
        b_open
            .cmp(&a_open)
            .then_with(|| b.updated_at.cmp(&a.updated_at))
    });

    Ok(prs)
}

/// Fetch the PR's unified diff via `gh pr diff <num>`. Returned text is
/// parsed by the frontend into chunks for rendering and chunk-by-chunk
/// navigation. Errors from `gh` are surfaced verbatim so the inline error
/// path in the PR Viewer can show them.
#[tauri::command]
pub fn get_pr_diff(
    db: State<'_, SharedDb>,
    workspace_id: String,
    pr_number: u32,
) -> Result<String, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let repo_path = db
        .workspace_repo_path(&workspace_id)
        .map_err(|e| e.to_string())?
        .ok_or("workspace not found")?;

    let output = std::process::Command::new("gh")
        .no_window()
        .args(["pr", "diff", &pr_number.to_string()])
        .current_dir(&repo_path)
        .output()
        .map_err(|e| format!("failed to invoke gh: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("gh pr diff failed: {stderr}"));
    }

    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// CI checks for an arbitrary PR (workspace-scoped, not session-scoped). Used
/// by the PR Viewer header where the active session may not be the one that
/// owns the PR. Returns `None` when `gh pr checks` produces no parsable
/// output rather than surfacing the error: a missing CI block in the header
/// is benign and the user can retry by reopening the tab.
#[tauri::command]
pub fn get_pr_checks(
    db: State<'_, SharedDb>,
    workspace_id: String,
    pr_number: u32,
) -> Result<Option<crate::worktree::PrChecks>, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let repo_path = db
        .workspace_repo_path(&workspace_id)
        .map_err(|e| e.to_string())?
        .ok_or("workspace not found")?;
    Ok(crate::worktree::pr_checks(&repo_path, pr_number).ok())
}

/// Merge a PR via `gh pr merge <number>`. `strategy` is one of `squash`
/// (default, matches the project's PR convention), `merge`, or `rebase`.
/// Workspace-scoped (not session-scoped) so the PR Viewer can drive merges
/// for any PR the user clicks into, regardless of which session is active.
/// On `mergeable=false` (typically a conflict), the returned error string
/// contains `mergeable=false` so the frontend can switch to opening the
/// conflicts tab instead of just toasting the failure.
#[tauri::command]
pub fn gh_pr_merge(
    db: State<'_, SharedDb>,
    workspace_id: String,
    pr_number: u32,
    strategy: Option<String>,
) -> Result<(), String> {
    let strategy = strategy.unwrap_or_else(|| "squash".to_string());
    if !matches!(strategy.as_str(), "squash" | "merge" | "rebase") {
        return Err(format!("unknown merge strategy: {strategy}"));
    }

    let db = db.lock().map_err(|e| e.to_string())?;
    let cwd = db
        .workspace_repo_path(&workspace_id)
        .map_err(|e| e.to_string())?
        .ok_or("workspace not found")?;
    let strategy_flag = format!("--{strategy}");
    let pr_arg = pr_number.to_string();

    let output = std::process::Command::new("gh")
        .no_window()
        .args(["pr", "merge", &pr_arg, &strategy_flag])
        .current_dir(&cwd)
        .output()
        .map_err(|e| format!("failed to invoke gh: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // Marker the frontend can grep for to route to the conflicts tab
        // instead of a generic error toast.
        if stderr.contains("not mergeable") || stderr.contains("merge conflict") {
            return Err(format!("mergeable=false: {stderr}"));
        }
        return Err(format!("gh pr merge failed: {stderr}"));
    }

    Ok(())
}

#[tauri::command]
pub fn get_pr_status(
    db: State<'_, SharedDb>,
    session_id: String,
) -> Result<Option<crate::worktree::PrInfo>, String> {
    let db = db.lock().map_err(|e| e.to_string())?;

    let Some(session) = db
        .find_session(&session_id)
        .map_err(|e: anyhow::Error| e.to_string())?
    else {
        return Err("session not found".into());
    };

    let cwd = resolve_session_cwd(&db, &session_id)?;
    // Sessions without a worktree (e.g., working directly on main or a
    // user-created feature branch) still benefit from PR detection — fall
    // back to whatever branch the cwd is currently on.
    let branch_owned;
    let branch: &str = match session.worktree_branch.as_deref() {
        Some(b) => b,
        None => {
            branch_owned = crate::worktree::current_branch(&cwd).map_err(|e| e.to_string())?;
            &branch_owned
        }
    };
    crate::worktree::pr_status(&cwd, branch).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn create_pr(
    db: State<'_, SharedDb>,
    session_id: String,
    title: String,
    body: String,
) -> Result<crate::worktree::PrInfo, String> {
    // Extract everything under a short guard, then drop it before the
    // network-bound list_branches / create_pr calls below.
    let (cwd, branch, repo_path) = {
        let db = db.lock().map_err(|e| e.to_string())?;

        let Some(session) = db
            .find_session(&session_id)
            .map_err(|e: anyhow::Error| e.to_string())?
        else {
            return Err("session not found".into());
        };

        let Some(branch) = session.worktree_branch.clone() else {
            return Err("session has no worktree branch".into());
        };

        let cwd = resolve_session_cwd(&db, &session_id)?;

        let repo_path = db
            .workspace_repo_path(&session.workspace_id)
            .map_err(|e: anyhow::Error| e.to_string())?
            .ok_or("workspace not found")?;

        (cwd, branch, repo_path)
    };

    let branches = crate::worktree::list_branches(&repo_path).map_err(|e| e.to_string())?;
    let base = if branches.iter().any(|b| b == "main") {
        "main"
    } else {
        "master"
    };

    crate::worktree::create_pr(&cwd, &branch, base, &title, &body).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn complete_pending_merge(
    db: State<'_, SharedDb>,
    session_id: String,
) -> Result<String, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let cwd = resolve_session_cwd(&db, &session_id)?;
    crate::worktree::complete_pending_merge(&cwd).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn has_pending_merge(db: State<'_, SharedDb>, session_id: String) -> Result<bool, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let cwd = resolve_session_cwd(&db, &session_id)?;
    Ok(crate::worktree::has_pending_merge(&cwd))
}

#[tauri::command]
pub fn enable_pr_auto_merge(
    db: State<'_, SharedDb>,
    session_id: String,
    pr_number: u32,
) -> Result<(), String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let cwd = resolve_session_cwd(&db, &session_id)?;
    crate::worktree::enable_pr_auto_merge(&cwd, pr_number).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn gh_available() -> bool {
    crate::worktree::gh_available()
}

#[tauri::command]
pub fn get_pr_preview_data(
    db: State<'_, SharedDb>,
    session_id: String,
) -> Result<crate::worktree::PrPreviewData, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let cwd = resolve_session_cwd(&db, &session_id)?;
    let base = resolve_session_base(&db, &session_id)?;
    crate::worktree::pr_preview_data(&cwd, &base, "HEAD").map_err(|e| e.to_string())
}

#[tauri::command]
pub fn poll_pr_checks(
    db: State<'_, SharedDb>,
    session_id: String,
) -> Result<Option<crate::worktree::PrChecks>, String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let cwd = resolve_session_cwd(&db, &session_id)?;
    let branch = resolve_session_branch(&db, &session_id)?;
    let Some(pr) = crate::worktree::pr_status(&cwd, &branch).map_err(|e| e.to_string())? else {
        return Ok(None);
    };
    if pr.state != "OPEN" {
        return Ok(None);
    }
    crate::worktree::pr_checks(&cwd, pr.number)
        .map(Some)
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[allow(clippy::too_many_arguments)] // Tauri command surface — collapsing to a struct breaks the JS call shape.
pub fn git_ship(
    app: tauri::AppHandle,
    db: State<'_, SharedDb>,
    session_id: String,
    message: Option<String>,
    pr_title: String,
    pr_body: String,
    auto_merge: Option<bool>,
    target_branch: Option<String>,
) -> Result<crate::worktree::ShipResult, String> {
    use tauri::Emitter;

    // Extract everything under a short guard, then drop it before the
    // git/network work below (branch/base resolution can shell out to git,
    // and worktree::ship pushes + calls gh).
    let (cwd, worktree_branch, repo_path) = {
        let db = db.lock().map_err(|e| e.to_string())?;
        let cwd = resolve_session_cwd(&db, &session_id)?;
        let Some(session) = db
            .find_session(&session_id)
            .map_err(|e: anyhow::Error| e.to_string())?
        else {
            return Err("session not found".into());
        };
        let repo_path = db
            .workspace_repo_path(&session.workspace_id)
            .map_err(|e: anyhow::Error| e.to_string())?
            .ok_or("workspace not found")?;
        (cwd, session.worktree_branch, repo_path)
    };

    let branch = match worktree_branch {
        Some(b) => b,
        None => crate::worktree::current_branch(&cwd).map_err(|e| e.to_string())?,
    };

    // Frontend override (PR target picker on Step 2) wins over the
    // session's resolved base when supplied; otherwise fall back to
    // the workspace default.
    let base = match target_branch
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some(override_base) => override_base.to_string(),
        None => {
            let branches = crate::worktree::list_branches(&repo_path).map_err(|e| e.to_string())?;
            if branches.iter().any(|b| b == "main") {
                "main".to_string()
            } else {
                "master".to_string()
            }
        }
    };
    let sid = session_id.clone();
    let app_clone = app.clone();
    let on_stage = move |stage: crate::worktree::ShipStage, ok: bool| {
        let _ = app_clone.emit(
            "ship:progress",
            serde_json::json!({
                "session_id": sid,
                "stage": stage.as_str(),
                "ok": ok,
            }),
        );
    };
    let result = crate::worktree::ship(
        &cwd,
        &branch,
        &base,
        message.as_deref(),
        &pr_title,
        &pr_body,
        on_stage,
    )
    .map_err(|e| e.to_string())?;

    if auto_merge.unwrap_or(false) && result.pr_info.number > 0 {
        let _ = app.emit(
            "ship:progress",
            serde_json::json!({ "session_id": session_id, "stage": "auto-merge", "ok": true }),
        );
        if let Err(e) = crate::worktree::enable_pr_auto_merge(&cwd, result.pr_info.number) {
            let _ = app.emit(
                "ship:progress",
                serde_json::json!({ "session_id": session_id, "stage": "auto-merge", "ok": false, "error": e.to_string() }),
            );
            return Err(format!("PR created but auto-merge failed: {e}"));
        }
    }

    Ok(result)
}
