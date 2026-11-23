use super::*;
use crate::agents::state::AgentRuntimeState;

#[test]
fn a_reserved_session_stays_live_while_only_the_frontend_soft_close_timer_runs() {
    let (mgr, agents) = (PtyManager::new(), AgentRuntimeState::bootstrap().unwrap());
    reserve_session_pty(&mgr, &agents, "soft-1", "pty-1").unwrap();

    assert!(agents.live_session_ids().contains("soft-1"));
    assert!(mgr.has_session_pty("soft-1"));
}
