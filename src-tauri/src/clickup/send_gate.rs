//! Mid-stream send-gate (clickup-task-integration, design Revision 1).
//!
//! One mutex over `{run_state, queued, guard_verified}` — two separate locks
//! had a lost-wakeup race between check-Running/enqueue and set-Idle/drain.
//! Keyed STRICTLY by `cluihud_session_id`; events without it are skipped so
//! external CC sessions never create ghost entries. The guard is best-effort
//! (signal latency, event ordering); what it guarantees: a queued send is
//! never silently lost (every outcome emits an event), never double-delivered
//! (destructive pop under one lock), and stays user-actionable (cancel /
//! deliver-now).

use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, PoisonError};

use tauri::{AppHandle, Emitter, Manager};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum RunState {
    Idle,
    Running,
}

/// A composed send waiting for the target session to go idle. One slot per
/// session, replace semantics (a one-shot imperative prompt: stale multi-
/// delivery into a single turn would be worse than replacing).
#[derive(Clone, Debug)]
pub struct QueuedSend {
    pub task_id: String,
    pub text: String,
}

#[derive(Default)]
struct SendGate {
    /// Absent = Idle = deliver immediately (today's exact behavior for agents
    /// without a Running edge).
    run_state: HashMap<String, RunState>,
    queued: HashMap<String, QueuedSend>,
    /// Sessions whose Running edge was OBSERVED at runtime — the only honest
    /// basis for the confirm dialog's `guard_active` flag.
    guard_verified: HashSet<String>,
}

pub enum EnqueueOutcome {
    /// Gate read Idle: caller delivers (paste + submit) outside the lock.
    DeliverNow,
    /// Gate read Running: stored for the Stop drain.
    Queued { replaced: bool },
}

/// Tauri-managed wrapper. All methods recover from a poisoned lock via
/// `into_inner`: the critical sections are plain map ops that cannot leave
/// the struct half-mutated, and a wedged gate must never take hook
/// processing down with it.
#[derive(Default)]
pub struct SendGateState {
    inner: Mutex<SendGate>,
}

impl SendGateState {
    fn lock(&self) -> std::sync::MutexGuard<'_, SendGate> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Dispatcher writer: `UserPromptSubmit` → Running + guard observed.
    /// `None` (no `cluihud_session_id`) is skipped — strict keying.
    pub fn note_user_prompt(&self, csid: Option<&str>) {
        let Some(csid) = csid else { return };
        let mut gate = self.lock();
        gate.run_state.insert(csid.to_string(), RunState::Running);
        gate.guard_verified.insert(csid.to_string());
    }

    /// Dispatcher writer: `Stop` → Idle + destructive pop of the queued send.
    /// Delivery happens at the caller, OUTSIDE this lock.
    pub fn note_stop(&self, csid: Option<&str>) -> Option<QueuedSend> {
        let csid = csid?;
        let mut gate = self.lock();
        gate.run_state.insert(csid.to_string(), RunState::Idle);
        gate.queued.remove(csid)
    }

    /// Check-and-enqueue, atomic under the gate lock: Running ⇒ store
    /// (replacing any previous slot), Idle ⇒ tell the caller to deliver.
    pub fn check_and_enqueue(&self, csid: &str, send: QueuedSend) -> EnqueueOutcome {
        let mut gate = self.lock();
        if gate.run_state.get(csid) == Some(&RunState::Running) {
            let replaced = gate.queued.insert(csid.to_string(), send).is_some();
            EnqueueOutcome::Queued { replaced }
        } else {
            EnqueueOutcome::DeliverNow
        }
    }

    /// Destructive pop for cancel / force-deliver. At most one popper wins
    /// against a concurrent Stop drain.
    pub fn pop_queued(&self, csid: &str) -> Option<QueuedSend> {
        self.lock().queued.remove(csid)
    }

    /// Teardown purge: clears all three sub-maps for the session. Returns the
    /// dropped queued send (if any) so the caller emits `clickup:send-dropped`
    /// only when an entry was actually removed — double-purge is a silent
    /// no-op.
    pub fn purge(&self, csid: &str) -> Option<QueuedSend> {
        let mut gate = self.lock();
        gate.run_state.remove(csid);
        gate.guard_verified.remove(csid);
        gate.queued.remove(csid)
    }

    /// True only when the session's Running edge was observed at runtime.
    pub fn guard_active(&self, csid: &str) -> bool {
        self.lock().guard_verified.contains(csid)
    }
}

// ── Dispatcher-facing helpers (resolve managed state off the AppHandle) ──

#[derive(Clone, serde::Serialize)]
struct SendQueuedPayload {
    session_id: String,
    task_id: String,
    replaced: bool,
}

#[derive(Clone, serde::Serialize)]
struct SendDeliveredPayload {
    session_id: String,
    task_id: String,
    /// "immediate" (gate Idle: pasted + submitted) | "deferred" (Stop drain:
    /// pasted WITHOUT submit) | "forced" (deliver-now: pasted WITHOUT submit).
    mode: String,
}

#[derive(Clone, serde::Serialize)]
struct SendDroppedPayload {
    session_id: String,
    task_id: Option<String>,
    reason: String,
}

pub(crate) fn emit_send_queued(app: &AppHandle, csid: &str, task_id: &str, replaced: bool) {
    if replaced {
        tracing::info!(session = %csid, task = %task_id, "queued clickup send replaced a previous one");
    }
    let _ = app.emit(
        "clickup:send-queued",
        SendQueuedPayload {
            session_id: csid.to_string(),
            task_id: task_id.to_string(),
            replaced,
        },
    );
}

pub(crate) fn emit_send_delivered(app: &AppHandle, csid: &str, task_id: &str, mode: &str) {
    let _ = app.emit(
        "clickup:send-delivered",
        SendDeliveredPayload {
            session_id: csid.to_string(),
            task_id: task_id.to_string(),
            mode: mode.to_string(),
        },
    );
}

pub(crate) fn emit_send_dropped(app: &AppHandle, csid: &str, task_id: Option<&str>, reason: &str) {
    tracing::warn!(session = %csid, task = ?task_id, reason = %reason, "clickup send dropped");
    let _ = app.emit(
        "clickup:send-dropped",
        SendDroppedPayload {
            session_id: csid.to_string(),
            task_id: task_id.map(str::to_string),
            reason: reason.to_string(),
        },
    );
}

/// `UserPromptSubmit` dispatcher arm.
pub(crate) fn note_user_prompt(app: &AppHandle, csid: Option<&str>) {
    if let Some(state) = app.try_state::<SendGateState>() {
        state.note_user_prompt(csid);
    }
}

/// `Stop` dispatcher arms: mark Idle, pop the queued send under the lock,
/// deliver outside it — pasted WITHOUT auto-submit (the user may be
/// mid-draft; a deferred splice must be visible, never auto-submitted).
pub(crate) fn drain_on_stop(app: &AppHandle, csid: Option<&str>) {
    let Some(state) = app.try_state::<SendGateState>() else {
        return;
    };
    let Some(send) = state.note_stop(csid) else {
        return;
    };
    let Some(csid) = csid else { return };
    let Some(pty) = app.try_state::<crate::pty::PtyManager>() else {
        emit_send_dropped(app, csid, Some(&send.task_id), "pty manager unavailable");
        return;
    };
    match crate::pty::paste_to_session(&pty, csid, &send.text, false) {
        Ok(()) => emit_send_delivered(app, csid, &send.task_id, "deferred"),
        Err(e) => emit_send_dropped(
            app,
            csid,
            Some(&send.task_id),
            &format!("pty write failed: {e}"),
        ),
    }
}

/// Teardown purge (`kill_session_pty`, `SessionEnd` arm, PTY-reader EOF).
/// Emits `clickup:send-dropped` only when a queued send was actually removed.
pub(crate) fn purge_session(app: &AppHandle, csid: &str) {
    let Some(state) = app.try_state::<SendGateState>() else {
        return;
    };
    if let Some(send) = state.purge(csid) {
        emit_send_dropped(app, csid, Some(&send.task_id), "session closed");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn send(task: &str) -> QueuedSend {
        QueuedSend {
            task_id: task.to_string(),
            text: format!("body of {task}"),
        }
    }

    #[test]
    fn running_queues_and_idle_delivers() {
        let state = SendGateState::default();

        // No entry yet: absent = Idle = deliver immediately.
        assert!(matches!(
            state.check_and_enqueue("s1", send("t1")),
            EnqueueOutcome::DeliverNow
        ));
        assert!(
            state.pop_queued("s1").is_none(),
            "DeliverNow must not store"
        );

        // Observed Running: queue.
        state.note_user_prompt(Some("s1"));
        assert!(matches!(
            state.check_and_enqueue("s1", send("t1")),
            EnqueueOutcome::Queued { replaced: false }
        ));

        // Stop → Idle + pop; next send delivers immediately again.
        let drained = state.note_stop(Some("s1")).expect("queued send drained");
        assert_eq!(drained.task_id, "t1");
        assert!(matches!(
            state.check_and_enqueue("s1", send("t2")),
            EnqueueOutcome::DeliverNow
        ));
    }

    #[test]
    fn destructive_pop_has_a_single_winner() {
        let state = SendGateState::default();
        state.note_user_prompt(Some("s1"));
        assert!(matches!(
            state.check_and_enqueue("s1", send("t1")),
            EnqueueOutcome::Queued { .. }
        ));

        // A Stop drain and a force-deliver cannot both get the send.
        assert!(state.note_stop(Some("s1")).is_some());
        assert!(state.pop_queued("s1").is_none());
        assert!(state.note_stop(Some("s1")).is_none());
    }

    #[test]
    fn strict_keying_skips_none() {
        let state = SendGateState::default();
        state.note_user_prompt(None);
        assert!(state.note_stop(None).is_none());

        // No ghost entries were created.
        assert!(!state.guard_active(""));
        let gate = state.lock();
        assert!(gate.run_state.is_empty());
        assert!(gate.guard_verified.is_empty());
        assert!(gate.queued.is_empty());
    }

    #[test]
    fn purge_clears_all_three_submaps_and_is_idempotent() {
        let state = SendGateState::default();
        state.note_user_prompt(Some("s1"));
        assert!(matches!(
            state.check_and_enqueue("s1", send("t1")),
            EnqueueOutcome::Queued { .. }
        ));
        assert!(state.guard_active("s1"));

        let dropped = state.purge("s1");
        assert_eq!(dropped.map(|s| s.task_id), Some("t1".to_string()));
        assert!(!state.guard_active("s1"));
        assert!(state.pop_queued("s1").is_none());
        // run_state cleared too: absent = Idle.
        assert!(matches!(
            state.check_and_enqueue("s1", send("t2")),
            EnqueueOutcome::DeliverNow
        ));

        // Second purge (kill → late EOF) is a silent no-op: nothing to emit.
        assert!(state.purge("s1").is_none());
    }

    #[test]
    fn double_enqueue_replaces_the_slot() {
        let state = SendGateState::default();
        state.note_user_prompt(Some("s1"));
        assert!(matches!(
            state.check_and_enqueue("s1", send("t1")),
            EnqueueOutcome::Queued { replaced: false }
        ));
        assert!(matches!(
            state.check_and_enqueue("s1", send("t2")),
            EnqueueOutcome::Queued { replaced: true }
        ));
        let drained = state.note_stop(Some("s1")).expect("one slot survives");
        assert_eq!(drained.task_id, "t2", "replace keeps the latest send");
        assert!(state.pop_queued("s1").is_none(), "exactly one slot");
    }
}
