use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use anyhow::Result;
use notify::Watcher;
use tauri::{AppHandle, Emitter};

/// Watches a project's `openspec/` directory and emits `openspec:changed`
/// Tauri events on create/modify/remove (debounced). The watched directory is
/// re-targetable at runtime via [`Self::retarget`] so it can follow the active
/// session — including overrides that point outside the code repo
/// (per-workspace configurable specs path).
pub struct OpenSpecWatcher {
    app: AppHandle,
    last_emit: Arc<AtomicU64>,
    watcher: Option<notify::RecommendedWatcher>,
    watched: Option<PathBuf>,
}

/// Shared handle so the watcher can be re-targeted from Tauri commands.
pub type SharedOpenSpecWatcher = Arc<Mutex<OpenSpecWatcher>>;

impl OpenSpecWatcher {
    pub fn new(app: AppHandle) -> Self {
        Self {
            app,
            last_emit: Arc::new(AtomicU64::new(0)),
            watcher: None,
            watched: None,
        }
    }

    /// Point the watcher at `dir`. No-op when already watching it; drops the
    /// previous watch otherwise. A non-existent `dir` clears the watch (the
    /// override may point at a path the user hasn't created yet).
    pub fn retarget(&mut self, dir: &Path) -> Result<()> {
        if self.watched.as_deref() == Some(dir) {
            return Ok(());
        }
        // Dropping the old watcher releases its inotify handles.
        self.watcher = None;
        self.watched = None;

        if !dir.exists() {
            tracing::debug!(
                "openspec watcher: target does not exist yet: {}",
                dir.display()
            );
            return Ok(());
        }

        let app = self.app.clone();
        let last_emit = self.last_emit.clone();
        let mut watcher =
            notify::recommended_watcher(move |res: Result<notify::Event, notify::Error>| {
                let Ok(event) = res else { return };
                if event.kind.is_modify() || event.kind.is_create() || event.kind.is_remove() {
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis() as u64;
                    let prev = last_emit.load(Ordering::Relaxed);
                    if now - prev < 500 {
                        return;
                    }
                    last_emit.store(now, Ordering::Relaxed);
                    tracing::debug!("openspec changed: {:?}", event.paths);
                    let _ = app.emit("openspec:changed", ());
                }
            })?;
        watcher.watch(dir, notify::RecursiveMode::Recursive)?;
        tracing::info!("openspec watcher targeting {}", dir.display());
        self.watcher = Some(watcher);
        self.watched = Some(dir.to_path_buf());
        Ok(())
    }
}
