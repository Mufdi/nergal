use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use super::{CommitEntry, branch_mid_operation, git, remove_worktree};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BranchCandidate {
    pub name: String,
    pub sha: String,
    pub last_commit_unix: i64,
    pub last_subject: String,
}

// The full refname is used because `refname:short` turns into `heads/nergal/x`
// when a tag shares the name.
pub fn nergal_branch_candidates(repo: &Path) -> Result<Vec<BranchCandidate>> {
    let output = git()
        .args([
            "for-each-ref",
            "refs/heads/nergal/",
            "--format=%(refname)%09%(objectname)%09%(symref)%09%(committerdate:unix)%09%(subject)",
        ])
        .current_dir(repo)
        .output()
        .context("failed to execute git for-each-ref")?;
    if !output.status.success() {
        bail!(
            "git for-each-ref failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let mut candidates: Vec<BranchCandidate> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| {
            let mut fields = line.splitn(5, '\t');
            let refname = fields.next()?;
            let sha = fields.next()?;
            let symref = fields.next()?;
            let unix = fields.next()?;
            let subject = fields.next().unwrap_or_default();
            if !symref.is_empty() {
                return None;
            }
            Some(BranchCandidate {
                name: refname.strip_prefix("refs/heads/")?.to_string(),
                sha: sha.to_string(),
                last_commit_unix: unix.parse().ok()?,
                last_subject: subject.to_string(),
            })
        })
        .collect();
    candidates.sort_by_key(|c| std::cmp::Reverse(c.last_commit_unix));
    Ok(candidates)
}

fn target_ref(target: Option<&str>) -> String {
    target.map_or_else(|| "HEAD".to_string(), |t| format!("refs/heads/{t}"))
}

pub fn pending_commit_count(repo: &Path, sha: &str, target: Option<&str>) -> Option<u32> {
    let out = git()
        .args(["rev-list", "--count", sha, "--not", "--remotes"])
        .arg(target_ref(target))
        .arg("--")
        .current_dir(repo)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8_lossy(&out.stdout).trim().parse().ok()
}

pub fn pending_commits(
    repo: &Path,
    sha: &str,
    target: Option<&str>,
    limit: u32,
) -> Result<Vec<CommitEntry>> {
    let output = git()
        .args(["log", "--format=%h%x09%s"])
        .arg(format!("-{limit}"))
        .args([sha, "--not", "--remotes"])
        .arg(target_ref(target))
        .arg("--")
        .current_dir(repo)
        .output()
        .context("failed to execute git log")?;
    if !output.status.success() {
        bail!(
            "git log failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| {
            let (hash, message) = line.split_once('\t')?;
            Some(CommitEntry {
                hash: hash.to_string(),
                message: message.to_string(),
            })
        })
        .collect())
}

pub fn is_valid_commit_id(hash: &str) -> bool {
    (4..=64).contains(&hash.len()) && hash.bytes().all(|b| b.is_ascii_hexdigit())
}

// A branch named like a hash resolves through `rev-parse` to some other commit,
// so the full id must still start with the requested prefix.
pub fn resolve_commit(base: &Path, hash: &str) -> Result<String> {
    if !is_valid_commit_id(hash) {
        bail!("invalid commit id");
    }
    let output = git()
        .args(["rev-parse", "--verify", "--quiet"])
        .arg(format!("{hash}^{{commit}}"))
        .current_dir(base)
        .output()
        .context("failed to execute git rev-parse")?;
    let full = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if !output.status.success() || !full.starts_with(&hash.to_ascii_lowercase()) {
        bail!("unknown commit {hash}");
    }
    Ok(full)
}

pub fn nergal_slug(branch: &str) -> Option<&str> {
    let slug = branch.strip_prefix("nergal/")?;
    let mut chars = slug.chars();
    let first = chars.next()?;
    let first_ok = first.is_ascii_alphanumeric() || first == '_' || first == '-';
    let rest_ok = chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
    (first_ok && rest_ok).then_some(slug)
}

// `diff-tree -m --first-parent <merge>` still lists the diff against every
// parent on the git versions we ship against, so the first parent is passed
// explicitly as the old tree.
fn commit_trees(base: &Path, full: &str) -> Vec<String> {
    let first_parent = git()
        .args(["rev-parse", "--verify", "--quiet"])
        .arg(format!("{full}^1"))
        .current_dir(base)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty());
    match first_parent {
        Some(parent) => vec![parent, full.to_string()],
        None => vec!["--root".to_string(), full.to_string()],
    }
}

pub fn commit_files(base: &Path, hash: &str) -> Result<Vec<String>> {
    let full = resolve_commit(base, hash)?;
    let output = git()
        .args(["diff-tree", "--no-commit-id", "-r", "--name-only", "-z"])
        .args(commit_trees(base, &full))
        .current_dir(base)
        .output()
        .context("failed to execute git diff-tree")?;
    if !output.status.success() {
        bail!(
            "git diff-tree failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    // Without -z git C-quotes non-ASCII names, which then match no file.
    Ok(output
        .stdout
        .split(|b| *b == 0)
        .filter(|name| !name.is_empty())
        .map(|name| {
            base.join(String::from_utf8_lossy(name).as_ref())
                .to_string_lossy()
                .into_owned()
        })
        .collect())
}

pub fn commit_file_diff(base: &Path, hash: &str, file_path: &str) -> Result<String> {
    let given = Path::new(file_path);
    let rel = given.strip_prefix(base).unwrap_or(given);
    if rel.has_root()
        || rel.is_absolute()
        || rel
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        bail!("path {file_path} is outside the repository");
    }
    let full = resolve_commit(base, hash)?;
    let output = git()
        .args([
            "--literal-pathspecs",
            "diff-tree",
            "-p",
            "--no-color",
            "--no-commit-id",
        ])
        .args(commit_trees(base, &full))
        .arg("--")
        .arg(rel)
        .current_dir(base)
        .output()
        .context("failed to execute git diff-tree")?;
    if !output.status.success() {
        bail!(
            "git diff-tree failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreeEntry {
    pub path: PathBuf,
    pub branch: Option<String>,
    pub locked: bool,
    pub prunable: bool,
}

pub fn worktree_entries(repo: &Path) -> Result<Vec<WorktreeEntry>> {
    let output = git()
        .args(["worktree", "list", "--porcelain"])
        .current_dir(repo)
        .output()
        .context("failed to execute git worktree list")?;
    if !output.status.success() {
        bail!(
            "git worktree list failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let mut entries: Vec<WorktreeEntry> = Vec::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        if let Some(path) = line.strip_prefix("worktree ") {
            entries.push(WorktreeEntry {
                path: PathBuf::from(path),
                branch: None,
                locked: false,
                prunable: false,
            });
            continue;
        }
        let Some(entry) = entries.last_mut() else {
            continue;
        };
        if let Some(branch) = line.strip_prefix("branch refs/heads/") {
            entry.branch = Some(branch.to_string());
        } else if line == "locked" || line.starts_with("locked ") {
            entry.locked = true;
        } else if line == "prunable" || line.starts_with("prunable ") {
            entry.prunable = true;
        }
    }
    Ok(entries)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BranchUsage {
    Free,
    InUse,
    MissingWorktree { paths: Vec<PathBuf> },
}

fn path_is_gone(path: &Path) -> bool {
    matches!(
        std::fs::symlink_metadata(path),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound
    )
}

pub fn branch_usage(repo: &Path, branch: &str) -> BranchUsage {
    let Ok(entries) = worktree_entries(repo) else {
        return BranchUsage::InUse;
    };
    let mut missing = Vec::new();
    for entry in entries
        .iter()
        .filter(|e| e.branch.as_deref() == Some(branch))
    {
        if entry.locked || !entry.prunable || !path_is_gone(&entry.path) {
            return BranchUsage::InUse;
        }
        missing.push(entry.path.clone());
    }
    if branch_mid_operation(repo, branch) {
        return BranchUsage::InUse;
    }
    if missing.is_empty() {
        BranchUsage::Free
    } else {
        BranchUsage::MissingWorktree { paths: missing }
    }
}

pub fn clear_missing_worktrees(repo: &Path, branch: &str) -> Result<Vec<PathBuf>> {
    let mut removed = Vec::new();
    for entry in worktree_entries(repo)?
        .into_iter()
        .filter(|e| e.branch.as_deref() == Some(branch) && !e.locked && e.prunable)
    {
        if !path_is_gone(&entry.path) {
            continue;
        }
        let output = git()
            .args(["worktree", "remove", "--force"])
            .arg(&entry.path)
            .current_dir(repo)
            .output()
            .context("failed to execute git worktree remove")?;
        if !output.status.success() {
            bail!(
                "git worktree remove failed for {}: {}",
                entry.path.display(),
                String::from_utf8_lossy(&output.stderr)
            );
        }
        removed.push(entry.path);
    }
    Ok(removed)
}

// The short branch name makes git attach the worktree to the branch; a full
// `refs/heads/...` ref would detach HEAD and lose later commits on delete.
pub fn add_worktree_for_branch(repo: &Path, slug: &str) -> Result<PathBuf> {
    let path = repo.join(".worktrees").join("nergal").join(slug);
    if std::fs::symlink_metadata(&path).is_ok() {
        bail!("{} already exists", path.display());
    }
    let branch = format!("nergal/{slug}");
    let output = git()
        .args(["worktree", "add"])
        .arg(&path)
        .arg(&branch)
        .current_dir(repo)
        .output()
        .context("failed to execute git worktree add")?;
    if !output.status.success() {
        bail!(
            "git worktree add failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let head = git()
        .args(["symbolic-ref", "-q", "HEAD"])
        .current_dir(&path)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());
    if head.as_deref() != Some(format!("refs/heads/{branch}").as_str()) {
        if let Err(e) = remove_worktree(repo, &path) {
            tracing::warn!(error = %e, worktree = %path.display(), "could not roll back a detached worktree");
        }
        bail!("worktree for {branch} did not attach to the branch");
    }
    Ok(path)
}
