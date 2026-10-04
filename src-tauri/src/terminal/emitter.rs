use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use tauri::async_runtime::{self, JoinHandle};
use tauri::ipc::Channel;
use tokio::sync::Notify;

use super::differ::GridDiffer;
use super::session::TerminalSession;
use super::types::GridUpdate;

/// Bundle of per-session state that together drives the grid-update stream
/// to the frontend.
///
/// Ownership model:
/// - [`TerminalHandle::session`] is the VT emulator. The PTY reader thread
///   locks it to feed bytes; the emitter task locks it to take a snapshot.
/// - [`TerminalHandle::differ`] tracks what was last emitted so we can send
///   deltas. Touched only by the emitter task and by `full_grid` helpers.
/// - [`TerminalHandle::notify`] is the "something changed" signal fired by
///   the reader thread after each chunk.
/// - [`TerminalHandle::shutdown`] tells the emitter task to exit; set on
///   session drop.
/// - [`TerminalHandle::subscriber`] is the frontend renderer's channel. A
///   per-terminal channel instead of an app-wide event: broadcast emits were
///   eval'd as a fresh script per update in every webview listener, and over
///   hours of streaming that GC-thrashed the webview into a freeze (BUG-37).
///
/// The mutexes are `std::sync::Mutex` rather than `tokio::sync::Mutex`
/// because they are locked briefly across both sync (reader thread) and
/// async (emitter task) contexts, and are never held across `.await`.
pub struct TerminalHandle {
    pub session: Arc<Mutex<TerminalSession>>,
    pub differ: Arc<Mutex<GridDiffer>>,
    pub notify: Arc<Notify>,
    pub shutdown: Arc<AtomicBool>,
    subscriber: Arc<Mutex<Option<Channel<GridUpdate>>>>,
    task: Option<JoinHandle<()>>,
}

impl TerminalHandle {
    pub fn new(session: TerminalSession) -> Self {
        Self {
            session: Arc::new(Mutex::new(session)),
            differ: Arc::new(Mutex::new(GridDiffer::new())),
            notify: Arc::new(Notify::new()),
            shutdown: Arc::new(AtomicBool::new(false)),
            subscriber: Arc::new(Mutex::new(None)),
            task: None,
        }
    }

    /// Spawn the coalescing emitter. Safe to call once per handle.
    pub fn spawn_emitter(&mut self, session_id: String) {
        if self.task.is_some() {
            return;
        }
        let session = Arc::clone(&self.session);
        let differ = Arc::clone(&self.differ);
        let notify = Arc::clone(&self.notify);
        let shutdown = Arc::clone(&self.shutdown);
        let subscriber = Arc::clone(&self.subscriber);
        self.task = Some(async_runtime::spawn(async move {
            run_emitter(session_id, session, differ, notify, shutdown, subscriber).await;
        }));
    }

    pub fn attach(&self, channel: Channel<GridUpdate>) {
        if let Ok(mut slot) = self.subscriber.lock() {
            *slot = Some(channel);
        }
        self.wake();
    }

    /// Nudge the emitter — the reader thread calls this after `advance_bytes`.
    pub fn wake(&self) {
        self.notify.notify_one();
    }
}

impl Drop for TerminalHandle {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Release);
        self.notify.notify_one();
        if let Some(task) = self.task.take() {
            task.abort();
        }
    }
}

async fn run_emitter(
    session_id: String,
    session: Arc<Mutex<TerminalSession>>,
    differ: Arc<Mutex<GridDiffer>>,
    notify: Arc<Notify>,
    shutdown: Arc<AtomicBool>,
    subscriber: Arc<Mutex<Option<Channel<GridUpdate>>>>,
) {
    // 8ms ≈ 120Hz. Generous enough to coalesce bursts without adding
    // perceptible latency; see Decision 3 in the change design.
    let coalesce = Duration::from_millis(8);

    loop {
        notify.notified().await;
        if shutdown.load(Ordering::Acquire) {
            break;
        }

        // Coalesce burst: after the first signal, wait `coalesce` ms while
        // silently absorbing any further signals that arrive in that window.
        tokio::time::sleep(coalesce).await;
        if shutdown.load(Ordering::Acquire) {
            break;
        }

        // Unattached: skip the diff entirely; `terminal_attach` sends a full
        // grid when a renderer arrives.
        let Some(channel) = subscriber.lock().ok().and_then(|slot| slot.clone()) else {
            continue;
        };

        let snapshot = match session.lock() {
            Ok(mut guard) => guard.grid_snapshot(),
            Err(err) => {
                tracing::error!(error = %err, "terminal session mutex poisoned");
                break;
            }
        };

        let update = match differ.lock() {
            Ok(mut guard) => guard.compute_update(&session_id, &snapshot),
            Err(err) => {
                tracing::error!(error = %err, "grid differ mutex poisoned");
                break;
            }
        };

        if let Some(update) = update
            && let Err(err) = channel.send(update)
        {
            tracing::warn!(error = %err, session_id = %session_id, "failed to send grid update");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use tauri::ipc::{Channel, InvokeResponseBody};

    #[test]
    fn attached_channel_receives_session_updates() {
        let mut handle =
            TerminalHandle::new(TerminalSession::new(20, 5, Box::new(std::io::sink())));
        handle.spawn_emitter("s1".to_string());
        let (tx, rx) = mpsc::channel();
        handle.attach(Channel::new(move |body| {
            let _ = tx.send(body);
            Ok(())
        }));

        handle.session.lock().unwrap().advance_bytes(b"hello");
        handle.wake();

        let body = rx
            .recv_timeout(Duration::from_secs(5))
            .expect("emitter never delivered to the attached channel");
        let InvokeResponseBody::Json(json) = body else {
            panic!("expected a JSON body");
        };
        assert!(json.contains("\"sessionId\":\"s1\""), "{json}");
    }
}
