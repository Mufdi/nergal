use super::*;
use crate::terminal::program_status::{ProgramStatus, ReportAction, RootState};

fn working() -> ReportAction {
    ReportAction::Set(ProgramStatus {
        state: RootState::Working,
        kind: None,
        msg: None,
    })
}

fn live(sid: &str, pty: &str) -> AgentRuntimeState {
    let agents = AgentRuntimeState::bootstrap().unwrap();
    agents.mark_live(sid, pty);
    agents
}

#[test]
fn a_session_is_unarmed_until_the_scanner_answers_a_query() {
    let agents = live("arm-none", "pty-1");
    assert!(!agents.program_status_armed("arm-none", "pty-1"));
}

#[test]
fn arming_covers_only_the_pty_that_was_queried() {
    let agents = live("arm-pty", "pty-1");
    agents.arm_program_status("arm-pty", "pty-1");

    assert!(agents.program_status_armed("arm-pty", "pty-1"));
    assert!(!agents.program_status_armed("arm-pty", "pty-2"));
    assert!(!agents.program_status_armed("arm-other", "pty-1"));
}

#[test]
fn a_new_pty_arms_itself_and_drops_the_old_ones_arming() {
    let agents = live("arm-replace", "pty-a");
    agents.arm_program_status("arm-replace", "pty-a");
    agents.mark_live("arm-replace", "pty-b");
    agents.arm_program_status("arm-replace", "pty-b");

    assert!(agents.program_status_armed("arm-replace", "pty-b"));
    assert!(!agents.program_status_armed("arm-replace", "pty-a"));
}

#[test]
fn disarming_stops_reports_without_touching_the_stored_status() {
    let agents = live("arm-off", "pty-1");
    agents.arm_program_status("arm-off", "pty-1");
    agents.apply_program_status("arm-off", "pty-1", working(), 1000);

    agents.disarm_program_status("arm-off");

    assert!(!agents.program_status_armed("arm-off", "pty-1"));
    assert_eq!(
        agents.program_status("arm-off").unwrap().status.state,
        RootState::Working
    );
}

#[test]
fn eof_of_the_owner_disarms_even_before_any_report_was_stored() {
    let agents = live("arm-eof", "pty-1");
    agents.arm_program_status("arm-eof", "pty-1");

    agents.clear_program_status_if("arm-eof", "pty-1");

    assert!(!agents.program_status_armed("arm-eof", "pty-1"));
}

#[test]
fn eof_of_an_old_pty_keeps_the_new_ptys_arming() {
    let agents = live("arm-old-eof", "pty-b");
    agents.arm_program_status("arm-old-eof", "pty-b");

    agents.clear_program_status_if("arm-old-eof", "pty-a");

    assert!(agents.program_status_armed("arm-old-eof", "pty-b"));
}

#[test]
fn an_identical_report_from_the_live_pty_takes_over_ownership_and_keeps_the_set_time() {
    let agents = live("arm-owner", "pty-a");
    agents.apply_program_status("arm-owner", "pty-a", working(), 1000);
    agents.mark_live("arm-owner", "pty-b");

    let change = agents.apply_program_status("arm-owner", "pty-b", working(), 1005);

    assert!(change.is_none());
    let stored = agents.program_status("arm-owner").unwrap();
    assert_eq!((stored.pty_id.as_str(), stored.set_at), ("pty-b", 1000));
    assert!(
        agents
            .clear_program_status_if("arm-owner", "pty-a")
            .is_none(),
        "the old PTY's EOF no longer removes the status"
    );
}
