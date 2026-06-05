//! Hot-reload watcher for pinned vault notes (N2). Watches the union of every
//! session's pinned-note files; on change it emits `vault:pinned-note-changed`
//! per affected session so the UI can offer an explicit re-inject (never auto).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use anyhow::{Context, Result};
use notify_debouncer_full::{Debouncer, RecommendedCache, new_debouncer};
use tauri::{AppHandle, Emitter};

#[derive(serde::Serialize, Clone)]
struct PinnedNoteChanged {
    session_id: String,
    path: String,
}

pub struct PinnedNotesWatcher {
    _debouncer: Debouncer<notify::RecommendedWatcher, RecommendedCache>,
    /// Sorted union of watched note paths, used to short-circuit rebuilds when
    /// the pin set hasn't changed.
    watched: Vec<PathBuf>,
}

/// Canonicalize best-effort; fall back to the raw path when the file can't be
/// resolved (e.g. a transient delete during an editor's save-replace).
fn canonical_or_raw(p: &Path) -> PathBuf {
    std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf())
}

impl PinnedNotesWatcher {
    pub fn watched(&self) -> &[PathBuf] {
        &self.watched
    }

    /// `pins` = (session_id, [absolute note paths]). Watches each note's parent
    /// directory (deduped) so editor rename-replace saves are still caught.
    fn spawn(pins: &[(String, Vec<String>)], app: AppHandle) -> Result<Self> {
        // Keyed by canonical path so OS events (which may report the canonical
        // or temp-rename path) resolve; the value carries the ORIGINAL pinned
        // path so the emitted event — and the reinject that follows — read the
        // exact path that was pinned, not the raw event path.
        let mut path_to_sessions: HashMap<PathBuf, Vec<(String, String)>> = HashMap::new();
        let mut watched: Vec<PathBuf> = Vec::new();
        let mut parents: Vec<PathBuf> = Vec::new();

        for (session_id, paths) in pins {
            for raw in paths {
                let p = PathBuf::from(raw);
                watched.push(p.clone());
                path_to_sessions
                    .entry(canonical_or_raw(&p))
                    .or_default()
                    .push((session_id.clone(), raw.clone()));
                if let Some(parent) = p.parent() {
                    let parent = parent.to_path_buf();
                    if !parents.contains(&parent) {
                        parents.push(parent);
                    }
                }
            }
        }
        watched.sort();
        watched.dedup();

        let mut debouncer = new_debouncer(
            Duration::from_millis(200),
            None,
            move |result: notify_debouncer_full::DebounceEventResult| {
                let events = match result {
                    Ok(events) => events,
                    Err(errors) => {
                        for err in &errors {
                            tracing::warn!("pinned-notes watcher error: {err}");
                        }
                        return;
                    }
                };
                for ev in events {
                    // Reading a note (ripgrep/walkdir during a vault search)
                    // raises Access events on the watched dir — those must NOT
                    // be reported as "the note changed". Only real writes count.
                    if !matches!(
                        ev.kind,
                        notify::EventKind::Modify(_) | notify::EventKind::Create(_)
                    ) {
                        continue;
                    }
                    for path in &ev.paths {
                        let key = canonical_or_raw(path);
                        if let Some(sessions) = path_to_sessions.get(&key) {
                            for (session_id, pinned_path) in sessions {
                                let _ = app.emit(
                                    "vault:pinned-note-changed",
                                    PinnedNoteChanged {
                                        session_id: session_id.clone(),
                                        path: pinned_path.clone(),
                                    },
                                );
                            }
                        }
                    }
                }
            },
        )
        .context("creating pinned-notes debouncer")?;

        for parent in &parents {
            if parent.is_dir() {
                debouncer
                    .watch(parent, notify::RecursiveMode::NonRecursive)
                    .with_context(|| format!("watching pinned-note dir: {}", parent.display()))?;
            }
        }

        Ok(Self {
            _debouncer: debouncer,
            watched,
        })
    }
}

pub struct PinnedNotesWatcherState {
    inner: Mutex<Option<PinnedNotesWatcher>>,
}

impl PinnedNotesWatcherState {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(None),
        }
    }

    fn current_watched(&self) -> Vec<PathBuf> {
        self.inner
            .lock()
            .ok()
            .and_then(|g| g.as_ref().map(|w| w.watched().to_vec()))
            .unwrap_or_default()
    }

    /// Rebuild the watcher from the current union of pinned paths. No-op when the
    /// union is unchanged; tears the watcher down entirely when nothing is pinned.
    pub fn rebuild(&self, pins: &[(String, Vec<String>)], app: AppHandle) -> Result<()> {
        let mut next: Vec<PathBuf> = pins
            .iter()
            .flat_map(|(_, paths)| paths.iter().map(PathBuf::from))
            .collect();
        next.sort();
        next.dedup();

        if self.current_watched() == next {
            return Ok(());
        }
        {
            let mut guard = self
                .inner
                .lock()
                .map_err(|_| anyhow::anyhow!("pinned-notes watcher mutex poisoned"))?;
            *guard = None;
        }
        if !next.is_empty() {
            let w = PinnedNotesWatcher::spawn(pins, app)?;
            *self
                .inner
                .lock()
                .map_err(|_| anyhow::anyhow!("pinned-notes watcher mutex poisoned"))? = Some(w);
        }
        Ok(())
    }
}

impl Default for PinnedNotesWatcherState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_starts_empty() {
        let s = PinnedNotesWatcherState::new();
        assert!(s.current_watched().is_empty());
    }
}
