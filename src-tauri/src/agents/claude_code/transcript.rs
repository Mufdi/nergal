use std::path::{Path, PathBuf};

use anyhow::Result;
use notify::Watcher;
use serde::Serialize;
use tauri::{AppHandle, Emitter};

#[derive(Debug, Clone, Serialize)]
pub enum TranscriptEvent {
    Updated(PathBuf),
}

/// Watches transcript directories for `.jsonl` file changes.
/// Emits `transcript:event` Tauri events instead of sending to a channel.
pub struct TranscriptWatcher {
    _watcher: notify::RecommendedWatcher,
}

impl TranscriptWatcher {
    pub fn new(watch_dir: &Path, app: AppHandle) -> Result<Self> {
        let watch_dir = watch_dir.to_path_buf();
        let mut watcher =
            notify::recommended_watcher(move |res: Result<notify::Event, notify::Error>| {
                let Ok(event) = res else { return };
                if event.kind.is_modify() || event.kind.is_create() {
                    for path in event.paths {
                        if path.extension().is_some_and(|ext| ext == "jsonl") {
                            let _ = app.emit("transcript:event", TranscriptEvent::Updated(path));
                        }
                    }
                }
            })?;

        watcher.watch(&watch_dir, notify::RecursiveMode::Recursive)?;

        Ok(Self { _watcher: watcher })
    }
}
