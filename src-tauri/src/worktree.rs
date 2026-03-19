use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};

/// Check whether a path is inside a git repository.
pub fn is_git_repo(path: &Path) -> bool {
    path.join(".git").exists()
}

/// List local branch names in the repository.
pub fn list_branches(repo_path: &Path) -> Result<Vec<String>> {
    let output = Command::new("git")
        .args(["branch", "--list", "--format=%(refname:short)"])
        .current_dir(repo_path)
        .output()
        .context("failed to execute git branch --list")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git branch --list failed: {stderr}");
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let branches: Vec<String> = stdout.lines().map(|l| l.to_string()).collect();
    Ok(branches)
}

/// Squash-merge `source` branch into `target` branch with a single commit message.
///
/// Uses a temporary detached worktree so the main repo directory is NEVER touched.
/// This prevents disrupting Vite/HMR or the running app.
pub fn squash_merge(repo_path: &Path, source: &str, target: &str, message: &str) -> Result<()> {
    let tmp_dir = repo_path.join(".worktrees").join("cluihud").join("_merge_tmp");

    // Clean up any leftover temp worktree
    if tmp_dir.exists() {
        let _ = Command::new("git")
            .args(["worktree", "remove", "--force"])
            .arg(&tmp_dir)
            .current_dir(repo_path)
            .output();
    }

    // Create temp worktree detached at target branch tip
    let add = Command::new("git")
        .args(["worktree", "add", "--detach"])
        .arg(&tmp_dir)
        .arg(target)
        .current_dir(repo_path)
        .output()
        .context("failed to create temp merge worktree")?;

    if !add.status.success() {
        let stderr = String::from_utf8_lossy(&add.stderr);
        anyhow::bail!("failed to create merge worktree: {stderr}");
    }

    // Squash merge in the temp worktree
    let merge = Command::new("git")
        .args(["merge", "--squash", source])
        .current_dir(&tmp_dir)
        .output()
        .context("failed to squash merge")?;

    if !merge.status.success() {
        let stderr = String::from_utf8_lossy(&merge.stderr);
        let stdout = String::from_utf8_lossy(&merge.stdout);
        let detail = if stderr.trim().is_empty() { stdout } else { stderr };
        let _ = Command::new("git").args(["merge", "--abort"]).current_dir(&tmp_dir).output();
        let _ = Command::new("git").args(["worktree", "remove", "--force"]).arg(&tmp_dir).current_dir(repo_path).output();
        anyhow::bail!("conflict:{detail}");
    }

    // Commit in the temp worktree (detached HEAD)
    let commit = Command::new("git")
        .args(["commit", "-m", message])
        .current_dir(&tmp_dir)
        .output()
        .context("failed to commit squash merge")?;

    if !commit.status.success() {
        let stderr = String::from_utf8_lossy(&commit.stderr);
        let stdout = String::from_utf8_lossy(&commit.stdout);
        // "nothing to commit" can appear in stdout or stderr
        if stderr.contains("nothing to commit") || stdout.contains("nothing to commit") {
            let _ = Command::new("git").args(["worktree", "remove", "--force"]).arg(&tmp_dir).current_dir(repo_path).output();
            return Ok(());
        }
        let detail = if stderr.trim().is_empty() { stdout } else { stderr };
        let _ = Command::new("git").args(["worktree", "remove", "--force"]).arg(&tmp_dir).current_dir(repo_path).output();
        anyhow::bail!("commit failed: {detail}");
    }

    // Get the new commit hash from the detached HEAD
    let rev = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(&tmp_dir)
        .output()
        .context("failed to get merge commit hash")?;
    let merge_commit = String::from_utf8_lossy(&rev.stdout).trim().to_string();

    // Fast-forward the target branch ref to include the merge commit
    let update = Command::new("git")
        .args(["update-ref", &format!("refs/heads/{target}"), &merge_commit])
        .current_dir(repo_path)
        .output()
        .context("failed to update target branch ref")?;

    if !update.status.success() {
        let stderr = String::from_utf8_lossy(&update.stderr);
        let _ = Command::new("git").args(["worktree", "remove", "--force"]).arg(&tmp_dir).current_dir(repo_path).output();
        anyhow::bail!("failed to update {target} ref: {stderr}");
    }

    // Clean up temp worktree
    let _ = Command::new("git")
        .args(["worktree", "remove", "--force"])
        .arg(&tmp_dir)
        .current_dir(repo_path)
        .output();

    Ok(())
}

/// Delete a local git branch forcefully.
pub fn delete_branch(repo_path: &Path, branch: &str) -> Result<()> {
    let output = Command::new("git")
        .args(["branch", "-D", branch])
        .current_dir(repo_path)
        .output()
        .context("failed to delete branch")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git branch -D failed: {stderr}");
    }

    Ok(())
}

/// Check if the worktree branch has commits ahead of main_branch.
pub fn has_commits_ahead(worktree_path: &Path, main_branch: &str) -> Result<bool> {
    let range = format!("{main_branch}..HEAD");
    let output = Command::new("git")
        .args(["log", &range, "--oneline"])
        .current_dir(worktree_path)
        .output()
        .context("failed to check commits ahead")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git log failed: {stderr}");
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(!stdout.trim().is_empty())
}

/// Count how many commits the current branch is ahead of `main_branch`.
pub fn commits_ahead_count(worktree_path: &Path, main_branch: &str) -> Result<u32> {
    let range = format!("{main_branch}..HEAD");
    let output = Command::new("git")
        .args(["rev-list", "--count", &range])
        .current_dir(worktree_path)
        .output()
        .context("failed to count commits ahead")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git rev-list --count failed: {stderr}");
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let count: u32 = stdout.trim().parse().unwrap_or(0);
    Ok(count)
}

/// Get the current branch name for a path.
pub fn current_branch(path: &Path) -> Result<String> {
    let output = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(path)
        .output()
        .context("failed to get current branch")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git rev-parse --abbrev-ref HEAD failed: {stderr}");
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Check if a worktree has uncommitted changes.
#[allow(dead_code)]
pub fn is_worktree_dirty(path: &Path) -> Result<bool> {
    let output = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(path)
        .output()
        .context("failed to check worktree status")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git status failed: {stderr}");
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(!stdout.trim().is_empty())
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

/// Get the unified diff for a single file against HEAD.
///
/// For tracked files, runs `git diff HEAD -- <relative_path>`.
/// For untracked/new files, runs `git diff --no-index /dev/null <relative_path>`.
/// Converts absolute paths to cwd-relative so git output stays clean.
pub fn file_diff(cwd: &Path, file_path: &str) -> Result<String> {
    let rel_path = Path::new(file_path)
        .strip_prefix(cwd)
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| file_path.to_string());

    let output = Command::new("git")
        .args(["diff", "HEAD", "--", &rel_path])
        .current_dir(cwd)
        .output()
        .context("failed to execute git diff")?;

    let stdout = String::from_utf8_lossy(&output.stdout);

    if !stdout.trim().is_empty() {
        return Ok(stdout.into_owned());
    }

    // Might be untracked — try --no-index against /dev/null
    let abs_path = if Path::new(file_path).is_absolute() {
        PathBuf::from(file_path)
    } else {
        cwd.join(file_path)
    };

    if !abs_path.exists() {
        return Ok(String::new());
    }

    let output = Command::new("git")
        .args(["diff", "--no-index", "/dev/null", &rel_path])
        .current_dir(cwd)
        .output()
        .context("failed to execute git diff --no-index")?;

    // --no-index returns exit code 1 when there are differences (not an error)
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// A file changed in the working tree, as reported by `git status`.
#[derive(Clone, serde::Serialize)]
pub struct ChangedFile {
    pub path: String,
    pub status: String,
}

/// List files changed in the working tree compared to HEAD.
///
/// Runs `git status --porcelain` and parses the output.
pub fn changed_files(cwd: &Path) -> Result<Vec<ChangedFile>> {
    let output = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(cwd)
        .output()
        .context("failed to execute git status")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git status failed: {stderr}");
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut files = Vec::new();

    for line in stdout.lines() {
        if line.len() < 4 {
            continue;
        }
        let xy = &line[..2];
        let rel_path = &line[3..];

        let status = match xy.trim() {
            "A" | "??" => "Create",
            "M" | "MM" => "Edit",
            "D" => "Delete",
            _ => "Edit",
        };

        // Return absolute paths to match hook event paths
        let abs_path = cwd.join(rel_path).to_string_lossy().into_owned();

        files.push(ChangedFile {
            path: abs_path,
            status: status.to_string(),
        });
    }

    Ok(files)
}

/// List all worktree paths for a git repository.
///
/// Parses the porcelain output of `git worktree list --porcelain`.
#[allow(dead_code)]
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
