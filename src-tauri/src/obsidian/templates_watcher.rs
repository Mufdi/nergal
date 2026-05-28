use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use anyhow::{Context, Result};
use notify_debouncer_full::{Debouncer, RecommendedCache, new_debouncer};
use tauri::{AppHandle, Emitter};

use crate::obsidian::templates::{Template, list_templates_from_dir};

pub struct TemplatesWatcher {
    _debouncer: Debouncer<notify::RecommendedWatcher, RecommendedCache>,
    root: PathBuf,
}

impl TemplatesWatcher {
    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn spawn(templates_dir: PathBuf, app: AppHandle) -> Result<Self> {
        let dir_for_handler = templates_dir.clone();
        let mut debouncer = new_debouncer(
            Duration::from_millis(200),
            None,
            move |result: notify_debouncer_full::DebounceEventResult| {
                if let Err(errors) = &result {
                    for err in errors {
                        tracing::warn!("templates watcher error: {err}");
                    }
                    return;
                }
                emit_list(&dir_for_handler, &app);
            },
        )
        .context("creating templates debouncer")?;

        debouncer
            .watch(&templates_dir, notify::RecursiveMode::NonRecursive)
            .with_context(|| format!("watching templates dir: {}", templates_dir.display()))?;

        Ok(Self {
            _debouncer: debouncer,
            root: templates_dir,
        })
    }
}

fn emit_list(dir: &Path, app: &AppHandle) {
    if !dir.is_dir() {
        let _ = app.emit("obsidian:templates-updated", Vec::<Template>::new());
        return;
    }
    match list_templates_from_dir(dir) {
        Ok(list) => {
            let _ = app.emit("obsidian:templates-updated", list);
        }
        Err(e) => tracing::warn!("templates rescan failed: {e}"),
    }
}

pub struct TemplatesWatcherState {
    pub watcher: Mutex<Option<TemplatesWatcher>>,
}

impl TemplatesWatcherState {
    pub fn new() -> Self {
        Self {
            watcher: Mutex::new(None),
        }
    }

    pub fn current_root(&self) -> Option<PathBuf> {
        self.watcher
            .lock()
            .ok()
            .and_then(|g| g.as_ref().map(|w| w.root().to_path_buf()))
    }

    pub fn rewatch(&self, new_dir: Option<PathBuf>, app: AppHandle) -> Result<()> {
        if self.current_root() == new_dir {
            return Ok(());
        }
        {
            let mut guard = self.watcher.lock().expect("templates watcher mutex");
            *guard = None;
        }
        if let Some(dir) = new_dir
            && dir.is_dir()
        {
            let w = TemplatesWatcher::spawn(dir, app)?;
            *self.watcher.lock().expect("templates watcher mutex") = Some(w);
        }
        Ok(())
    }
}

impl Default for TemplatesWatcherState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_starts_empty() {
        let s = TemplatesWatcherState::new();
        assert!(s.current_root().is_none());
    }

    #[test]
    fn current_root_tracks_assignments() {
        let s = TemplatesWatcherState::new();
        assert!(s.current_root().is_none());
        *s.watcher.lock().unwrap() = None;
        assert!(s.current_root().is_none());
    }
}
