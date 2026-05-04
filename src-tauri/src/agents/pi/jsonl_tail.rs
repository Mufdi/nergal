//! Tail-`-f` for Pi's session JSONL files.
//!
//! Reads the existing content on start (catch-up), then watches for changes
//! via `notify` and reads only the appended bytes. Each parsed line becomes
//! a [`TranscriptEvent`] which is wrapped in the appropriate [`HookEvent`]
//! shape and forwarded to the runtime's `EventSink`.
//!
//! The handle returned by [`start_tail`] cancels the background task on
//! drop or via the explicit `cancel` channel.

use std::path::PathBuf;
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
/// translated into a HookEvent the existing dispatcher understands.
pub fn start_tail(
    path: PathBuf,
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
                    last_offset = read_appended(&mut file, last_offset, &parse_line, &sink, &cluihud_session_id).await;
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
            && let Some(hook_event) = wrap(cluihud_session_id, ev)
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
fn wrap(session_id: &str, ev: TranscriptEvent) -> Option<HookEvent> {
    match ev {
        TranscriptEvent::ToolUse { name, input } => Some(HookEvent::PreToolUse {
            session_id: session_id.into(),
            tool_name: name,
            tool_input: input,
        }),
        TranscriptEvent::ToolResult { output, .. } => Some(HookEvent::PostToolUse {
            session_id: session_id.into(),
            tool_name: String::new(),
            tool_input: serde_json::Value::Null,
            tool_result: output
                .as_str()
                .map(String::from)
                .or_else(|| serde_json::to_string(&output).ok()),
        }),
        TranscriptEvent::Cost(_raw) => {
            // Pi cost surfacing is deferred to the pricing module — we drop
            // here rather than fabricate a Stop, which has its own semantics
            // (session end). A future TranscriptEvent → CostUpdate event in
            // the dispatcher will capture this directly.
            let _ = _raw;
            None
        }
        TranscriptEvent::Message { .. } | TranscriptEvent::Other(_) => None,
    }
}

// Suppress unused warning when Cost is dropped; RawCost itself is referenced
// elsewhere in the module so the import stays in scope.
#[allow(dead_code)]
fn _force_raw_cost_link(_r: RawCost) {}
