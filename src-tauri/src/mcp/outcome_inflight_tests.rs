use super::*;
use crate::mcp::delivery::{EmitHandle, SessionDelivery, SettleCallback, is_in_flight};
use std::sync::Mutex;

/// Records the in-flight mark at the moment of the paste: MockDelivery cannot block mid-paste,
/// so the wake itself is where a concurrent drain would race.
struct ProbeDelivery {
    in_flight_at_paste: Mutex<Option<bool>>,
    fail_paste: bool,
}

impl ProbeDelivery {
    fn new(fail_paste: bool) -> Self {
        Self {
            in_flight_at_paste: Mutex::new(None),
            fail_paste,
        }
    }
}

impl SessionDelivery for ProbeDelivery {
    fn wake_idle(
        &self,
        session_id: &str,
        _note: &str,
        _on_settled: SettleCallback,
    ) -> anyhow::Result<()> {
        *self.in_flight_at_paste.lock().unwrap() = Some(is_in_flight(session_id));
        if self.fail_paste {
            anyhow::bail!("probe paste failure");
        }
        Ok(())
    }

    fn emit(&self, _event: &str, _payload: serde_json::Value) {}

    fn emit_handle(&self) -> EmitHandle {
        Box::new(|_, _| {})
    }
}

#[test]
fn the_outcome_paste_holds_the_in_flight_mark_so_a_cross_session_drain_waits() {
    let gate = WorktreeGateState::default();
    gate.enqueue_outcome("inflight-held", "outcome note".to_string());
    let probe = ProbeDelivery::new(false);

    drain_worktree_outcomes(&gate, &probe, "inflight-held");

    assert_eq!(*probe.in_flight_at_paste.lock().unwrap(), Some(true));
}

#[test]
fn a_failed_outcome_paste_requeues_the_note_and_releases_the_mark() {
    let gate = WorktreeGateState::default();
    gate.enqueue_outcome("inflight-fail", "outcome note".to_string());
    let probe = ProbeDelivery::new(true);

    drain_worktree_outcomes(&gate, &probe, "inflight-fail");

    assert_eq!(*probe.in_flight_at_paste.lock().unwrap(), Some(true));
    assert!(!is_in_flight("inflight-fail"));
    assert_eq!(gate.take_outcomes("inflight-fail").len(), 1);
}

#[test]
fn an_outcome_drain_while_a_cross_session_wake_is_in_flight_pastes_nothing_and_keeps_the_note() {
    let sid = "inflight-busy";
    let db: crate::db::SharedDb =
        std::sync::Arc::new(Mutex::new(crate::db::Database::open_in_memory().unwrap()));
    {
        let g = db.lock().unwrap();
        g.insert_cross_session_thread(&crate::db::CrossSessionThread {
            id: "t1".into(),
            originator_session: "from".into(),
            participants: vec!["from".into(), sid.into()],
            status: "active".into(),
            max_hops: 4,
            msg_count: 1,
            msg_budget: Some(30),
            deadline_at: Some(u64::MAX >> 1),
            created_at: 0,
        })
        .unwrap();
        g.insert_cross_session_message(&crate::db::CrossSessionMessage {
            id: "m1".into(),
            thread_id: "t1".into(),
            from_session: "from".into(),
            to_session: sid.into(),
            body: "hello".into(),
            depth: 1,
            dedup_key: "k-m1".into(),
            agent_consumed_at: None,
            human_seen_at: None,
            created_at: 0,
        })
        .unwrap();
    }
    let wake = crate::mcp::delivery::test_support::MockDelivery::new(false);
    assert_eq!(crate::mcp::delivery::drain_idle(&db, &wake, sid, true), 1);

    let gate = WorktreeGateState::default();
    gate.enqueue_outcome(sid, "outcome note".to_string());
    let outcome = crate::mcp::delivery::test_support::MockDelivery::new(false);
    drain_worktree_outcomes(&gate, &outcome, sid);

    assert_eq!(outcome.wake_calls(), 0);
    assert_eq!(gate.take_outcomes(sid).len(), 1);
    wake.settle(false);
}
