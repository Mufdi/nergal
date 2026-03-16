#![allow(dead_code)]
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};

/// Check whether a path is inside a git repository.
pub fn is_git_repo(path: &Path) -> bool {
    path.join(".git").exists()
}

/// Create a git worktree at `<repo>/.worktrees/cluihud/<slug>/` with branch `cluihud/<slug>`.
///
/// If the branch already exists, reuses it. If the worktree directory already exists, returns it.
/// Returns the absolute path to the created worktree directory.
pub fn create_worktree(repo_path: &Path, slug: &str) -> Result<PathBuf> {
    let worktree_path = repo_path.join(".worktrees").join("cluihud").join(slug);
    let branch_name = format!("cluihud/{slug}");

    // Already exists on disk — reuse
    if worktree_path.exists() {
        return Ok(worktree_path);
    }

    // Try creating with new branch first
    let output = Command::new("git")
        .args(["worktree", "add", "-b", &branch_name])
        .arg(&worktree_path)
        .current_dir(repo_path)
        .output()
        .context("failed to execute git worktree add")?;

    if output.status.success() {
        return Ok(worktree_path);
    }

    // Branch might already exist — try without -b
    let output2 = Command::new("git")
        .args(["worktree", "add"])
        .arg(&worktree_path)
        .arg(&branch_name)
        .current_dir(repo_path)
        .output()
        .context("failed to execute git worktree add (reuse branch)")?;

    if output2.status.success() {
        return Ok(worktree_path);
    }

    let stderr = String::from_utf8_lossy(&output2.stderr);
    anyhow::bail!("git worktree add failed: {stderr}");
}

/// Remove a git worktree forcefully.
pub fn remove_worktree(repo_path: &Path, worktree_path: &Path) -> Result<()> {
    let output = Command::new("git")
        .args(["worktree", "remove", "--force"])
        .arg(worktree_path)
        .current_dir(repo_path)
        .output()
        .context("failed to execute git worktree remove")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git worktree remove failed: {stderr}");
    }

    Ok(())
}

/// List all worktree paths for a git repository.
///
/// Parses the porcelain output of `git worktree list --porcelain`.
pub fn list_worktrees(repo_path: &Path) -> Result<Vec<String>> {
    let output = Command::new("git")
        .args(["worktree", "list", "--porcelain"])
        .current_dir(repo_path)
        .output()
        .context("failed to execute git worktree list")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git worktree list failed: {stderr}");
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut paths = Vec::new();

    for line in stdout.lines() {
        let Some(path) = line.strip_prefix("worktree ") else {
            continue;
        };
        paths.push(path.to_string());
    }

    Ok(paths)
}
