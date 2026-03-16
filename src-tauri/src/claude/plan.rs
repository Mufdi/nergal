#![allow(dead_code)]
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

/// Watches `~/.claude/plans/` for `.md` file changes via inotify.
/// Emits `plan:event` Tauri events instead of sending to a channel.
pub struct PlanWatcher {
    _watcher: notify::RecommendedWatcher,
}

impl PlanWatcher {
    pub fn new(plans_dir: &Path, app: AppHandle) -> Result<Self> {
        let mut watcher =
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

        watcher.watch(plans_dir, notify::RecursiveMode::NonRecursive)?;

        Ok(Self { _watcher: watcher })
    }
}

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

    pub fn current_path(&self) -> Option<&Path> {
        self.current_plan.as_ref().map(|p| p.path.as_path())
    }

    /// Scans plans_dir for the most recently modified `.md` file.
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

/// Thread-safe wrapper for PlanManager, managed as Tauri state.
pub type SharedPlanManager = Arc<Mutex<PlanManager>>;
