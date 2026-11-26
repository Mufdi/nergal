use super::*;
use crate::agents::state::{AgentRuntimeState, OwnedStatus, SessionActivity};
use crate::db::{CrossSessionMessage, CrossSessionThread, Database, SharedDb};
use crate::mcp::delivery::SessionDelivery;
use crate::mcp::delivery::test_support::MockDelivery;
use crate::mcp::worktree_sessions::{WorktreeGateState, WorktreeRequestState, notify_outcome};
use crate::terminal::program_status::{ProgramStatus, ReportAction, RootState};
use std::sync::{Arc, Mutex};

fn set_root_at(agents: &AgentRuntimeState, sid: &str, state: RootState, at: u64) {
    agents.apply_program_status(
        sid,
        "pty-1",
        ReportAction::Set(ProgramStatus {
            state,
            kind: None,
            msg: None,
        }),
        at,
    );
}

fn hook(mode: &str, last_activity: u64) -> SessionActivity {
    SessionActivity {
        mode: mode.to_string(),
        last_activity,
        waiting_for: None,
    }
}

fn owned(state: RootState, set_at: u64) -> OwnedStatus {
    OwnedStatus {
        pty_id: "pty-1".to_string(),
        status: ProgramStatus {
            state,
            kind: None,
            msg: None,
        },
        set_at,
    }
}

#[test]
fn stop_drain_is_refused_only_for_a_blocked_root() {
    use RootState::{Blocked, Done, Error, Idle, Working};
    assert!(stop_drain_allowed(None), "a background agent wait has no root");
    for (root, expected) in [
        (Blocked, false),
        (Working, true),
        (Done, true),
        (Idle, true),
        (Error, true),
    ] {
        assert_eq!(
            stop_drain_allowed(Some(&root)),
            expected,
            "root {}",
            root.as_str()
        );
    }
}

#[test]
fn a_ready_transition_is_busy_before_and_not_busy_after() {
    for (before, after, expected) in [
        (true, false, true),
        (true, true, false),
        (false, false, false),
        (false, true, false),
    ] {
        assert_eq!(
            ready_transition(before, after),
            expected,
            "before {before} after {after}"
        );
    }
}

#[test]
fn idle_since_is_the_instant_the_target_stopped_being_busy() {
    use RootState::{Blocked, Done, Idle, Working};
    let cases: [(Option<SessionActivity>, Option<OwnedStatus>, Option<u64>); 8] = [
        (Some(hook("running", 900)), Some(owned(Done, 1000)), Some(1000)),
        (Some(hook("idle", 1002)), Some(owned(Done, 1000)), Some(1002)),
        (None, Some(owned(Idle, 1000)), Some(1000)),
        (Some(hook("idle", 900)), None, Some(900)),
        (Some(hook("idle", 900)), Some(owned(Working, 1000)), Some(900)),
        (Some(hook("running", 900)), None, None),
        (Some(hook("idle", 900)), Some(owned(Blocked, 1000)), None),
        (None, None, None),
    ];
    for (hook, root, expected) in cases {
        assert_eq!(
            idle_since(hook.as_ref(), root.as_ref()),
            expected,
            "hook {hook:?} root {root:?}"
        );
    }
}

fn enabled() -> crate::config::CrossSessionConfig {
    crate::config::CrossSessionConfig {
        enabled: true,
        max_hops: 4,
        msg_budget: 30,
        deadline_secs: 1800,
    }
}

#[test]
fn a_done_report_before_stop_opens_the_settle_window_for_a_send() {
    let mock = Arc::new(MockDelivery::new(false));
    let ctx = DaemonContext {
        db: Arc::new(Mutex::new(Database::open_in_memory().unwrap())),
        agents: AgentRuntimeState::bootstrap().unwrap(),
        delivery: mock.clone(),
        worktree_gate: Default::default(),
    };
    let now = now_secs();
    ctx.agents.mark_live("done-before-stop", "pty-1");
    ctx.agents.set_activity_for_test(
        "done-before-stop",
        "running",
        None,
        now.saturating_sub(60),
    );
    set_root_at(
        &ctx.agents,
        "done-before-stop",
        RootState::Done,
        now.saturating_sub(1),
    );

    let r = send_to_session(&ctx, &enabled(), "sender", "done-before-stop", "hello", None).unwrap();

    assert_eq!(r["delivery"], "queued");
    assert_eq!(mock.wake_calls(), 0);
}

fn seed_pending(db: &SharedDb, to: &str, thread_id: &str) {
    let g = db.lock().unwrap();
    g.insert_cross_session_thread(&CrossSessionThread {
        id: thread_id.into(),
        originator_session: "from".into(),
        participants: vec!["from".into(), to.into()],
        status: "active".into(),
        max_hops: 4,
        msg_count: 1,
        msg_budget: Some(30),
        deadline_at: Some(u64::MAX >> 1),
        created_at: 0,
    })
    .unwrap();
    g.insert_cross_session_message(&CrossSessionMessage {
        id: format!("m-{thread_id}"),
        thread_id: thread_id.into(),
        from_session: "from".into(),
        to_session: to.into(),
        body: "hello".into(),
        depth: 1,
        dedup_key: format!("k-{thread_id}"),
        agent_consumed_at: None,
        human_seen_at: None,
        created_at: 0,
    })
    .unwrap();
}

#[tokio::test(start_paused = true)]
async fn an_in_flight_wake_holds_the_worktree_outcome() {
    let sid = "inflight-outcome";
    let db: SharedDb = Arc::new(Mutex::new(Database::open_in_memory().unwrap()));
    let agents = AgentRuntimeState::bootstrap().unwrap();
    agents.mark_live(sid, "pty-1");
    let mock = Arc::new(MockDelivery::new(false));
    let gate = Arc::new(WorktreeGateState::default());
    set_root_at(&agents, sid, RootState::Blocked, 1000);
    notify_outcome(
        &gate,
        mock.as_ref(),
        &agents,
        sid,
        "req-inflight",
        &WorktreeRequestState::Denied,
    );
    seed_pending(&db, sid, "inflight-outcome-t");
    set_root_at(&agents, sid, RootState::Idle, 1001);
    let drain = || {
        ready_drain(
            db.clone(),
            agents.clone(),
            Some(gate.clone()),
            mock.clone() as Arc<dyn SessionDelivery>,
            sid.to_string(),
            true,
        )
    };

    assert_eq!(drain().await, 1);
    assert!(crate::mcp::delivery::is_in_flight(sid));
    assert_eq!(mock.wake_calls(), 1);

    assert_eq!(drain().await, 0);
    assert_eq!(
        mock.wake_calls(),
        1,
        "the outcome note waits for the wake in flight"
    );
    mock.settle(true);
}
