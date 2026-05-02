//! Filesystem watcher for the scratchpad directory.
//!
//! Emits Tauri events:
//! - `scratchpad:tab-changed` `{ tab_id, hash_hex }` when a note's content
//!   on disk has a hash NOT in the per-file own-write ring buffer (i.e. the
//!   change came from outside cluihud).
//! - `scratchpad:tab-deleted` `{ tab_id }` when a tracked note disappears.
//! - `scratchpad:dir-missing` when the watched root itself is removed.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use notify::EventKind;
use notify_debouncer_full::{Debouncer, RecommendedCache, new_debouncer};
use serde::Serialize;
use tauri::{AppHandle, Emitter};

use super::{MAX_NOTE_BYTES, OwnWriteHashes, sha256, tab_id_from_filename};

#[derive(Debug, Clone, Serialize)]
struct TabChangedPayload {
    tab_id: String,
    hash_hex: String,
}

#[derive(Debug, Clone, Serialize)]
struct TabDeletedPayload {
    tab_id: String,
}

pub struct ScratchpadWatcher {
    _debouncer: Debouncer<notify::RecommendedWatcher, RecommendedCache>,
    root: PathBuf,
}

impl ScratchpadWatcher {
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Spawn a debounced watcher on `scratchpad_root`. Events are filtered
    /// against the canonical `scratch-{uuid}.md` pattern; own-writes (whose
    /// content hash is in the ring buffer) are ignored.
    pub fn spawn(
        scratchpad_root: PathBuf,
        own_writes: Arc<OwnWriteHashes>,
        app: AppHandle,
    ) -> Result<Self> {
        let root_for_handler = scratchpad_root.clone();
        let mut debouncer = new_debouncer(
            Duration::from_millis(200),
            None,
            move |result: notify_debouncer_full::DebounceEventResult| {
                let events = match result {
                    Ok(events) => events,
                    Err(errors) => {
                        for err in errors {
                            tracing::warn!("scratchpad watcher error: {err}");
                        }
                        return;
                    }
                };
                for ev in events {
                    handle_event(&ev.event, &root_for_handler, own_writes.as_ref(), &app);
                }
            },
        )
        .context("creating debouncer")?;

        debouncer
            .watch(&scratchpad_root, notify::RecursiveMode::NonRecursive)
            .with_context(|| format!("watching scratchpad root: {}", scratchpad_root.display()))?;

        Ok(Self {
            _debouncer: debouncer,
            root: scratchpad_root,
        })
    }
}

fn handle_event(event: &notify::Event, root: &Path, own_writes: &OwnWriteHashes, app: &AppHandle) {
    if !root.exists() {
        let _ = app.emit("scratchpad:dir-missing", ());
        return;
    }

    let kind = &event.kind;
    if !matches!(
        kind,
        EventKind::Modify(_) | EventKind::Create(_) | EventKind::Remove(_)
    ) {
        return;
    }

    for path in &event.paths {
        let Some(file_name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if file_name.starts_with('.') {
            continue;
        }
        let Some(tab_id) = tab_id_from_filename(file_name) else {
            continue;
        };

        if matches!(kind, EventKind::Remove(_)) || !path.exists() {
            own_writes.forget(&tab_id);
            let _ = app.emit("scratchpad:tab-deleted", TabDeletedPayload { tab_id });
            continue;
        }

        // Reject symlinks discovered post-creation.
        if let Ok(meta) = fs::symlink_metadata(path) {
            if meta.is_symlink() || !meta.is_file() {
                continue;
            }
            if meta.len() > MAX_NOTE_BYTES {
                continue;
            }
        } else {
            continue;
        }

        let bytes = match fs::read(path) {
            Ok(b) => b,
            Err(_) => continue,
        };
        let hash = sha256(&bytes);
        if own_writes.contains(&tab_id, &hash) {
            // It's our own recent write; suppress.
            continue;
        }

        let hash_hex = hex_lower(&hash);
        let _ = app.emit(
            "scratchpad:tab-changed",
            TabChangedPayload { tab_id, hash_hex },
        );
    }
}

fn hex_lower(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push(hex_digit(b >> 4));
        s.push(hex_digit(b & 0xf));
    }
    s
}

fn hex_digit(n: u8) -> char {
    match n {
        0..=9 => (b'0' + n) as char,
        10..=15 => (b'a' + n - 10) as char,
        _ => unreachable!(),
    }
}
