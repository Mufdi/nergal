use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// Manages plan files in Claude's plans directory.
pub struct PlanManager {
    pub plans_dir: PathBuf,
    pub current_plan: Option<PlanFile>,
}

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
