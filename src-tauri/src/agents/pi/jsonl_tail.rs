//! Tail-`-f` for Pi's session JSONL files.
//!
//! Reads the existing content on start (catch-up), then watches for changes
//! via `notify` and reads only the appended bytes. Each parsed line becomes
//! a [`TranscriptEvent`] which is wrapped in the appropriate [`HookEvent`]
//! shape and forwarded to the runtime's `EventSink`.
//!
//! The handle returned by [`start_tail`] cancels the background task on
//! drop or via the explicit `cancel` channel.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use notify::Watcher;
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

use crate::agents::{EventSink, RawCost, TranscriptEvent};
use crate::hooks::events::HookEvent;

/// Boxed line-parser used by [`start_tail`]. Aliased so call sites stay
/// readable (avoids `Arc<dyn Fn(&str) -> Option<TranscriptEvent> + Send + Sync>`
/// repeated across signatures).
pub type LineParser = Arc<dyn Fn(&str) -> Option<TranscriptEvent> + Send + Sync>;

/// Handle to a running tail task. Drop to cancel; or call [`Self::cancel`]
/// to be explicit and await termination.
pub struct JsonlTailHandle {
    cancel: Option<oneshot::Sender<()>>,
    join: Option<JoinHandle<()>>,
}

impl JsonlTailHandle {
    pub async fn cancel(mut self) {
        if let Some(tx) = self.cancel.take() {
            let _ = tx.send(());
        }
        if let Some(h) = self.join.take() {
            let _ = tokio::time::timeout(std::time::Duration::from_secs(2), h).await;
        }
    }
}

impl Drop for JsonlTailHandle {
    fn drop(&mut self) {
        if let Some(tx) = self.cancel.take() {
            let _ = tx.send(());
        }
        if let Some(h) = self.join.take() {
            h.abort();
        }
    }
}

/// Start a background tail of `path`. Each line that parses through
/// `parse_line` (typically [`super::transcript::parse_transcript_line`]) is
/// translated into a HookEvent the existing dispatcher understands. `cwd` is
/// the session's working directory, used to resolve relative file paths Pi
/// emits in tool arguments (e.g. `./foo.js`) to absolute paths so the
/// Modified Files panel can dedupe against the absolute paths reported by
/// `git status`.
pub fn start_tail(
    path: PathBuf,
    cwd: PathBuf,
    cluihud_session_id: String,
    sink: EventSink,
    parse_line: LineParser,
) -> JsonlTailHandle {
    let (cancel_tx, mut cancel_rx) = oneshot::channel::<()>();

    let join = tokio::spawn(async move {
        let mut file = match tokio::fs::OpenOptions::new().read(true).open(&path).await {
            Ok(f) => f,
            Err(e) => {
                tracing::error!(path = %path.display(), error = %e, "pi jsonl open failed");
                return;
            }
        };

        let mut last_offset: u64 = 0;
        last_offset = read_appended(
            &mut file,
            last_offset,
            &parse_line,
            &sink,
            &cwd,
            &cluihud_session_id,
        )
        .await;

        // notify the channel on file modifications
        let (notify_tx, mut notify_rx) = tokio::sync::mpsc::unbounded_channel::<()>();
        let watcher_path = path.clone();
        let mut watcher =
            match notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
                if let Ok(ev) = res
                    && matches!(ev.kind, notify::EventKind::Modify(_))
                {
                    let _ = notify_tx.send(());
                }
            }) {
                Ok(w) => w,
                Err(e) => {
                    tracing::error!(error = %e, "notify watcher creation failed");
                    return;
                }
            };
        if let Err(e) = watcher.watch(&watcher_path, notify::RecursiveMode::NonRecursive) {
            tracing::error!(error = %e, path = %watcher_path.display(), "watcher.watch failed");
            return;
        }

        loop {
            tokio::select! {
                _ = &mut cancel_rx => break,
                Some(_) = notify_rx.recv() => {
                    last_offset = read_appended(&mut file, last_offset, &parse_line, &sink, &cwd, &cluihud_session_id).await;
                }
            }
        }
    });

    JsonlTailHandle {
        cancel: Some(cancel_tx),
        join: Some(join),
    }
}

/// Seek to `offset`, drain to EOF, parse each newline-delimited line through
/// `parse_line`, forward to `sink`. Returns the new offset.
async fn read_appended(
    file: &mut tokio::fs::File,
    offset: u64,
    parse_line: &LineParser,
    sink: &EventSink,
    cwd: &Path,
    cluihud_session_id: &str,
) -> u64 {
    if file.seek(std::io::SeekFrom::Start(offset)).await.is_err() {
        return offset;
    }
    let mut buf = String::new();
    let n = file.read_to_string(&mut buf).await.unwrap_or(0);
    let new_offset = offset + n as u64;
    for line in buf.lines() {
        if line.trim().is_empty() {
            continue;
        }
        if let Some(ev) = parse_line(line)
            && let Some(hook_event) = wrap(cluihud_session_id, cwd, ev)
            && sink.send(hook_event).is_err()
        {
            return new_offset;
        }
    }
    new_offset
}

/// Wrap a [`TranscriptEvent`] into the [`HookEvent`] shape the runtime's
/// dispatcher already understands, so adapters reuse the existing emit path.
/// `Cost` events are folded into `Stop` since the existing dispatcher
/// emits cost on Stop — for the foundation this means Pi token totals
/// surface at session end. A future change can introduce an explicit
/// per-event cost emission.
///
/// `cwd` resolves relative file paths in tool arguments to absolute paths
/// before they reach the dispatcher. Pi writes paths like `./foo.js` while
/// `git status` reports absolute paths; without this normalization the
/// Modified Files panel would render two rows per file (one from each
/// source) since the frontend dedupes by exact path string.
fn wrap(session_id: &str, cwd: &Path, ev: TranscriptEvent) -> Option<HookEvent> {
    match ev {
        // Pi only logs the tool *call* in its JSONL — the matching tool_result
        // doesn't carry name/input. To get downstream consumers (file diff,
        // task router) the data they need, surface ToolUse as PostToolUse with
        // name+input populated. The companion ToolResult is dropped: nothing
        // currently consumes the raw output value.
        TranscriptEvent::ToolUse { name, mut input } => {
            absolutize_path_field(&mut input, cwd);
            Some(HookEvent::PostToolUse {
                session_id: session_id.into(),
                tool_name: name,
                tool_input: input,
                tool_result: None,
            })
        }
        TranscriptEvent::ToolResult { .. } => None,
        TranscriptEvent::Cost(raw) => {
            // Surface only model identity. Token totals are intentionally not
            // forwarded — the status bar doesn't show cost.
            let model = raw.model_id?;
            Some(HookEvent::AgentStatus {
                session_id: session_id.into(),
                agent_id: Some("pi".into()),
                model_id: Some(model.clone()),
                model_name: Some(model),
                session_started_at: None,
                context_used_pct: None,
                context_window_size: None,
                rate_5h_pct: None,
                rate_5h_resets_at: None,
                rate_7d_pct: None,
                rate_7d_resets_at: None,
                effort_level: None,
            })
        }
        TranscriptEvent::Message { .. } | TranscriptEvent::Other(_) => None,
    }
}

// Suppress unused warning when Cost is dropped; RawCost itself is referenced
// elsewhere in the module so the import stays in scope.
#[allow(dead_code)]
fn _force_raw_cost_link(_r: RawCost) {}

/// Rewrite `tool_input["path"|"file_path"|"filePath"]` from a relative path
/// (e.g. `./foo.js`, `src/lib.rs`) into an absolute path under `cwd`. Leaves
/// already-absolute paths untouched and silently no-ops when the field is
/// absent or non-string.
fn absolutize_path_field(input: &mut serde_json::Value, cwd: &Path) {
    let Some(obj) = input.as_object_mut() else {
        return;
    };
    for key in ["path", "file_path", "filePath"] {
        if let Some(slot) = obj.get_mut(key)
            && let Some(raw) = slot.as_str()
            && !raw.is_empty()
            && !raw.starts_with('/')
        {
            let stripped = raw.strip_prefix("./").unwrap_or(raw);
            let joined = cwd.join(stripped);
            // Prefer dunce::canonicalize (resolves `..`/symlinks), fall back
            // to the raw join when the file is missing — `git status` paths
            // come out canonicalized so they'll match either way for files
            // that exist on disk.
            let abs = dunce::canonicalize(&joined)
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_else(|_| joined.to_string_lossy().into_owned());
            *slot = serde_json::Value::String(abs);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn absolutize_rewrites_relative_dot_path_under_cwd() {
        let mut input = json!({"path": "./foo.js"});
        absolutize_path_field(&mut input, Path::new("/home/x/proj"));
        let s = input.get("path").unwrap().as_str().unwrap();
        assert!(s.ends_with("/foo.js"), "got {s}");
        assert!(s.starts_with('/'), "got {s}");
    }

    #[test]
    fn absolutize_leaves_already_absolute_path_untouched() {
        let mut input = json!({"path": "/already/absolute/foo.js"});
        absolutize_path_field(&mut input, Path::new("/home/x/proj"));
        assert_eq!(
            input.get("path").unwrap().as_str(),
            Some("/already/absolute/foo.js")
        );
    }

    #[test]
    fn absolutize_handles_both_snake_and_camel_path_keys() {
        let mut input = json!({"file_path": "src/a.rs", "filePath": "src/b.rs"});
        absolutize_path_field(&mut input, Path::new("/proj"));
        let a = input.get("file_path").unwrap().as_str().unwrap();
        let b = input.get("filePath").unwrap().as_str().unwrap();
        assert!(a.ends_with("/src/a.rs"));
        assert!(b.ends_with("/src/b.rs"));
    }

    #[test]
    fn absolutize_noop_when_input_is_not_object() {
        let mut input = json!("not an object");
        absolutize_path_field(&mut input, Path::new("/proj"));
        assert_eq!(input, json!("not an object"));
    }

    #[test]
    fn absolutize_noop_when_path_field_missing() {
        let mut input = json!({"other": "value"});
        absolutize_path_field(&mut input, Path::new("/proj"));
        assert_eq!(input, json!({"other": "value"}));
    }
}
