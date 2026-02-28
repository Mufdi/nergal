use std::path::{Path, PathBuf};

use anyhow::Result;
use notify::Watcher;
use tokio::sync::mpsc;

#[derive(Debug, Clone)]
pub enum TranscriptEvent {
    Updated(PathBuf),
}

pub struct TranscriptWatcher {
    _watcher: notify::RecommendedWatcher,
}

impl TranscriptWatcher {
    pub fn new(watch_dir: &Path, tx: mpsc::Sender<TranscriptEvent>) -> Result<Self> {
        let watch_dir = watch_dir.to_path_buf();
        let mut watcher =
            notify::recommended_watcher(move |res: Result<notify::Event, notify::Error>| {
                let Ok(event) = res else { return };
                if event.kind.is_modify() || event.kind.is_create() {
                    for path in event.paths {
                        if path.extension().is_some_and(|ext| ext == "jsonl") {
                            let _ = tx.blocking_send(TranscriptEvent::Updated(path));
                        }
                    }
                }
            })?;

        watcher.watch(&watch_dir, notify::RecursiveMode::Recursive)?;

        Ok(Self { _watcher: watcher })
    }
}
