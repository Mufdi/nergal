// `ready_drain` takes the gate as `Option<Arc<WorktreeGateState>>`, exactly as implementation.md.
use super::*;
use crate::agents::state::AgentRuntimeState;
use crate::db::{CrossSessionMessage, CrossSessionThread, Database, SharedDb};
use crate::mcp::delivery::SessionDelivery;
use crate::mcp::delivery::test_support::MockDelivery;
use crate::mcp::worktree_sessions::{
    WorktreeGateState, WorktreeRequestState, drain_worktree_outcomes, notify_outcome,
};
use crate::terminal::program_status::{ProgramStatus, ReportAction, RootState};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const HOOK_MODES: [Option<&str>; 4] =
    [None, Some("running"), Some("idle"), Some("needs_attention")];

fn all_roots() -> Vec<Option<RootState>> {
    vec![
        None,
        Some(RootState::Working),
        Some(RootState::Blocked),
        Some(RootState::Done),
        Some(RootState::Idle),
        Some(RootState::Error),
    ]
}

// -- busy_rule --

#[test]
fn busy_rule_matches_the_design_table_for_all_24_combinations() {
    use RootState::{Blocked, Done, Error, Idle, Working};
    let table: [(Option<RootState>, Option<&str>, bool); 24] = [
        (None, None, false),
        (None, Some("running"), true),
        (None, Some("idle"), false),
        (None, Some("needs_attention"), true),
        (Some(Working), None, false),
        (Some(Working), Some("running"), true),
        (Some(Working), Some("idle"), false),
        (Some(Working), Some("needs_attention"), true),
        (Some(Blocked), None, true),
        (Some(Blocked), Some("running"), true),
        (Some(Blocked), Some("idle"), true),
        (Some(Blocked), Some("needs_attention"), true),
        (Some(Done), None, false),
        (Some(Done), Some("running"), false),
        (Some(Done), Some("idle"), false),
        (Some(Done), Some("needs_attention"), false),
        (Some(Idle), None, false),
        (Some(Idle), Some("running"), false),
        (Some(Idle), Some("idle"), false),
        (Some(Idle), Some("needs_attention"), false),
        (Some(Error), None, false),
        (Some(Error), Some("running"), false),
        (Some(Error), Some("idle"), false),
        (Some(Error), Some("needs_attention"), false),
    ];
    for (root, hook, expected) in table {
        let label = format!(
            "root {:?} hook {hook:?}",
            root.as_ref().map(RootState::as_str)
        );
        assert_eq!(busy_rule(root, hook), expected, "{label}");
    }
}

#[test]
fn busy_rule_never_pastes_into_a_dialog_and_never_strands_a_ready_target() {
    for hook in HOOK_MODES {
        for root in all_roots() {
            let name = root.as_ref().map(RootState::as_str);
            let busy = busy_rule(root, hook);
            match name {
                Some("blocked") => assert!(busy, "a dialog is never a paste target ({hook:?})"),
                Some("done" | "idle" | "error") => {
                    assert!(
                        !busy,
                        "{name:?} accepts a prompt whatever the hook says ({hook:?})"
                    )
                }
                None => assert_eq!(
                    busy,
                    matches!(hook, Some("running" | "needs_attention")),
                    "no root keeps today's rule ({hook:?})"
                ),
                _ => {}
            }
        }
    }
}

// -- delivery_busy --

fn live_agents(sid: &str) -> AgentRuntimeState {
    let agents = AgentRuntimeState::bootstrap().unwrap();
    agents.mark_live(sid, "pty-1");
    agents
}

fn set_root(agents: &AgentRuntimeState, sid: &str, state: RootState) {
    agents.apply_program_status(
        sid,
        "pty-1",
        ReportAction::Set(ProgramStatus {
            state,
            kind: None,
            msg: None,
        }),
        1000,
    );
}

#[test]
fn delivery_busy_reads_the_root_status_and_the_hook_mode_of_the_session() {
    let agents = live_agents("busy-read");
    assert!(
        !delivery_busy(&agents, "busy-read"),
        "unknown activity is not busy"
    );

    agents.record_activity("busy-read", "idle", None);
    set_root(&agents, "busy-read", RootState::Blocked);
    assert!(delivery_busy(&agents, "busy-read"));

    set_root(&agents, "busy-read", RootState::Working);
    assert!(
        !delivery_busy(&agents, "busy-read"),
        "background work after Stop"
    );
}

#[test]
fn a_background_agent_completing_frees_a_working_root() {
    let agents = live_agents("busy-bg");
    set_root(&agents, "busy-bg", RootState::Working);
    agents.record_activity(
        "busy-bg",
        "needs_attention",
        Some("background agent: input"),
    );
    assert!(delivery_busy(&agents, "busy-bg"));

    agents.record_activity("busy-bg", "idle", None);
    assert!(!delivery_busy(&agents, "busy-bg"));
}

// -- send_to_session --

fn ctx_with(mock: &Arc<MockDelivery>) -> DaemonContext {
    DaemonContext {
        db: Arc::new(Mutex::new(Database::open_in_memory().unwrap())),
        agents: AgentRuntimeState::bootstrap().unwrap(),
        delivery: mock.clone(),
        worktree_gate: Default::default(),
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

fn target_with_root(ctx: &DaemonContext, sid: &str, hook: Option<(&str, u64)>, root: RootState) {
    ctx.agents.mark_live(sid, "pty-1");
    if let Some((mode, last_activity)) = hook {
        ctx.agents
            .set_activity_for_test(sid, mode, None, last_activity);
    }
    set_root(&ctx.agents, sid, root);
}

fn pending_for(ctx: &DaemonContext, sid: &str) -> usize {
    ctx.with_db(|db| db.cross_session_undelivered_for(sid))
        .unwrap()
        .len()
}

#[test]
fn the_folder_trust_prompt_queues_the_message_and_pastes_nothing() {
    let mock = Arc::new(MockDelivery::new(false));
    let ctx = ctx_with(&mock);
    target_with_root(
        &ctx,
        "trust",
        Some(("running", now_secs())),
        RootState::Blocked,
    );

    let r = send_to_session(&ctx, &enabled(), "sender", "trust", "hello", None).unwrap();

    assert_eq!(r["delivery"], "queued");
    assert_eq!(mock.wake_calls(), 0);
    assert_eq!(pending_for(&ctx, "trust"), 1);
}

#[test]
fn background_work_after_stop_does_not_hold_delivery() {
    let mock = Arc::new(MockDelivery::new(false));
    let ctx = ctx_with(&mock);
    target_with_root(
        &ctx,
        "bg-after-stop",
        Some(("idle", now_secs().saturating_sub(60))),
        RootState::Working,
    );

    let r = send_to_session(&ctx, &enabled(), "sender", "bg-after-stop", "hello", None).unwrap();

    assert_eq!(r["delivery"], "delivered");
    assert_eq!(mock.wake_calls(), 1);
    mock.settle(true);
}

#[test]
fn an_idle_root_ignores_a_stale_running_hook_mode() {
    let mock = Arc::new(MockDelivery::new(false));
    let ctx = ctx_with(&mock);
    target_with_root(
        &ctx,
        "stale-running",
        Some(("running", now_secs().saturating_sub(60))),
        RootState::Idle,
    );

    let r = send_to_session(&ctx, &enabled(), "sender", "stale-running", "hello", None).unwrap();

    assert_eq!(r["delivery"], "delivered");
    assert_eq!(mock.wake_calls(), 1);
    mock.settle(true);
}

// -- ready_drain (paused tokio clock) --

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

struct DrainFixture {
    db: SharedDb,
    agents: AgentRuntimeState,
    mock: Arc<MockDelivery>,
    gate: Arc<WorktreeGateState>,
}

fn drain_fixture(sid: &str) -> DrainFixture {
    DrainFixture {
        db: Arc::new(Mutex::new(Database::open_in_memory().unwrap())),
        agents: live_agents(sid),
        mock: Arc::new(MockDelivery::new(false)),
        gate: Arc::new(WorktreeGateState::default()),
    }
}

impl DrainFixture {
    async fn ready_drain(&self, sid: &str, with_gate: bool) -> usize {
        ready_drain(
            self.db.clone(),
            self.agents.clone(),
            with_gate.then(|| self.gate.clone()),
            self.mock.clone() as Arc<dyn SessionDelivery>,
            sid.to_string(),
            true,
        )
        .await
    }
}

#[tokio::test(start_paused = true)]
async fn ready_drain_waits_out_the_settle_window_then_drains_once() {
    let fx = drain_fixture("rd-free");
    seed_pending(&fx.db, "rd-free", "rd-free-t");
    set_root(&fx.agents, "rd-free", RootState::Idle);

    let started = tokio::time::Instant::now();
    let pasted = fx.ready_drain("rd-free", false).await;

    assert!(started.elapsed() >= Duration::from_secs(IDLE_SETTLE_SECS + 1));
    assert_eq!(pasted, 1);
    assert_eq!(fx.mock.pastes(), 1);
    fx.mock.settle(true);
}

#[tokio::test(start_paused = true)]
async fn ready_drain_does_not_paste_when_the_target_turns_busy_before_the_window_ends() {
    let fx = drain_fixture("rd-busy");
    seed_pending(&fx.db, "rd-busy", "rd-busy-t");
    set_root(&fx.agents, "rd-busy", RootState::Idle);

    let (pasted, ()) = tokio::join!(fx.ready_drain("rd-busy", false), async {
        tokio::time::sleep(Duration::from_secs(1)).await;
        set_root(&fx.agents, "rd-busy", RootState::Working);
        fx.agents.record_activity("rd-busy", "running", None);
    });

    assert_eq!(pasted, 0);
    assert_eq!(fx.mock.wake_calls(), 0);
    assert_eq!(
        fx.db
            .lock()
            .unwrap()
            .cross_session_undelivered_for("rd-busy")
            .unwrap()
            .len(),
        1,
        "the message waits for the next busy-to-not-busy transition"
    );
}

#[tokio::test(start_paused = true)]
async fn ready_drain_delivers_a_queued_worktree_outcome_when_no_message_is_pending() {
    let fx = drain_fixture("rd-outcome");
    set_root(&fx.agents, "rd-outcome", RootState::Blocked);
    notify_outcome(
        &fx.gate,
        fx.mock.as_ref(),
        &fx.agents,
        "rd-outcome",
        "req-1",
        &WorktreeRequestState::Denied,
    );
    assert_eq!(fx.mock.wake_calls(), 0, "a dialog is not a paste target");

    set_root(&fx.agents, "rd-outcome", RootState::Idle);
    fx.ready_drain("rd-outcome", true).await;

    assert_eq!(fx.mock.wake_calls(), 1);
}

#[tokio::test(start_paused = true)]
async fn ready_drain_pastes_only_the_cross_session_note_when_both_are_pending() {
    let fx = drain_fixture("rd-both");
    seed_pending(&fx.db, "rd-both", "rd-both-t");
    set_root(&fx.agents, "rd-both", RootState::Blocked);
    notify_outcome(
        &fx.gate,
        fx.mock.as_ref(),
        &fx.agents,
        "rd-both",
        "req-2",
        &WorktreeRequestState::Denied,
    );
    set_root(&fx.agents, "rd-both", RootState::Idle);

    fx.ready_drain("rd-both", true).await;
    assert_eq!(fx.mock.wake_calls(), 1, "one paste per transition");
    fx.mock.settle(true);

    drain_worktree_outcomes(&fx.gate, fx.mock.as_ref(), "rd-both");
    assert_eq!(fx.mock.wake_calls(), 2, "the outcome was still queued");
}
