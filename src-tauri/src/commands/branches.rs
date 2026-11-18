use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex};

use serde::{Deserialize, Serialize};
use tauri::State;

use super::git::DiffResponse;
use super::shared::{blocking, resolve_session_cwd};
use super::workspaces_sessions::{
    build_new_session, capabilities_for_agent_id, deleted_branches_record,
    extend_plan_watcher_for_session, kept_message,
};
use crate::agents::AgentId;
use crate::agents::state::AgentRuntimeState;
use crate::db::SharedDb;
use crate::models::{BranchOwner, Session};
use crate::worktree::local_branches::{
    BranchUsage, add_worktree_for_branch, branch_usage, clear_missing_worktrees, commit_file_diff,
    commit_files, nergal_branch_candidates, nergal_slug, pending_commit_count, pending_commits,
};
use crate::worktree::{
    BranchOutcome, BranchState, CommitEntry, DropPolicy, branch_state, drop_branch, is_git_repo,
    remove_worktree,
};

const BRANCH_COMMITS_LIMIT: u32 = 100;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BranchSession {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LocalBranchRow {
    pub name: String,
    pub sha: String,
    pub pending: Option<u32>,
    pub last_commit_unix: i64,
    pub last_subject: String,
    pub session: Option<BranchSession>,
    pub in_use: bool,
    pub missing_worktree: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LocalBranchDelete {
    Deleted { sha: String, repo: String },
    Missing,
    Kept { message: String },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum CommitScope {
    #[serde(rename_all = "camelCase")]
    Session { session_id: String },
    #[serde(rename_all = "camelCase")]
    Workspace { workspace_id: String },
}

type RepoLocks = Mutex<HashMap<PathBuf, Arc<Mutex<()>>>>;

static REPO_LOCKS: LazyLock<RepoLocks> = LazyLock::new(|| Mutex::new(HashMap::new()));

// Lock order: read the repo path under the DB guard, drop it, then take this
// lock. The DB guard may be taken while holding it, never the other way round.
pub(crate) fn branch_repo_lock(repo: &Path) -> Arc<Mutex<()>> {
    let key = repo.canonicalize().unwrap_or_else(|_| repo.to_path_buf());
    let mut locks = REPO_LOCKS.lock().unwrap_or_else(|e| e.into_inner());
    locks.entry(key).or_default().clone()
}

fn repo_path_of(db: &SharedDb, workspace_id: &str) -> Result<PathBuf, String> {
    let guard = db.lock().map_err(|e| e.to_string())?;
    guard
        .workspace_repo_path(workspace_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "workspace not found".to_string())
}

fn owners_of(db: &SharedDb, workspace_id: &str) -> Result<Vec<BranchOwner>, String> {
    let guard = db.lock().map_err(|e| e.to_string())?;
    guard.branch_owners(workspace_id).map_err(|e| e.to_string())
}

pub(crate) fn local_branch_rows(
    repo: &Path,
    owners: &[BranchOwner],
) -> Result<Vec<LocalBranchRow>, String> {
    if !is_git_repo(repo) {
        return Ok(Vec::new());
    }
    let candidates = nergal_branch_candidates(repo).map_err(|e| e.to_string())?;
    let mut rows = Vec::new();
    for candidate in candidates {
        let owner = owners.iter().find(|o| o.branch == candidate.name);
        let target = owner.and_then(|o| o.merge_target.as_deref());
        let pending = pending_commit_count(repo, &candidate.sha, target);
        if pending == Some(0) {
            continue;
        }
        let (in_use, missing_worktree) = match branch_usage(repo, &candidate.name) {
            BranchUsage::Free => (false, None),
            BranchUsage::InUse => (true, None),
            BranchUsage::MissingWorktree { paths } => (
                false,
                paths.first().map(|p| p.to_string_lossy().into_owned()),
            ),
        };
        rows.push(LocalBranchRow {
            name: candidate.name,
            sha: candidate.sha,
            pending,
            last_commit_unix: candidate.last_commit_unix,
            last_subject: candidate.last_subject,
            session: owner.map(|o| BranchSession {
                id: o.session_id.clone(),
                name: o.session_name.clone(),
            }),
            in_use,
            missing_worktree,
        });
    }
    Ok(rows)
}

pub(crate) fn delete_local_branch_core(
    db: &SharedDb,
    workspace_id: &str,
    branch: &str,
    expected_sha: &str,
    record: &Path,
) -> Result<LocalBranchDelete, String> {
    if nergal_slug(branch).is_none() {
        return Ok(LocalBranchDelete::Kept {
            message: format!("Branch {branch} kept: not a Nergal branch name"),
        });
    }
    let repo = repo_path_of(db, workspace_id)?;
    let lock = branch_repo_lock(&repo);
    let _held = lock.lock().unwrap_or_else(|e| e.into_inner());

    if let Some(owner) = owners_of(db, workspace_id)?
        .into_iter()
        .find(|o| o.branch == branch)
    {
        return Ok(LocalBranchDelete::Kept {
            message: format!(
                "Branch {branch} kept: session {} uses it",
                owner.session_name
            ),
        });
    }
    // Checked before clearing stale registrations so a refused delete leaves
    // the repository exactly as it was.
    match crate::worktree::branch_state(&repo, branch) {
        crate::worktree::BranchState::Missing => return Ok(LocalBranchDelete::Missing),
        crate::worktree::BranchState::Unknown => {
            return Ok(LocalBranchDelete::Kept {
                message: kept_message(branch, &crate::worktree::KeptReason::Unchecked),
            });
        }
        crate::worktree::BranchState::Exists { sha } if sha != expected_sha => {
            return Ok(LocalBranchDelete::Kept {
                message: kept_message(branch, &crate::worktree::KeptReason::Moved),
            });
        }
        crate::worktree::BranchState::Exists { .. } => {}
    }
    clear_missing_worktrees(&repo, branch).map_err(|e| e.to_string())?;

    Ok(
        match drop_branch(
            &repo,
            branch,
            Some(expected_sha),
            DropPolicy::Unconditional,
            record,
        ) {
            BranchOutcome::Deleted { sha } => LocalBranchDelete::Deleted {
                sha,
                repo: repo.display().to_string(),
            },
            BranchOutcome::Missing => LocalBranchDelete::Missing,
            BranchOutcome::Kept { reason } => LocalBranchDelete::Kept {
                message: kept_message(branch, &reason),
            },
        },
    )
}

pub(crate) fn finish_branch_session(
    db: &SharedDb,
    repo: &Path,
    worktree: &Path,
    session: Session,
) -> Result<Session, String> {
    let failure = {
        let guard = db.lock().map_err(|e| e.to_string());
        match guard {
            Err(e) => Some(e),
            Ok(guard) => {
                let rival = session.worktree_branch.as_deref().and_then(|branch| {
                    guard
                        .branch_owners(&session.workspace_id)
                        .ok()?
                        .into_iter()
                        .find(|o| o.branch == branch)
                });
                match rival {
                    Some(owner) => Some(format!(
                        "Branch {} is already used by session {}",
                        owner.branch, owner.session_name
                    )),
                    None => guard.create_session(&session).err().map(|e| e.to_string()),
                }
            }
        }
    };
    match failure {
        None => Ok(session),
        Some(error) => {
            if let Err(e) = remove_worktree(repo, worktree) {
                tracing::warn!(error = %e, worktree = %worktree.display(), "could not remove the worktree after a failed session open");
            }
            Err(error)
        }
    }
}

pub(crate) fn open_branch_session_core(
    db: &SharedDb,
    workspace_id: &str,
    branch: &str,
    ts: u64,
    agent_capabilities: Vec<String>,
) -> Result<(Session, PathBuf), String> {
    let slug =
        nergal_slug(branch).ok_or_else(|| format!("{branch} is not a Nergal branch name"))?;
    let repo = repo_path_of(db, workspace_id)?;
    let lock = branch_repo_lock(&repo);
    let _held = lock.lock().unwrap_or_else(|e| e.into_inner());

    let default_pinned_notes = {
        let guard = db.lock().map_err(|e| e.to_string())?;
        if let Some(owner) = guard
            .branch_owners(workspace_id)
            .map_err(|e| e.to_string())?
            .into_iter()
            .find(|o| o.branch == branch)
        {
            return Err(format!(
                "Branch {branch} is already used by session {}",
                owner.session_name
            ));
        }
        guard
            .get_obsidian_config(workspace_id)
            .ok()
            .flatten()
            .map(|c| c.default_pinned_note_paths)
            .unwrap_or_default()
    };

    if !matches!(branch_state(&repo, branch), BranchState::Exists { .. }) {
        return Err(format!("Branch {branch} does not exist"));
    }
    clear_missing_worktrees(&repo, branch).map_err(|e| e.to_string())?;
    if branch_usage(&repo, branch) == BranchUsage::InUse {
        return Err(format!(
            "Branch {branch} is checked out, or being rebased or bisected, in a worktree"
        ));
    }

    let worktree = add_worktree_for_branch(&repo, slug).map_err(|e| e.to_string())?;
    let session_id = format!("{}-{ts}", workspace_id.chars().take(6).collect::<String>());
    let session = build_new_session(
        session_id,
        slug.to_string(),
        workspace_id.to_string(),
        Some(worktree.clone()),
        Some(branch.to_string()),
        ts,
        &AgentId::claude_code(),
        agent_capabilities,
        None,
        None,
        default_pinned_notes,
    );
    let session = finish_branch_session(db, &repo, &worktree, session)?;
    Ok((session, repo))
}

fn scope_base(db: &SharedDb, scope: &CommitScope) -> Result<PathBuf, String> {
    let guard = db.lock().map_err(|e| e.to_string())?;
    match scope {
        CommitScope::Session { session_id } => resolve_session_cwd(&guard, session_id),
        CommitScope::Workspace { workspace_id } => guard
            .workspace_repo_path(workspace_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "workspace not found".to_string()),
    }
}

#[tauri::command]
pub async fn list_local_branches(
    db: State<'_, SharedDb>,
    workspace_id: String,
) -> Result<Vec<LocalBranchRow>, String> {
    let db = db.inner().clone();
    blocking(move || {
        let repo = repo_path_of(&db, &workspace_id)?;
        let owners = owners_of(&db, &workspace_id)?;
        local_branch_rows(&repo, &owners)
    })
    .await
}

#[tauri::command]
pub async fn get_branch_commits(
    db: State<'_, SharedDb>,
    workspace_id: String,
    branch: String,
) -> Result<Vec<CommitEntry>, String> {
    let db = db.inner().clone();
    blocking(move || {
        if nergal_slug(&branch).is_none() {
            return Err(format!("{branch} is not a Nergal branch name"));
        }
        let repo = repo_path_of(&db, &workspace_id)?;
        let target = owners_of(&db, &workspace_id)?
            .into_iter()
            .find(|o| o.branch == branch)
            .and_then(|o| o.merge_target);
        let BranchState::Exists { sha } = branch_state(&repo, &branch) else {
            return Err(format!("Branch {branch} does not exist"));
        };
        pending_commits(&repo, &sha, target.as_deref(), BRANCH_COMMITS_LIMIT)
            .map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
pub async fn get_scoped_commit_files(
    db: State<'_, SharedDb>,
    scope: CommitScope,
    hash: String,
) -> Result<Vec<String>, String> {
    let db = db.inner().clone();
    blocking(move || {
        let base = scope_base(&db, &scope)?;
        commit_files(&base, &hash).map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
pub async fn get_commit_file_diff(
    db: State<'_, SharedDb>,
    scope: CommitScope,
    hash: String,
    file_path: String,
) -> Result<DiffResponse, String> {
    let db = db.inner().clone();
    blocking(move || {
        let base = scope_base(&db, &scope)?;
        let diff_text = commit_file_diff(&base, &hash, &file_path).map_err(|e| e.to_string())?;
        Ok(DiffResponse {
            file_path,
            diff_text,
            is_new: false,
        })
    })
    .await
}

#[tauri::command]
pub async fn open_branch_session(
    db: State<'_, SharedDb>,
    agents: State<'_, AgentRuntimeState>,
    plan_watcher: State<'_, crate::agents::claude_code::plan::SharedPlanWatcher>,
    workspace_id: String,
    branch: String,
) -> Result<Session, String> {
    let db = db.inner().clone();
    let agent_id = AgentId::claude_code();
    let capabilities = capabilities_for_agent_id(&agents, agent_id.as_str());
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let (session, repo) =
        blocking(move || open_branch_session_core(&db, &workspace_id, &branch, ts, capabilities))
            .await?;
    agents.register_session(&session.id, agent_id);
    extend_plan_watcher_for_session(&agents, &plan_watcher, &session, &repo);
    Ok(session)
}

#[tauri::command]
pub async fn delete_local_branch(
    db: State<'_, SharedDb>,
    workspace_id: String,
    branch: String,
    expected_sha: String,
) -> Result<LocalBranchDelete, String> {
    let db = db.inner().clone();
    blocking(move || {
        delete_local_branch_core(
            &db,
            &workspace_id,
            &branch,
            &expected_sha,
            &deleted_branches_record(),
        )
    })
    .await
}

#[cfg(test)]
#[path = "branches_tests.rs"]
mod branches_tests;

#[cfg(test)]
#[path = "branches_review_tests.rs"]
mod branches_review_tests;
