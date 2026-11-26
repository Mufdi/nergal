use super::*;
use crate::mcp::delivery::test_support::MockDelivery;
use crate::terminal::program_status::{ProgramStatus, ReportAction, RootState};

fn root(agents: &AgentRuntimeState, sid: &str, state: RootState) {
    agents.mark_live(sid, "pty-1");
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

fn notify(gate: &WorktreeGateState, mock: &MockDelivery, agents: &AgentRuntimeState, sid: &str) {
    notify_outcome(
        gate,
        mock,
        agents,
        sid,
        "req-1",
        &WorktreeRequestState::Denied,
    );
}

#[test]
fn an_outcome_for_a_blocked_requester_is_queued_even_when_its_hook_mode_is_idle() {
    let gate = WorktreeGateState::default();
    let mock = MockDelivery::new(false);
    let agents = AgentRuntimeState::bootstrap().unwrap();
    agents.record_activity("req-blocked", "idle", None);
    root(&agents, "req-blocked", RootState::Blocked);

    notify(&gate, &mock, &agents, "req-blocked");

    assert_eq!(mock.wake_calls(), 0);
    assert_eq!(gate.take_outcomes("req-blocked").len(), 1);
}

#[test]
fn an_outcome_for_an_unknown_session_is_still_queued() {
    let gate = WorktreeGateState::default();
    let mock = MockDelivery::new(false);
    let agents = AgentRuntimeState::bootstrap().unwrap();

    notify(&gate, &mock, &agents, "req-unknown");

    assert_eq!(mock.wake_calls(), 0);
    assert_eq!(gate.take_outcomes("req-unknown").len(), 1);
}

#[test]
fn an_outcome_for_a_requester_with_a_root_status_that_is_not_busy_wakes_it_now() {
    let gate = WorktreeGateState::default();
    let mock = MockDelivery::new(false);
    let agents = AgentRuntimeState::bootstrap().unwrap();
    agents.record_activity("req-ready", "running", None);
    root(&agents, "req-ready", RootState::Idle);

    notify(&gate, &mock, &agents, "req-ready");

    assert_eq!(mock.wake_calls(), 1);
    assert!(gate.take_outcomes("req-ready").is_empty());
}

#[test]
fn an_outcome_for_an_idle_hook_requester_without_a_root_status_wakes_it_now() {
    let gate = WorktreeGateState::default();
    let mock = MockDelivery::new(false);
    let agents = AgentRuntimeState::bootstrap().unwrap();
    agents.record_activity("req-hook-idle", "idle", None);

    notify(&gate, &mock, &agents, "req-hook-idle");

    assert_eq!(mock.wake_calls(), 1);
    assert!(gate.take_outcomes("req-hook-idle").is_empty());
}
