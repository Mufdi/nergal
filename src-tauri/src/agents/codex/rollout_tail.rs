//! Tail of Codex's rollout JSONL → status-bar telemetry.
//!
//! Codex writes `~/.codex/sessions/<yyyy>/<mm>/<dd>/rollout-<uuid>.jsonl` while
//! a session runs. Unlike the speculative shape the legacy transcript parser
//! assumed, the real records are `{timestamp, type, payload}` envelopes where
//! `type` is one of `session_meta | turn_context | event_msg | response_item`.
//! The status bar only needs two of those: `turn_context.payload.model`/`.effort`
//! (model name + effort cell) and `event_msg` with `payload.type ==
//! "token_count"` (`payload.info.total_token_usage.total_tokens` over
//! `model_context_window` → context-window percentage). Tool calls/activities
//! already arrive via Codex's shared hook socket, so this tail emits only
//! [`HookEvent::AgentStatus`].
//!
//! Rate limits ARE present (`token_count.rate_limits`) but Codex's window is a
//! single ~30-day budget — it doesn't map onto the status bar's CC-shaped
//! 5h/7d cells, so those stay absent rather than mislabel a monthly limit.

use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use notify::Watcher;
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

use super::rollout_resolver::{find_rollout_after_spawn, find_rollout_for_cwd};
use crate::agents::EventSink;
use crate::hooks::events::HookEvent;

/// Handle to a running rollout tail. Drop to cancel.
pub struct RolloutTailHandle {
    cancel: Option<oneshot::Sender<()>>,
    join: Option<JoinHandle<()>>,
}

impl RolloutTailHandle {
    pub async fn cancel(mut self) {
        if let Some(tx) = self.cancel.take() {
            let _ = tx.send(());
        }
        if let Some(h) = self.join.take() {
            let _ = tokio::time::timeout(std::time::Duration::from_secs(2), h).await;
        }
    }
}

impl Drop for RolloutTailHandle {
    fn drop(&mut self) {
        if let Some(tx) = self.cancel.take() {
            let _ = tx.send(());
        }
        if let Some(h) = self.join.take() {
            h.abort();
        }
    }
}

/// Accumulates fields across rollout records (model lands in `turn_context`,
/// tokens in `token_count`) and renders a full [`HookEvent::AgentStatus`].
struct StatusAcc {
    session_id: String,
    started_at: u64,
    model: Option<String>,
    effort: Option<String>,
    context_window: Option<u64>,
    total_tokens: Option<u64>,
}

impl StatusAcc {
    fn new(session_id: String, started_at: u64) -> Self {
        Self {
            session_id,
            started_at,
            model: None,
            effort: None,
            context_window: None,
            total_tokens: None,
        }
    }

    /// Fold one rollout line into the accumulator. Returns an updated status
    /// event when the line carried something the status bar renders.
    fn ingest(&mut self, line: &str) -> Option<HookEvent> {
        let v: serde_json::Value = serde_json::from_str(line).ok()?;
        let payload = v.get("payload")?;
        match v.get("type").and_then(|t| t.as_str())? {
            "turn_context" => {
                if let Some(m) = payload.get("model").and_then(|m| m.as_str()) {
                    self.model = Some(m.to_string());
                }
                // `effort` is often null on Codex; only overwrite with a real value.
                if let Some(e) = payload.get("effort").and_then(|e| e.as_str()) {
                    self.effort = Some(e.to_string());
                }
                Some(self.status())
            }
            "event_msg" if payload.get("type").and_then(|t| t.as_str()) == Some("token_count") => {
                let info = payload.get("info")?;
                if let Some(w) = info.get("model_context_window").and_then(|w| w.as_u64()) {
                    self.context_window = Some(w);
                }
                if let Some(t) = info
                    .get("total_token_usage")
                    .and_then(|u| u.get("total_tokens"))
                    .and_then(|t| t.as_u64())
                {
                    self.total_tokens = Some(t);
                }
                Some(self.status())
            }
            _ => None,
        }
    }

    fn status(&self) -> HookEvent {
        let context_used_pct = match (self.total_tokens, self.context_window) {
            (Some(t), Some(w)) if w > 0 => Some((t as f64 / w as f64) * 100.0),
            _ => None,
        };
        HookEvent::AgentStatus {
            session_id: self.session_id.clone(),
            agent_id: Some("codex".into()),
            model_id: self.model.clone(),
            model_name: self.model.clone(),
            session_started_at: Some(self.started_at),
            context_used_pct,
            context_window_size: self.context_window,
            rate_5h_pct: None,
            rate_5h_resets_at: None,
            rate_7d_pct: None,
            rate_7d_resets_at: None,
            effort_level: self.effort.clone(),
        }
    }
}

/// Start a background tail of this session's rollout, emitting `AgentStatus`
/// events into `sink`. Resolution happens inside the task (the rollout appears
/// a moment after spawn), so the caller never blocks. `started_at` (epoch secs,
/// captured at spawn) seeds the duration cell and bounds the resolver scan.
pub fn start_rollout_tail(
    sessions_root: PathBuf,
    cwd: Option<PathBuf>,
    session_id: String,
    started_at: u64,
    sink: EventSink,
) -> RolloutTailHandle {
    let (cancel_tx, mut cancel_rx) = oneshot::channel::<()>();

    let join = tokio::spawn(async move {
        // Prefer matching the rollout by its session_meta.cwd (robust against
        // other live/resumed codex sessions); fall back to newest-after-spawn
        // when the cwd is unknown.
        let resolve = async {
            match cwd.as_deref().and_then(|p| p.to_str()) {
                Some(cwd_str) => {
                    find_rollout_for_cwd(&sessions_root, cwd_str, Duration::from_secs(20)).await
                }
                None => {
                    let after = SystemTime::now()
                        .checked_sub(Duration::from_secs(10))
                        .unwrap_or(SystemTime::UNIX_EPOCH);
                    find_rollout_after_spawn(&sessions_root, after, Duration::from_secs(20)).await
                }
            }
        };
        let path = tokio::select! {
            _ = &mut cancel_rx => return,
            r = resolve => match r {
                Ok(p) => {
                    tracing::info!(path = %p.display(), "codex rollout tail attached");
                    p
                }
                Err(e) => {
                    tracing::warn!(error = %e, "codex rollout not found; status bar telemetry unavailable");
                    return;
                }
            },
        };
        let mut file = match tokio::fs::OpenOptions::new().read(true).open(&path).await {
            Ok(f) => f,
            Err(e) => {
                tracing::error!(path = %path.display(), error = %e, "codex rollout open failed");
                return;
            }
        };

        let mut acc = StatusAcc::new(session_id, started_at);
        let mut offset: u64 = 0;
        offset = read_appended(&mut file, offset, &mut acc, &sink).await;

        let (notify_tx, mut notify_rx) = tokio::sync::mpsc::unbounded_channel::<()>();
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
                    tracing::error!(error = %e, "codex rollout watcher creation failed");
                    return;
                }
            };
        if let Err(e) = watcher.watch(&path, notify::RecursiveMode::NonRecursive) {
            tracing::error!(error = %e, path = %path.display(), "codex rollout watch failed");
            return;
        }

        loop {
            tokio::select! {
                _ = &mut cancel_rx => break,
                Some(_) = notify_rx.recv() => {
                    offset = read_appended(&mut file, offset, &mut acc, &sink).await;
                }
            }
        }
    });

    RolloutTailHandle {
        cancel: Some(cancel_tx),
        join: Some(join),
    }
}

/// Seek to `offset`, drain to EOF, fold each line into `acc`, forward any
/// emitted status to `sink`. Returns the new offset.
///
/// The offset advances only past the last `\n` in the read: a modify event
/// can fire mid-write, so a trailing partial line is left unconsumed and
/// re-read whole on the next event, instead of being dropped (and corrupting
/// the following read, since the offset would otherwise already be past its
/// bytes).
async fn read_appended(
    file: &mut tokio::fs::File,
    offset: u64,
    acc: &mut StatusAcc,
    sink: &EventSink,
) -> u64 {
    if file.seek(std::io::SeekFrom::Start(offset)).await.is_err() {
        return offset;
    }
    let mut buf = Vec::new();
    if file.read_to_end(&mut buf).await.is_err() {
        return offset;
    }
    let Some(last_newline) = buf.iter().rposition(|&b| b == b'\n') else {
        return offset;
    };
    let complete = &buf[..=last_newline];
    let new_offset = offset + complete.len() as u64;
    for line in complete.split(|&b| b == b'\n') {
        let Ok(line) = std::str::from_utf8(line) else {
            tracing::warn!("codex rollout line was not valid utf-8; skipping");
            continue;
        };
        if line.trim().is_empty() {
            continue;
        }
        if let Some(ev) = acc.ingest(line)
            && sink.send(ev).is_err()
        {
            return new_offset;
        }
    }
    new_offset
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncWriteExt;

    fn acc() -> StatusAcc {
        StatusAcc::new("sess-1".into(), 1000)
    }

    async fn append(path: &std::path::Path, bytes: &[u8]) {
        let mut f = tokio::fs::OpenOptions::new()
            .append(true)
            .open(path)
            .await
            .unwrap();
        f.write_all(bytes).await.unwrap();
        f.flush().await.unwrap();
    }

    #[tokio::test]
    async fn half_line_holds_offset_until_completed() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let path = tmp.path().to_path_buf();
        tokio::fs::write(
            &path,
            b"{\"type\":\"turn_context\",\"payload\":{\"model\":\"gpt-5.5\"}}\n",
        )
        .await
        .unwrap();

        let mut file = tokio::fs::OpenOptions::new()
            .read(true)
            .open(&path)
            .await
            .unwrap();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let mut a = acc();

        let offset = read_appended(&mut file, 0, &mut a, &tx).await;
        assert!(rx.try_recv().is_ok(), "the whole first line must emit");
        assert!(rx.try_recv().is_err());

        append(
            &path,
            br#"{"type":"turn_context","payload":{"model":"gpt-6"#,
        )
        .await;
        let offset_after_partial = read_appended(&mut file, offset, &mut a, &tx).await;
        assert_eq!(
            offset_after_partial, offset,
            "a partial line must not advance the offset"
        );
        assert!(
            rx.try_recv().is_err(),
            "a partial line must not emit a record"
        );

        append(&path, b"\"}}\n").await;
        let offset_after_complete =
            read_appended(&mut file, offset_after_partial, &mut a, &tx).await;
        assert!(offset_after_complete > offset_after_partial);
        match rx
            .try_recv()
            .expect("completed line must emit exactly once")
        {
            HookEvent::AgentStatus { model_name, .. } => {
                assert_eq!(model_name.as_deref(), Some("gpt-6"));
            }
            other => panic!("expected AgentStatus, got {other:?}"),
        }
        assert!(rx.try_recv().is_err(), "must emit exactly one record");
    }

    #[tokio::test]
    async fn multi_byte_utf8_char_split_across_read_boundary_is_not_lost() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let path = tmp.path().to_path_buf();
        tokio::fs::write(&path, b"").await.unwrap();

        let mut file = tokio::fs::OpenOptions::new()
            .read(true)
            .open(&path)
            .await
            .unwrap();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let mut a = acc();

        // `😀` (U+1F600) is 4 bytes in UTF-8; split the append mid-character.
        let emoji = "😀".as_bytes();
        assert_eq!(emoji.len(), 4);
        let mut line = br#"{"type":"turn_context","payload":{"model":""#.to_vec();
        line.extend_from_slice(emoji);
        line.extend_from_slice(br#""}}"#);
        line.push(b'\n');

        let split_at = line.len() - 3; // lands inside the 4-byte emoji sequence
        append(&path, &line[..split_at]).await;
        let offset = read_appended(&mut file, 0, &mut a, &tx).await;
        assert_eq!(offset, 0, "no newline yet; nothing should be consumed");
        assert!(rx.try_recv().is_err());

        append(&path, &line[split_at..]).await;
        let offset = read_appended(&mut file, offset, &mut a, &tx).await;
        assert_eq!(offset as usize, line.len());
        match rx
            .try_recv()
            .expect("completed line must emit exactly once")
        {
            HookEvent::AgentStatus { model_name, .. } => {
                assert!(model_name.unwrap().contains('😀'));
            }
            other => panic!("expected AgentStatus, got {other:?}"),
        }
        assert!(rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn whole_lines_appended_at_once_emit_one_record_each() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let path = tmp.path().to_path_buf();
        tokio::fs::write(
            &path,
            b"{\"type\":\"turn_context\",\"payload\":{\"model\":\"a\"}}\n\
              {\"type\":\"turn_context\",\"payload\":{\"model\":\"b\"}}\n\
              {\"type\":\"turn_context\",\"payload\":{\"model\":\"c\"}}\n",
        )
        .await
        .unwrap();

        let mut file = tokio::fs::OpenOptions::new()
            .read(true)
            .open(&path)
            .await
            .unwrap();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let mut a = acc();

        let offset = read_appended(&mut file, 0, &mut a, &tx).await;
        let eof = tokio::fs::metadata(&path).await.unwrap().len();
        assert_eq!(offset, eof, "offset must land exactly at EOF");

        let mut models = Vec::new();
        while let Ok(HookEvent::AgentStatus { model_name, .. }) = rx.try_recv() {
            models.push(model_name.unwrap());
        }
        assert_eq!(models, vec!["a", "b", "c"]);
    }

    #[test]
    fn turn_context_sets_model_and_effort() {
        let mut a = acc();
        let line = r#"{"type":"turn_context","payload":{"model":"gpt-5.5","effort":"high"}}"#;
        match a.ingest(line).unwrap() {
            HookEvent::AgentStatus {
                model_name,
                effort_level,
                session_started_at,
                ..
            } => {
                assert_eq!(model_name.as_deref(), Some("gpt-5.5"));
                assert_eq!(effort_level.as_deref(), Some("high"));
                assert_eq!(session_started_at, Some(1000));
            }
            other => panic!("expected AgentStatus, got {other:?}"),
        }
    }

    #[test]
    fn token_count_computes_context_pct() {
        let mut a = acc();
        let line = r#"{"type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"total_tokens":25840},"model_context_window":258400}}}"#;
        match a.ingest(line).unwrap() {
            HookEvent::AgentStatus {
                context_used_pct,
                context_window_size,
                ..
            } => {
                assert_eq!(context_window_size, Some(258400));
                assert_eq!(context_used_pct, Some(10.0));
            }
            other => panic!("expected AgentStatus, got {other:?}"),
        }
    }

    #[test]
    fn model_persists_across_lines() {
        let mut a = acc();
        a.ingest(r#"{"type":"turn_context","payload":{"model":"gpt-5.5"}}"#);
        let ev = a
            .ingest(r#"{"type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"total_tokens":10},"model_context_window":100}}}"#)
            .unwrap();
        match ev {
            HookEvent::AgentStatus {
                model_name,
                context_used_pct,
                ..
            } => {
                assert_eq!(model_name.as_deref(), Some("gpt-5.5"));
                assert_eq!(context_used_pct, Some(10.0));
            }
            other => panic!("expected AgentStatus, got {other:?}"),
        }
    }

    #[test]
    fn unrelated_records_yield_nothing() {
        let mut a = acc();
        assert!(
            a.ingest(r#"{"type":"session_meta","payload":{"id":"x"}}"#)
                .is_none()
        );
        assert!(
            a.ingest(r#"{"type":"response_item","payload":{}}"#)
                .is_none()
        );
        assert!(a.ingest("garbage").is_none());
    }
}
