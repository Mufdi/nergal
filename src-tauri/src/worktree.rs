use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{Context, Result};

use crate::platform_spawn::NoWindow;

/// A `git` command with its console window suppressed on Windows. Every git
/// invocation here runs in the background; without this each would flash a
/// `cmd` window on Windows. No-op off Windows.
fn git() -> Command {
    let mut c = Command::new("git");
    c.no_window();
    c
}

/// Check whether a path is inside a git repository.
pub fn is_git_repo(path: &Path) -> bool {
    path.join(".git").exists()
}

/// Initialize a fresh git repository ("Init git" on a non-git workspace).
pub fn init_repo(path: &Path) -> Result<()> {
    let output = git()
        .args(["init"])
        .current_dir(path)
        .output()
        .context("failed to execute git init")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git init failed: {stderr}");
    }
    Ok(())
}

/// List local branch names in the repository.
pub fn list_branches(repo_path: &Path) -> Result<Vec<String>> {
    let output = git()
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

/// Prefix shared by every per-call merge temp worktree directory under
/// `.worktrees/nergal/`. Each full name is `<PREFIX><pid>.<seq>`.
const MERGE_TMP_PREFIX: &str = "_merge_tmp.";

/// Process-local counter disambiguating concurrent `squash_merge` calls in
/// the same process (mirrors the `atomic_write.rs` `SEQ` technique) — pid
/// alone isn't enough since two merges can run on different threads of the
/// same process.
static MERGE_SEQ: AtomicU64 = AtomicU64::new(0);

/// Derive a per-call unique temp worktree path so concurrent merges into the
/// same repo never share (and thus never force-remove) each other's worktree.
fn derive_merge_tmp_dir(repo_path: &Path) -> PathBuf {
    let pid = std::process::id();
    let seq = MERGE_SEQ.fetch_add(1, Ordering::Relaxed);
    repo_path
        .join(".worktrees")
        .join("nergal")
        .join(format!("{MERGE_TMP_PREFIX}{pid}.{seq}"))
}

/// Whether `pid` is currently running. Used by the stale-sweep to tell a
/// crashed leftover temp worktree (owning pid dead — safe to remove) apart
/// from a concurrent in-flight merge (owning pid alive — must not touch).
/// `sysinfo`-only per the cross-platform invariant: no `cfg(unix)` seam, no
/// `/proc` reads, no `libc::kill`.
fn pid_is_alive(pid: u32) -> bool {
    use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};
    let spid = Pid::from_u32(pid);
    let mut sys = System::new();
    sys.refresh_processes_specifics(
        ProcessesToUpdate::Some(&[spid]),
        false,
        ProcessRefreshKind::nothing(),
    );
    sys.process(spid).is_some()
}

/// Sweep `_merge_tmp.<pid>.<seq>` siblings whose embedded pid is no longer
/// alive, plus a best-effort `git worktree prune`. Runs on entry to
/// `squash_merge`, before creating this call's own temp worktree.
///
/// A sibling name that doesn't parse as `<PREFIX><pid>.<seq>` is left alone —
/// conservative by design: an unparseable name is more likely a future naming
/// change we don't understand than a safe-to-delete leftover, and the cost of
/// under-sweeping (a stray dir) is far lower than the cost of over-sweeping
/// (deleting a live concurrent merge).
fn sweep_stale_merge_worktrees(repo_path: &Path, nergal_worktrees_dir: &Path) {
    let Ok(entries) = std::fs::read_dir(nergal_worktrees_dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        let Some(rest) = name.strip_prefix(MERGE_TMP_PREFIX) else {
            continue;
        };
        let Some(pid_str) = rest.split('.').next() else {
            continue;
        };
        let Ok(pid) = pid_str.parse::<u32>() else {
            continue; // malformed — leave it, conservative
        };
        if pid_is_alive(pid) {
            continue; // owned by a concurrent in-flight merge — never touch
        }

        let stale_path = entry.path();
        let _ = git()
            .args(["worktree", "remove", "--force"])
            .arg(&stale_path)
            .current_dir(repo_path)
            .output();
        let _ = std::fs::remove_dir_all(&stale_path);
    }
    let _ = git()
        .args(["worktree", "prune"])
        .current_dir(repo_path)
        .output();
}

/// RAII guard that force-removes this call's temp merge worktree on drop,
/// regardless of which exit path `squash_merge` takes (success, conflict,
/// commit failure, ref-update failure). Replaces the ~5 manual cleanup calls
/// that used to be duplicated across every early return.
struct MergeWorktreeGuard {
    repo_path: PathBuf,
    tmp_dir: PathBuf,
}

impl Drop for MergeWorktreeGuard {
    fn drop(&mut self) {
        let _ = git()
            .args(["worktree", "remove", "--force"])
            .arg(&self.tmp_dir)
            .current_dir(&self.repo_path)
            .output();
        // Best-effort: `worktree remove` already deletes the dir on success;
        // this catches the case where it refused (e.g. dirty submodule) so no
        // `_merge_tmp.*` residue survives this call.
        let _ = std::fs::remove_dir_all(&self.tmp_dir);
    }
}

/// Squash-merge `source` branch into `target` branch with a single commit message.
///
/// Uses a temporary detached worktree so the main repo directory is NEVER touched.
/// This prevents disrupting Vite/HMR or the running app. The temp worktree lives
/// at a per-call unique path (see `derive_merge_tmp_dir`) so concurrent merges
/// into the same repo cannot interleave or delete each other's in-progress work.
pub fn squash_merge(repo_path: &Path, source: &str, target: &str, message: &str) -> Result<()> {
    let nergal_worktrees_dir = repo_path.join(".worktrees").join("nergal");
    sweep_stale_merge_worktrees(repo_path, &nergal_worktrees_dir);

    let tmp_dir = derive_merge_tmp_dir(repo_path);
    let _guard = MergeWorktreeGuard {
        repo_path: repo_path.to_path_buf(),
        tmp_dir: tmp_dir.clone(),
    };

    // Create temp worktree detached at target branch tip
    let add = git()
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
    let merge = git()
        .args(["merge", "--squash", source])
        .current_dir(&tmp_dir)
        .output()
        .context("failed to squash merge")?;

    if !merge.status.success() {
        let stderr = String::from_utf8_lossy(&merge.stderr);
        let stdout = String::from_utf8_lossy(&merge.stdout);
        let detail = if stderr.trim().is_empty() {
            stdout
        } else {
            stderr
        };
        let _ = git()
            .args(["merge", "--abort"])
            .current_dir(&tmp_dir)
            .output();
        anyhow::bail!("conflict:{detail}");
    }

    // Commit in the temp worktree (detached HEAD)
    let commit = git()
        .args(["commit", "-m", message])
        .current_dir(&tmp_dir)
        .output()
        .context("failed to commit squash merge")?;

    if !commit.status.success() {
        let stderr = String::from_utf8_lossy(&commit.stderr);
        let stdout = String::from_utf8_lossy(&commit.stdout);
        // "nothing to commit" can appear in stdout or stderr
        if stderr.contains("nothing to commit") || stdout.contains("nothing to commit") {
            return Ok(());
        }
        let detail = if stderr.trim().is_empty() {
            stdout
        } else {
            stderr
        };
        anyhow::bail!("commit failed: {detail}");
    }

    // Get the new commit hash from the detached HEAD
    let rev = git()
        .args(["rev-parse", "HEAD"])
        .current_dir(&tmp_dir)
        .output()
        .context("failed to get merge commit hash")?;
    let merge_commit = String::from_utf8_lossy(&rev.stdout).trim().to_string();

    // Fast-forward the target branch ref to include the merge commit
    let update = git()
        .args(["update-ref", &format!("refs/heads/{target}"), &merge_commit])
        .current_dir(repo_path)
        .output()
        .context("failed to update target branch ref")?;

    if !update.status.success() {
        let stderr = String::from_utf8_lossy(&update.stderr);
        anyhow::bail!("failed to update {target} ref: {stderr}");
    }

    Ok(())
}

/// Delete a local git branch forcefully.
pub fn delete_branch(repo_path: &Path, branch: &str) -> Result<()> {
    let output = git()
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
    let output = git()
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
    let output = git()
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
    let output = git()
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
    let output = git()
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

/// Create a git worktree at `<repo>/.worktrees/nergal/<slug>/` with branch `nergal/<slug>`.
///
/// If the branch already exists, reuses it. If the worktree directory already exists, returns it.
/// Returns the absolute path to the created worktree directory.
pub fn create_worktree(repo_path: &Path, slug: &str) -> Result<PathBuf> {
    let worktree_path = repo_path.join(".worktrees").join("nergal").join(slug);
    let branch_name = format!("nergal/{slug}");

    // Already exists on disk — reuse
    if worktree_path.exists() {
        return Ok(worktree_path);
    }

    // Try creating with new branch first
    let output = git()
        .args(["worktree", "add", "-b", &branch_name])
        .arg(&worktree_path)
        .current_dir(repo_path)
        .output()
        .context("failed to execute git worktree add")?;

    if output.status.success() {
        return Ok(worktree_path);
    }

    // Branch might already exist — try without -b
    let output2 = git()
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

/// Remove a git worktree forcefully. When `git worktree remove` refuses
/// (already-deleted dir, locked worktree, dirty submodule), fall back to
/// deleting the directory and pruning the stale registration — otherwise
/// the entry lingers in `git worktree list` forever.
pub fn remove_worktree(repo_path: &Path, worktree_path: &Path) -> Result<()> {
    let output = git()
        .args(["worktree", "remove", "--force"])
        .arg(worktree_path)
        .current_dir(repo_path)
        .output()
        .context("failed to execute git worktree remove")?;

    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();

    if worktree_path.exists() {
        std::fs::remove_dir_all(worktree_path).with_context(|| {
            format!("removing orphaned worktree dir {}", worktree_path.display())
        })?;
    }
    let prune = git()
        .args(["worktree", "prune"])
        .current_dir(repo_path)
        .output()
        .context("failed to execute git worktree prune")?;
    if !prune.status.success() {
        let prune_err = String::from_utf8_lossy(&prune.stderr);
        anyhow::bail!("git worktree remove failed ({stderr}); prune also failed: {prune_err}");
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

    let output = git()
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

    let output = git()
        .args(["diff", "--no-index", "/dev/null", &rel_path])
        .current_dir(cwd)
        .output()
        .context("failed to execute git diff --no-index")?;

    // --no-index returns exit code 1 when there are differences (not an error)
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Lines added/removed stats from `git diff --shortstat`.
pub struct DiffShortstat {
    pub lines_added: u32,
    pub lines_removed: u32,
}

/// Get total lines added/removed in the working tree compared to HEAD.
pub fn diff_shortstat(cwd: &Path) -> Result<DiffShortstat> {
    let output = git()
        .args(["diff", "HEAD", "--shortstat"])
        .current_dir(cwd)
        .output()
        .context("failed to execute git diff --shortstat")?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut added = 0u32;
    let mut removed = 0u32;

    // Format: " 5 files changed, 124 insertions(+), 13 deletions(-)"
    for part in stdout.split(',') {
        let part = part.trim();
        if part.contains("insertion") {
            if let Some(n) = part.split_whitespace().next().and_then(|s| s.parse().ok()) {
                added = n;
            }
        } else if part.contains("deletion")
            && let Some(n) = part.split_whitespace().next().and_then(|s| s.parse().ok())
        {
            removed = n;
        }
    }

    Ok(DiffShortstat {
        lines_added: added,
        lines_removed: removed,
    })
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
    let output = git()
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

/// List staged files via `git diff --cached --name-status`.
pub fn staged_files(cwd: &Path) -> Result<Vec<ChangedFile>> {
    let output = git()
        .args(["diff", "--cached", "--name-status"])
        .current_dir(cwd)
        .output()
        .context("failed to execute git diff --cached")?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut files = Vec::new();
    for line in stdout.lines() {
        let mut parts = line.splitn(2, '\t');
        let Some(status_char) = parts.next() else {
            continue;
        };
        let Some(path) = parts.next() else { continue };
        let status = match status_char.trim() {
            "A" => "Create",
            "M" => "Edit",
            "D" => "Delete",
            "R" => "Rename",
            _ => "Edit",
        };
        files.push(ChangedFile {
            path: path.to_string(),
            status: status.to_string(),
        });
    }
    Ok(files)
}

/// List unstaged (modified tracked) files via `git diff --name-status`.
pub fn unstaged_files(cwd: &Path) -> Result<Vec<ChangedFile>> {
    let output = git()
        .args(["diff", "--name-status"])
        .current_dir(cwd)
        .output()
        .context("failed to execute git diff")?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut files = Vec::new();
    for line in stdout.lines() {
        let mut parts = line.splitn(2, '\t');
        let Some(status_char) = parts.next() else {
            continue;
        };
        let Some(path) = parts.next() else { continue };
        let status = match status_char.trim() {
            "M" => "Edit",
            "D" => "Delete",
            _ => "Edit",
        };
        files.push(ChangedFile {
            path: path.to_string(),
            status: status.to_string(),
        });
    }
    Ok(files)
}

/// List untracked files via `git ls-files --others --exclude-standard`.
pub fn untracked_files(cwd: &Path) -> Result<Vec<String>> {
    let output = git()
        .args(["ls-files", "--others", "--exclude-standard"])
        .current_dir(cwd)
        .output()
        .context("failed to execute git ls-files")?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(stdout
        .lines()
        .filter(|l| !l.is_empty())
        .map(String::from)
        .collect())
}

/// Rename the current branch in place. Local-only by design: the remote
/// branch (and any open PR) keeps its name; the next push re-links via -u.
pub fn rename_current_branch(cwd: &Path, new_name: &str) -> Result<()> {
    let output = git()
        .args(["branch", "-m", "--", new_name])
        .current_dir(cwd)
        .output()
        .context("failed to execute git branch -m")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git branch -m failed: {stderr}");
    }
    Ok(())
}

/// Stage a single file.
///
/// `path` is caller-supplied, so it is vetted through
/// `fs_guard::resolve_within_base` before touching git (same convention as
/// `file_conflict_versions`) — `git` bounds traversal on its own in practice,
/// this is defense-in-depth consistency, not a live vuln fix.
///
/// Because the guard canonicalizes the leaf, a tracked symlink pointing outside
/// the worktree is fail-closed rejected, and one pointing inside stages its
/// resolved target (not the link entry). Uncommon for the git-panel use case
/// and identical to `file_conflict_versions`; accepted over byte-parity.
pub fn stage_file(cwd: &Path, path: &str) -> Result<()> {
    let safe = crate::fs_guard::resolve_within_base(cwd, path).map_err(|e| anyhow::anyhow!(e))?;
    let output = git()
        .args(["add", "--"])
        .arg(&safe)
        .current_dir(cwd)
        .output()
        .context("failed to execute git add")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git add failed: {stderr}");
    }
    Ok(())
}

/// Unstage a single file. See `stage_file` for the path-vetting rationale.
pub fn unstage_file(cwd: &Path, path: &str) -> Result<()> {
    let safe = crate::fs_guard::resolve_within_base(cwd, path).map_err(|e| anyhow::anyhow!(e))?;
    let output = git()
        .args(["restore", "--staged", "--"])
        .arg(&safe)
        .current_dir(cwd)
        .output()
        .context("failed to execute git restore --staged")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git restore --staged failed: {stderr}");
    }
    Ok(())
}

/// Stage all changes.
pub fn stage_all(cwd: &Path) -> Result<()> {
    let output = git()
        .args(["add", "-A"])
        .current_dir(cwd)
        .output()
        .context("failed to execute git add -A")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git add -A failed: {stderr}");
    }
    Ok(())
}

/// Unstage all staged changes.
pub fn unstage_all(cwd: &Path) -> Result<()> {
    let output = git()
        .args(["reset", "HEAD"])
        .current_dir(cwd)
        .output()
        .context("failed to execute git reset HEAD")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git reset HEAD failed: {stderr}");
    }
    Ok(())
}

/// Commit staged changes with a message. Returns the short commit hash.
pub fn commit(cwd: &Path, message: &str) -> Result<String> {
    let output = git()
        .args(["commit", "-m", message])
        .current_dir(cwd)
        .output()
        .context("failed to execute git commit")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout_str = String::from_utf8_lossy(&output.stdout);
        let detail = if stderr.trim().is_empty() {
            stdout_str
        } else {
            stderr
        };
        anyhow::bail!("{detail}");
    }
    let rev = git()
        .args(["rev-parse", "--short", "HEAD"])
        .current_dir(cwd)
        .output()
        .context("failed to get commit hash")?;
    Ok(String::from_utf8_lossy(&rev.stdout).trim().to_string())
}

/// A single commit entry from the log.
#[derive(Clone, serde::Serialize)]
pub struct CommitEntry {
    pub hash: String,
    pub message: String,
}

/// Get recent commits via `git log --oneline`.
/// If `range` is provided (e.g. "main..HEAD"), only shows commits in that range.
pub fn recent_commits(cwd: &Path, count: u32, range: Option<&str>) -> Result<Vec<CommitEntry>> {
    let mut args = vec!["log", "--oneline"];
    let count_str = format!("-{count}");
    args.push(&count_str);
    let range_owned;
    if let Some(r) = range {
        range_owned = r.to_string();
        args.push(&range_owned);
    }

    let output = git()
        .args(&args)
        .current_dir(cwd)
        .output()
        .context("failed to execute git log")?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut entries = Vec::new();
    for line in stdout.lines() {
        if let Some((hash, message)) = line.split_once(' ') {
            entries.push(CommitEntry {
                hash: hash.to_string(),
                message: message.to_string(),
            });
        }
    }
    Ok(entries)
}

/// PR info from GitHub CLI.
#[derive(Clone, serde::Serialize)]
pub struct PrInfo {
    pub number: u32,
    pub title: String,
    pub state: String,
    pub url: String,
}

/// Check if a PR exists for a branch via `gh pr view`.
pub fn pr_status(cwd: &Path, branch: &str) -> Result<Option<PrInfo>> {
    let output = Command::new("gh")
        .args(["pr", "view", branch, "--json", "number,title,state,url"])
        .current_dir(cwd)
        .output();

    let Ok(output) = output else { return Ok(None) };
    if !output.status.success() {
        return Ok(None);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let val: serde_json::Value = serde_json::from_str(&stdout).unwrap_or_default();

    let number = val.get("number").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
    let title = val
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let state = val
        .get("state")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let url = val
        .get("url")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    if number == 0 {
        return Ok(None);
    }

    Ok(Some(PrInfo {
        number,
        title,
        state,
        url,
    }))
}

/// Create a PR via `gh pr create`.
pub fn create_pr(cwd: &Path, branch: &str, base: &str, title: &str, body: &str) -> Result<PrInfo> {
    let output = Command::new("gh")
        .args([
            "pr", "create", "--head", branch, "--base", base, "--title", title, "--body", body,
        ])
        .current_dir(cwd)
        .output()
        .context("failed to execute gh pr create")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("gh pr create failed: {stderr}");
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let url = stdout
        .lines()
        .rev()
        .find(|l| l.starts_with("https://"))
        .unwrap_or("")
        .trim()
        .to_string();

    if let Some(info) = pr_status(cwd, branch)? {
        return Ok(info);
    }

    Ok(PrInfo {
        number: 0,
        title: title.to_string(),
        state: "OPEN".into(),
        url,
    })
}

/// Push the current branch to `origin` with upstream tracking.
/// Returns `true` if new commits were pushed, `false` if already up-to-date.
pub fn push(cwd: &Path, branch: &str) -> Result<bool> {
    let output = git()
        .args(["push", "-u", "origin", branch])
        .current_dir(cwd)
        .output()
        .context("failed to execute git push")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git push failed: {stderr}");
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let up_to_date = stderr.contains("Everything up-to-date");
    Ok(!up_to_date)
}

/// Commit shown in a PR preview.
#[derive(Clone, serde::Serialize)]
pub struct PrCommit {
    pub hash: String,
    pub subject: String,
}

/// Diffstat aggregate for a PR preview.
#[derive(Clone, serde::Serialize)]
pub struct PrDiffstat {
    pub added: u32,
    pub removed: u32,
    pub files: u32,
}

/// Data needed to prefill the Ship/PR preview dialog.
#[derive(Clone, serde::Serialize)]
pub struct PrPreviewData {
    pub base: String,
    pub commits: Vec<PrCommit>,
    pub diffstat: PrDiffstat,
    pub template: Option<String>,
    pub staged_count: u32,
    pub has_staged_diffstat: bool,
}

/// Build preview data for a PR: commits in `base..head`, diffstat, optional template.
pub fn pr_preview_data(cwd: &Path, base: &str, head: &str) -> Result<PrPreviewData> {
    let range = format!("{base}..{head}");

    let log_out = git()
        .args(["log", &range, "--format=%h%x00%s"])
        .current_dir(cwd)
        .output()
        .context("failed to execute git log for pr preview")?;
    if !log_out.status.success() {
        let stderr = String::from_utf8_lossy(&log_out.stderr);
        anyhow::bail!("git log {range} failed: {stderr}");
    }

    let log_stdout = String::from_utf8_lossy(&log_out.stdout);
    let mut seen_subjects: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut commits: Vec<PrCommit> = Vec::new();
    for line in log_stdout.lines() {
        let mut parts = line.splitn(2, '\0');
        let Some(hash) = parts.next() else { continue };
        let Some(subject) = parts.next() else {
            continue;
        };
        if !seen_subjects.insert(subject.to_string()) {
            continue;
        }
        commits.push(PrCommit {
            hash: hash.to_string(),
            subject: subject.to_string(),
        });
    }

    let diff_out = git()
        .args(["diff", "--shortstat", &range])
        .current_dir(cwd)
        .output()
        .context("failed to execute git diff --shortstat for pr preview")?;
    let diff_stdout = String::from_utf8_lossy(&diff_out.stdout);

    let mut added = 0u32;
    let mut removed = 0u32;
    let mut files = 0u32;
    for part in diff_stdout.split(',') {
        let part = part.trim();
        let parsed: Option<u32> = part.split_whitespace().next().and_then(|s| s.parse().ok());
        if let Some(n) = parsed {
            if part.contains("insertion") {
                added = n;
            } else if part.contains("deletion") {
                removed = n;
            } else if part.contains("file") {
                files = n;
            }
        }
    }

    let template_path = cwd.join(".nergal").join("pr-template.md");
    let template = std::fs::read_to_string(&template_path).ok();

    let staged = staged_files(cwd).unwrap_or_default();
    let staged_count = staged.len() as u32;
    let mut has_staged_diffstat = false;
    if staged_count > 0 {
        let staged_diff = git()
            .args(["diff", "--cached", "--shortstat"])
            .current_dir(cwd)
            .output();
        if let Ok(out) = staged_diff {
            has_staged_diffstat = !String::from_utf8_lossy(&out.stdout).trim().is_empty();
        }
    }

    Ok(PrPreviewData {
        base: base.to_string(),
        commits,
        diffstat: PrDiffstat {
            added,
            removed,
            files,
        },
        template,
        staged_count,
        has_staged_diffstat,
    })
}

/// CI checks aggregate from `gh pr checks`.
#[derive(Clone, serde::Serialize)]
pub struct PrChecks {
    pub passing: u32,
    pub failing: u32,
    pub pending: u32,
    pub total: u32,
}

/// Query CI checks for a PR via `gh pr checks <n> --json state,conclusion`.
pub fn pr_checks(cwd: &Path, pr_number: u32) -> Result<PrChecks> {
    let pr_arg = pr_number.to_string();
    let output = Command::new("gh")
        .args(["pr", "checks", &pr_arg, "--json", "state,conclusion"])
        .current_dir(cwd)
        .output()
        .context("failed to execute gh pr checks")?;

    // gh exits non-zero when any check is failing; the JSON is still valid on stdout.
    let stdout = String::from_utf8_lossy(&output.stdout);
    let val: serde_json::Value = match serde_json::from_str(&stdout) {
        Ok(v) => v,
        Err(_) => {
            return Ok(PrChecks {
                passing: 0,
                failing: 0,
                pending: 0,
                total: 0,
            });
        }
    };

    let arr = val.as_array().cloned().unwrap_or_default();
    let mut passing = 0u32;
    let mut failing = 0u32;
    let mut pending = 0u32;
    for item in &arr {
        let state = item.get("state").and_then(|v| v.as_str()).unwrap_or("");
        let conclusion = item
            .get("conclusion")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        match state {
            "COMPLETED" => match conclusion {
                "SUCCESS" | "NEUTRAL" | "SKIPPED" => passing += 1,
                "" => pending += 1,
                _ => failing += 1,
            },
            "IN_PROGRESS" | "QUEUED" | "PENDING" | "WAITING" => pending += 1,
            "" => pending += 1,
            _ => failing += 1,
        }
    }

    Ok(PrChecks {
        passing,
        failing,
        pending,
        total: arr.len() as u32,
    })
}

/// Whether `gh` is installed and authenticated.
pub fn gh_available() -> bool {
    let Ok(output) = Command::new("gh").args(["auth", "status"]).output() else {
        return false;
    };
    output.status.success()
}

/// Pull `target` branch into the current worktree via `git merge --no-ff --no-commit`,
/// leaving conflict markers on disk so the conflict tab can surface them.
/// Returns the list of conflicted files.
pub fn pull_target_into_worktree(cwd: &Path, target: &str) -> Result<Vec<String>> {
    let output = git()
        .args(["merge", "--no-ff", "--no-commit", target])
        .current_dir(cwd)
        .output()
        .context("failed to execute git merge")?;
    // Merge with conflicts exits non-zero but leaves markers; that's what we want.
    let _ = output;
    conflicted_files(cwd)
}

/// Finish a pending merge by committing staged resolutions.
pub fn complete_pending_merge(cwd: &Path) -> Result<String> {
    let output = git()
        .args(["commit", "--no-edit"])
        .current_dir(cwd)
        .output()
        .context("failed to execute git commit --no-edit")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git commit --no-edit failed: {stderr}");
    }
    let rev = git()
        .args(["rev-parse", "--short", "HEAD"])
        .current_dir(cwd)
        .output()
        .context("failed to get merge commit hash")?;
    Ok(String::from_utf8_lossy(&rev.stdout).trim().to_string())
}

/// Whether the working tree is in the middle of a merge (MERGE_HEAD exists).
pub fn has_pending_merge(cwd: &Path) -> bool {
    let output = git()
        .args(["rev-parse", "--verify", "--quiet", "MERGE_HEAD"])
        .current_dir(cwd)
        .output();
    matches!(output, Ok(o) if o.status.success())
}

/// List files with merge conflicts.
pub fn conflicted_files(cwd: &Path) -> Result<Vec<String>> {
    let output = git()
        .args(["status", "--porcelain"])
        .current_dir(cwd)
        .output()
        .context("failed to execute git status for conflicts")?;

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
        let is_conflict = matches!(xy, "UU" | "AA" | "DD" | "AU" | "UA" | "UD" | "DU");
        if is_conflict {
            files.push(rel_path.to_string());
        }
    }
    Ok(files)
}

/// Ours/theirs/merged content for a conflicted file.
#[derive(Clone, serde::Serialize)]
pub struct ConflictVersions {
    pub ours: String,
    pub theirs: String,
    pub merged: String,
}

fn git_show_stage(cwd: &Path, stage: u8, path: &str) -> String {
    let spec = format!(":{stage}:{path}");
    let output = git().args(["show", &spec]).current_dir(cwd).output();
    match output {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).into_owned(),
        _ => String::new(),
    }
}

/// Read ours/theirs/merged versions of a conflicted file.
///
/// `path` is caller-supplied (via `get_file_conflict_versions`), so it is
/// vetted through `fs_guard::resolve_within_base` before touching the
/// filesystem or `git show`. The repo-relative path handed to `git show` is
/// re-derived from the vetted, canonical path rather than trusting the raw
/// input — `git show` takes a ref-relative path, not an absolute one.
pub fn file_conflict_versions(cwd: &Path, path: &str) -> Result<ConflictVersions> {
    let merged_path =
        crate::fs_guard::resolve_within_base(cwd, path).map_err(|e| anyhow::anyhow!("{e}"))?;
    let canonical_cwd = dunce::canonicalize(cwd).context("failed to resolve session directory")?;
    let repo_rel_path = merged_path
        .strip_prefix(&canonical_cwd)
        .map_err(|_| anyhow::anyhow!("path escapes the session directory"))?
        .to_string_lossy()
        .into_owned();

    let ours = git_show_stage(cwd, 2, &repo_rel_path);
    let theirs = git_show_stage(cwd, 3, &repo_rel_path);
    let merged = std::fs::read_to_string(&merged_path).unwrap_or_default();
    Ok(ConflictVersions {
        ours,
        theirs,
        merged,
    })
}

/// Stage of the Ship pipeline, reported via progress callbacks.
#[derive(Clone, Copy, Debug)]
pub enum ShipStage {
    Commit,
    Push,
    Pr,
}

impl ShipStage {
    pub fn as_str(&self) -> &'static str {
        match self {
            ShipStage::Commit => "commit",
            ShipStage::Push => "push",
            ShipStage::Pr => "pr",
        }
    }
}

/// Result of a successful Ship pipeline.
#[derive(Clone, serde::Serialize)]
pub struct ShipResult {
    pub commit_hash: Option<String>,
    pub pr_info: PrInfo,
}

/// Enable auto-merge on an existing PR via `gh pr merge --auto --squash`.
pub fn enable_pr_auto_merge(cwd: &Path, pr_number: u32) -> Result<()> {
    let pr_arg = pr_number.to_string();
    let output = Command::new("gh")
        .args(["pr", "merge", &pr_arg, "--auto", "--squash"])
        .current_dir(cwd)
        .output()
        .context("failed to execute gh pr merge --auto")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("gh pr merge --auto failed: {stderr}");
    }
    Ok(())
}

/// Compose commit (optional) + push + create PR. Invokes `on_stage` callback
/// with `(ShipStage, Ok)` as each stage completes so the caller can emit events.
pub fn ship<F: Fn(ShipStage, bool)>(
    cwd: &Path,
    branch: &str,
    base: &str,
    commit_message: Option<&str>,
    pr_title: &str,
    pr_body: &str,
    on_stage: F,
) -> Result<ShipResult> {
    let commit_hash = if let Some(message) = commit_message.filter(|m| !m.trim().is_empty()) {
        let staged = staged_files(cwd).unwrap_or_default();
        if staged.is_empty() {
            None
        } else {
            match commit(cwd, message) {
                Ok(h) => {
                    on_stage(ShipStage::Commit, true);
                    Some(h)
                }
                Err(e) => {
                    on_stage(ShipStage::Commit, false);
                    return Err(e);
                }
            }
        }
    } else {
        None
    };

    match push(cwd, branch) {
        Ok(_) => on_stage(ShipStage::Push, true),
        Err(e) => {
            on_stage(ShipStage::Push, false);
            return Err(e);
        }
    }

    let pr_info = match create_pr(cwd, branch, base, pr_title, pr_body) {
        Ok(p) => {
            on_stage(ShipStage::Pr, true);
            p
        }
        Err(e) => {
            on_stage(ShipStage::Pr, false);
            return Err(e);
        }
    };

    Ok(ShipResult {
        commit_hash,
        pr_info,
    })
}

/// List all worktree paths for a git repository.
///
/// Parses the porcelain output of `git worktree list --porcelain`.
#[allow(dead_code)]
pub fn list_worktrees(repo_path: &Path) -> Result<Vec<String>> {
    let output = git()
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

/// A single entry in the stash stack.
#[derive(Clone, serde::Serialize)]
pub struct StashEntry {
    pub index: u32,
    pub message: String,
    pub branch: String,
    pub age: String,
}

/// Parse a single line of `git stash list --format='%gd%x09%cr%x09%gs'`.
///
/// Returns `None` for malformed lines. The `%gs` (subject) field has two
/// canonical shapes:
/// - `WIP on <branch>: <short-hash> <commit-subject>` — automatic stash
///   created without `-m`. We surface the commit subject as the message and
///   the branch as the captured branch.
/// - `On <branch>: <user-message>` — user-supplied message via `git stash
///   push -m <msg>`. The user-message is the visible part.
fn parse_stash_line(line: &str) -> Option<StashEntry> {
    let parts: Vec<&str> = line.splitn(3, '\t').collect();
    let [stash_ref, age, subject] = parts.as_slice() else {
        return None;
    };

    let inner = stash_ref.strip_prefix("stash@{")?.strip_suffix('}')?;
    let index: u32 = inner.parse().ok()?;

    let (branch, message) = if let Some(rest) = subject.strip_prefix("WIP on ") {
        let (b, m) = rest.split_once(": ")?;
        (b.to_string(), m.to_string())
    } else if let Some(rest) = subject.strip_prefix("On ") {
        let (b, m) = rest.split_once(": ")?;
        (b.to_string(), m.to_string())
    } else {
        ("".to_string(), subject.to_string())
    };

    Some(StashEntry {
        index,
        message,
        branch,
        age: age.to_string(),
    })
}

/// List all stashes on the stack.
pub fn stash_list(cwd: &Path) -> Result<Vec<StashEntry>> {
    let output = git()
        .args(["stash", "list", "--format=%gd%x09%cr%x09%gs"])
        .current_dir(cwd)
        .output()
        .context("failed to execute git stash list")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git stash list failed: {stderr}");
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut entries = Vec::new();
    for line in stdout.lines() {
        if let Some(entry) = parse_stash_line(line) {
            entries.push(entry);
        }
    }
    Ok(entries)
}

/// Create a new stash from the current working tree (including untracked
/// files via `-u`). When `message` is empty, git falls back to the auto
/// "WIP on …" subject.
pub fn stash_create(cwd: &Path, message: &str) -> Result<()> {
    let mut args: Vec<&str> = vec!["stash", "push", "-u"];
    if !message.is_empty() {
        args.push("-m");
        args.push(message);
    }
    let output = git()
        .args(&args)
        .current_dir(cwd)
        .output()
        .context("failed to execute git stash push")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git stash push failed: {stderr}");
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    // Git prints "No local changes to save" on stdout with success status when
    // there's nothing to stash. Surface that as an error so the UI can react.
    if stdout.contains("No local changes to save") {
        anyhow::bail!("No local changes to save");
    }
    Ok(())
}

fn stash_ref(index: u32) -> String {
    format!("stash@{{{index}}}")
}

/// Outcome of a `git stash apply`/`git stash pop`. Conflicts are reported as
/// data instead of as an error so the UI can route the user to the Conflicts
/// chip rather than surfacing a generic failure toast. `stash_kept` matches
/// git's own behavior: apply never drops the stash; pop drops it on a clean
/// merge but keeps it when conflicts force git to abort the drop step.
#[derive(Clone, serde::Serialize)]
pub struct StashApplyOutcome {
    pub conflicted_files: Vec<String>,
    pub stash_kept: bool,
}

/// Apply a stash without removing it from the stack. Conflicts produced by
/// the apply are returned as data — git exits non-zero when conflicts arise
/// but the apply itself succeeded with markers in the working tree.
pub fn stash_apply(cwd: &Path, index: u32) -> Result<StashApplyOutcome> {
    let output = git()
        .args(["stash", "apply", &stash_ref(index)])
        .current_dir(cwd)
        .output()
        .context("failed to execute git stash apply")?;
    if output.status.success() {
        return Ok(StashApplyOutcome {
            conflicted_files: Vec::new(),
            stash_kept: true,
        });
    }
    let conflicts = conflicted_files(cwd).unwrap_or_default();
    if !conflicts.is_empty() {
        return Ok(StashApplyOutcome {
            conflicted_files: conflicts,
            stash_kept: true,
        });
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    anyhow::bail!("git stash apply failed: {stderr}");
}

/// Apply a stash and drop it from the stack on success. On conflicts git
/// keeps the stash (so the user can retry), and we surface that via
/// `stash_kept = true`.
pub fn stash_pop(cwd: &Path, index: u32) -> Result<StashApplyOutcome> {
    let output = git()
        .args(["stash", "pop", &stash_ref(index)])
        .current_dir(cwd)
        .output()
        .context("failed to execute git stash pop")?;
    if output.status.success() {
        return Ok(StashApplyOutcome {
            conflicted_files: Vec::new(),
            stash_kept: false,
        });
    }
    let conflicts = conflicted_files(cwd).unwrap_or_default();
    if !conflicts.is_empty() {
        return Ok(StashApplyOutcome {
            conflicted_files: conflicts,
            stash_kept: true,
        });
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    anyhow::bail!("git stash pop failed: {stderr}");
}

/// Drop a stash without applying.
pub fn stash_drop(cwd: &Path, index: u32) -> Result<()> {
    let output = git()
        .args(["stash", "drop", &stash_ref(index)])
        .current_dir(cwd)
        .output()
        .context("failed to execute git stash drop")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git stash drop failed: {stderr}");
    }
    Ok(())
}

/// Show the file list of a stash via `git stash show --name-only`.
pub fn stash_show(cwd: &Path, index: u32) -> Result<Vec<String>> {
    let output = git()
        .args(["stash", "show", "--name-only", &stash_ref(index)])
        .current_dir(cwd)
        .output()
        .context("failed to execute git stash show")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git stash show failed: {stderr}");
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|l| !l.is_empty())
        .map(String::from)
        .collect())
}

/// Create a new branch from a stash via `git stash branch <name> <ref>`.
/// The stash is dropped on success per git's default behavior.
pub fn stash_branch(cwd: &Path, index: u32, branch_name: &str) -> Result<()> {
    let output = git()
        .args(["stash", "branch", branch_name, &stash_ref(index)])
        .current_dir(cwd)
        .output()
        .context("failed to execute git stash branch")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git stash branch failed: {stderr}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_stash_line_wip_form() {
        let line = "stash@{0}\t2 hours ago\tWIP on main: a1b2c3d fix login";
        let entry = parse_stash_line(line).expect("should parse");
        assert_eq!(entry.index, 0);
        assert_eq!(entry.branch, "main");
        assert_eq!(entry.message, "a1b2c3d fix login");
        assert_eq!(entry.age, "2 hours ago");
    }

    #[test]
    fn parse_stash_line_user_message_form() {
        let line = "stash@{3}\t5 minutes ago\tOn feature/x: rebase wip";
        let entry = parse_stash_line(line).expect("should parse");
        assert_eq!(entry.index, 3);
        assert_eq!(entry.branch, "feature/x");
        assert_eq!(entry.message, "rebase wip");
        assert_eq!(entry.age, "5 minutes ago");
    }

    #[test]
    fn parse_stash_line_malformed_returns_none() {
        assert!(parse_stash_line("not a stash line").is_none());
        assert!(parse_stash_line("stash@{abc}\t1m\tWIP on x: y").is_none());
        assert!(parse_stash_line("stash@{0}\tonly one tab").is_none());
    }

    #[test]
    fn parse_stash_line_message_with_colon() {
        // Colons inside the user message must not be split — only the first
        // ": " separator after the branch name counts.
        let line = "stash@{1}\t3 days ago\tOn main: feat: add new api endpoint";
        let entry = parse_stash_line(line).expect("should parse");
        assert_eq!(entry.branch, "main");
        assert_eq!(entry.message, "feat: add new api endpoint");
    }

    #[test]
    fn derive_merge_tmp_dir_is_unique_per_call() {
        let repo = Path::new("/tmp/fake-repo-for-derivation-test");
        let a = derive_merge_tmp_dir(repo);
        let b = derive_merge_tmp_dir(repo);
        assert_ne!(a, b, "two calls must never collide on the same tmp path");
        assert!(a.starts_with(repo.join(".worktrees").join("nergal")));
    }

    #[test]
    fn pid_is_alive_distinguishes_live_and_dead() {
        assert!(
            pid_is_alive(std::process::id()),
            "the test process itself must read as alive"
        );
        assert!(
            !pid_is_alive(u32::MAX),
            "a pid that cannot exist must read as dead"
        );
    }

    /// Full git plumbing for the concurrency test below.
    fn init_repo_with_two_branch_pairs(repo: &Path) {
        assert!(
            git()
                .args(["init", "-q", "-b", "main"])
                .current_dir(repo)
                .status()
                .unwrap()
                .success()
        );
        assert!(
            git()
                .args(["config", "user.email", "test@nergal.dev"])
                .current_dir(repo)
                .status()
                .unwrap()
                .success()
        );
        assert!(
            git()
                .args(["config", "user.name", "Nergal Test"])
                .current_dir(repo)
                .status()
                .unwrap()
                .success()
        );
        std::fs::write(repo.join("README.md"), "init").unwrap();
        assert!(
            git()
                .args(["add", "-A"])
                .current_dir(repo)
                .status()
                .unwrap()
                .success()
        );
        assert!(
            git()
                .args(["commit", "-q", "-m", "init"])
                .current_dir(repo)
                .status()
                .unwrap()
                .success()
        );

        // Two independent target branches off the same initial commit, so the
        // two concurrent merges below never race on the same ref update.
        for target in ["target1", "target2"] {
            assert!(
                git()
                    .args(["branch", target, "main"])
                    .current_dir(repo)
                    .status()
                    .unwrap()
                    .success()
            );
        }
        // Two source branches, each adding one distinct file.
        for (source, file) in [("source1", "a.txt"), ("source2", "b.txt")] {
            assert!(
                git()
                    .args(["checkout", "-q", "-b", source, "main"])
                    .current_dir(repo)
                    .status()
                    .unwrap()
                    .success()
            );
            std::fs::write(repo.join(file), "content").unwrap();
            assert!(
                git()
                    .args(["add", "-A"])
                    .current_dir(repo)
                    .status()
                    .unwrap()
                    .success()
            );
            let msg = format!("add {file}");
            assert!(
                git()
                    .args(["commit", "-q", "-m", &msg])
                    .current_dir(repo)
                    .status()
                    .unwrap()
                    .success()
            );
        }
        assert!(
            git()
                .args(["checkout", "-q", "main"])
                .current_dir(repo)
                .status()
                .unwrap()
                .success()
        );
    }

    /// Whether `path` exists in `target`'s tree, via `git cat-file -e`.
    fn tree_has_file(repo: &Path, target: &str, path: &str) -> bool {
        git()
            .args(["cat-file", "-e", &format!("{target}:{path}")])
            .current_dir(repo)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    #[test]
    fn squash_merge_concurrent_calls_do_not_interfere() {
        // Regression test for the bug this change fixes: the old code used ONE
        // fixed `_merge_tmp` path for every call, so two concurrent merges —
        // even into different targets — would race on the same directory and
        // the second's cleanup could delete the first's in-progress worktree.
        // With per-call unique paths, both merges below run genuinely
        // concurrently (synchronized to start together via a barrier) and must
        // each land only their own branch's content.
        use std::sync::{Arc, Barrier};

        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();
        init_repo_with_two_branch_pairs(repo);

        let barrier = Arc::new(Barrier::new(2));
        let (repo1, repo2) = (repo.to_path_buf(), repo.to_path_buf());
        let (b1, b2) = (barrier.clone(), barrier.clone());

        let t1 = std::thread::spawn(move || {
            b1.wait();
            squash_merge(&repo1, "source1", "target1", "merge source1")
        });
        let t2 = std::thread::spawn(move || {
            b2.wait();
            squash_merge(&repo2, "source2", "target2", "merge source2")
        });

        t1.join()
            .unwrap()
            .expect("merge into target1 should succeed");
        t2.join()
            .unwrap()
            .expect("merge into target2 should succeed");

        // Each target must contain ONLY its own branch's file — proves no
        // cross-merge content leakage through a shared temp worktree.
        assert!(tree_has_file(repo, "target1", "a.txt"));
        assert!(!tree_has_file(repo, "target1", "b.txt"));
        assert!(tree_has_file(repo, "target2", "b.txt"));
        assert!(!tree_has_file(repo, "target2", "a.txt"));

        // No leftover temp worktree directories after both merges complete.
        let worktrees_dir = repo.join(".worktrees").join("nergal");
        if worktrees_dir.exists() {
            let leftover: Vec<_> = std::fs::read_dir(&worktrees_dir)
                .unwrap()
                .flatten()
                .filter(|e| e.file_name().to_string_lossy().starts_with("_merge_tmp"))
                .collect();
            assert!(leftover.is_empty(), "leftover merge tmp dirs: {leftover:?}");
        }
    }

    #[test]
    fn squash_merge_sequential_calls_leave_no_residue() {
        // Complements the concurrency test with the simpler sequential case:
        // two back-to-back merges into the same target must both land
        // correctly and never accumulate `_merge_tmp*` residue between calls.
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();
        init_repo_with_two_branch_pairs(repo);

        squash_merge(repo, "source1", "target1", "merge source1").unwrap();
        squash_merge(repo, "source2", "target1", "merge source2").unwrap();

        assert!(tree_has_file(repo, "target1", "a.txt"));
        assert!(tree_has_file(repo, "target1", "b.txt"));

        let worktrees_dir = repo.join(".worktrees").join("nergal");
        if worktrees_dir.exists() {
            let leftover: Vec<_> = std::fs::read_dir(&worktrees_dir)
                .unwrap()
                .flatten()
                .filter(|e| e.file_name().to_string_lossy().starts_with("_merge_tmp"))
                .collect();
            assert!(leftover.is_empty(), "leftover merge tmp dirs: {leftover:?}");
        }
    }
}
