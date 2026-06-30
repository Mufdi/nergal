#![allow(dead_code)]
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use notify::Watcher;
use serde::Serialize;
use tauri::{AppHandle, Emitter};

#[derive(Debug, Clone, Serialize)]
pub enum PlanEvent {
    Updated(PathBuf),
}

/// Watches `.md` file changes on a dynamic set of plans directories via
/// inotify and emits `plan:event` Tauri events. The set is mutable so the
/// app can extend coverage as sessions land in cwds the boot-time watcher
/// didn't know about (worktrees outside the first workspace).
pub struct PlanWatcher {
    watcher: notify::RecommendedWatcher,
    watched: HashSet<PathBuf>,
}

impl PlanWatcher {
    pub fn new(app: AppHandle) -> Result<Self> {
        let watcher =
            notify::recommended_watcher(move |res: Result<notify::Event, notify::Error>| {
                let Ok(event) = res else { return };
                if event.kind.is_modify() || event.kind.is_create() {
                    for path in event.paths {
                        if path.extension().is_some_and(|ext| ext == "md") {
                            let _ = app.emit("plan:event", PlanEvent::Updated(path));
                        }
                    }
                }
            })?;
        Ok(Self {
            watcher,
            watched: HashSet::new(),
        })
    }

    /// Idempotent: adding an already-watched path is a no-op. Skips paths
    /// that don't yet exist on disk; callers re-invoke on plans-dir creation
    /// via [`Self::ensure_dir_and_watch`].
    pub fn watch_dir(&mut self, dir: &Path) -> Result<()> {
        if self.watched.contains(dir) {
            return Ok(());
        }
        if !dir.exists() {
            return Ok(());
        }
        self.watcher
            .watch(dir, notify::RecursiveMode::NonRecursive)
            .with_context(|| format!("watch dir: {}", dir.display()))?;
        self.watched.insert(dir.to_path_buf());
        Ok(())
    }

    /// Create the dir if missing, then watch it. Useful at session-create
    /// time where the worktree's `.claude/plans/` may not exist yet.
    pub fn ensure_dir_and_watch(&mut self, dir: &Path) -> Result<()> {
        std::fs::create_dir_all(dir).with_context(|| format!("mkdir: {}", dir.display()))?;
        self.watch_dir(dir)
    }
}

pub type SharedPlanWatcher = Arc<Mutex<PlanWatcher>>;

/// Manages plan files in Claude's plans directory.
#[derive(Debug)]
pub struct PlanManager {
    pub plans_dir: PathBuf,
    pub current_plan: Option<PlanFile>,
}

#[derive(Debug)]
pub struct PlanFile {
    pub path: PathBuf,
    pub content: String,
    pub original: String,
}

impl PlanFile {
    /// Returns true if the content has been modified from the original.
    pub fn has_edits(&self) -> bool {
        self.content != self.original
    }
}

impl PlanManager {
    pub fn new(plans_dir: PathBuf) -> Self {
        Self {
            plans_dir,
            current_plan: None,
        }
    }

    pub fn load_plan(&mut self, path: &Path) -> Result<()> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("reading plan: {}", path.display()))?;
        self.current_plan = Some(PlanFile {
            path: path.to_path_buf(),
            original: content.clone(),
            content,
        });
        Ok(())
    }

    pub fn current_content(&self) -> Option<&str> {
        self.current_plan.as_ref().map(|p| p.content.as_str())
    }

    /// Set the active plan from content already in hand, skipping the disk read.
    /// CC delivers the plan markdown inline in the ExitPlanMode hook payload, so
    /// the exact text is known before (or without) a backing file — this avoids
    /// the mtime race of re-reading "the latest file".
    pub fn set_plan(&mut self, path: PathBuf, content: String) {
        self.current_plan = Some(PlanFile {
            path,
            original: content.clone(),
            content,
        });
    }

    pub fn current_path(&self) -> Option<&Path> {
        self.current_plan.as_ref().map(|p| p.path.as_path())
    }

    /// Scans plans_dir for the most recently modified `.md` file.
    ///
    /// This is the last-resort heuristic: prefer [`find_plan_file_by_content`]
    /// when the plan's exact text is known (CC delivers it inline in the hook
    /// payload), since "newest mtime" can pick the wrong plan when two sessions
    /// emit concurrently.
    pub fn find_latest_plan(&self) -> Result<Option<PathBuf>> {
        if !self.plans_dir.exists() {
            return Ok(None);
        }

        let mut latest: Option<(PathBuf, std::time::SystemTime)> = None;

        for entry in std::fs::read_dir(&self.plans_dir)
            .with_context(|| format!("reading plans dir: {}", self.plans_dir.display()))?
        {
            let entry = entry?;
            let path = entry.path();

            let Some(ext) = path.extension() else {
                continue;
            };
            if ext != "md" {
                continue;
            }

            let modified = entry.metadata()?.modified()?;
            let dominated = latest.as_ref().is_some_and(|(_, t)| *t >= modified);
            if !dominated {
                latest = Some((path, modified));
            }
        }

        Ok(latest.map(|(p, _)| p))
    }

    /// Updates the in-memory content and writes edits to disk.
    pub fn save_edits(&mut self, content: String) -> Result<PathBuf> {
        let Some(plan) = self.current_plan.as_mut() else {
            anyhow::bail!("no plan loaded to save edits to");
        };

        plan.content = content;
        std::fs::write(&plan.path, &plan.content)
            .with_context(|| format!("writing plan edits: {}", plan.path.display()))?;

        Ok(plan.path.clone())
    }
}

/// Finds the `.md` file in `dir` whose content exactly matches `content`.
///
/// CC writes the plan to its plansDirectory AND delivers the same markdown
/// inline in the ExitPlanMode hook payload. Matching by content identity
/// locates the exact backing file without the mtime race of "newest file wins"
/// — so the edit/re-inject round-trip writes back to the plan the user is
/// actually looking at. Returns `None` if the dir is unreadable or no file
/// matches (e.g. CC hasn't flushed it yet).
pub fn find_plan_file_by_content(dir: &Path, content: &str) -> Option<PathBuf> {
    let entries = std::fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|ext| ext == "md")
            && std::fs::read_to_string(&path).is_ok_and(|c| c == content)
        {
            return Some(path);
        }
    }
    None
}

/// Thread-safe wrapper for PlanManager, managed as Tauri state.
pub type SharedPlanManager = Arc<Mutex<PlanManager>>;
