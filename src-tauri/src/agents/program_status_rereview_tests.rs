use super::*;

#[test]
fn a_late_query_from_an_old_pty_leaves_the_live_ptys_arming_intact() {
    let agents = AgentRuntimeState::bootstrap().unwrap();
    agents.mark_live("late-q", "pty-a");
    agents.mark_live("late-q", "pty-b");
    agents.arm_program_status("late-q", "pty-b");

    agents.arm_program_status("late-q", "pty-a");

    assert!(agents.program_status_armed("late-q", "pty-b"));
    assert!(!agents.program_status_armed("late-q", "pty-a"));
}

#[test]
fn a_query_from_a_pty_that_is_not_live_arms_nothing() {
    let agents = AgentRuntimeState::bootstrap().unwrap();
    agents.mark_live("not-live", "pty-b");

    agents.arm_program_status("not-live", "pty-a");

    assert!(!agents.program_status_armed("not-live", "pty-a"));
    assert!(!agents.program_status_armed("not-live", "pty-b"));
}
