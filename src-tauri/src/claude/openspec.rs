use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::Result;
use notify::Watcher;
use tauri::{AppHandle, Emitter};

/// Watches the `openspec/` directory for any file changes.
/// Emits `openspec:changed` Tauri events on create/modify/remove.
/// Debounces to avoid flooding the frontend with rapid successive events.
pub struct OpenSpecWatcher {
    _watcher: notify::RecommendedWatcher,
}

impl OpenSpecWatcher {
    pub fn new(openspec_dir: &Path, app: AppHandle) -> Result<Self> {
        let last_emit = std::sync::Arc::new(AtomicU64::new(0));

        let mut watcher =
            notify::recommended_watcher(move |res: Result<notify::Event, notify::Error>| {
                let Ok(event) = res else { return };
                if event.kind.is_modify()
                    || event.kind.is_create()
                    || event.kind.is_remove()
                {
                    // Debounce: skip if less than 500ms since last emit
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

        watcher.watch(openspec_dir, notify::RecursiveMode::Recursive)?;

        Ok(Self { _watcher: watcher })
    }
}
