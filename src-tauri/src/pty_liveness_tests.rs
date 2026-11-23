use super::*;
use crate::agents::state::AgentRuntimeState;

fn agents() -> AgentRuntimeState {
    AgentRuntimeState::bootstrap().unwrap()
}

fn is_live(agents: &AgentRuntimeState, session_id: &str) -> bool {
    agents.live_session_ids().contains(session_id)
}

#[test]
fn a_new_reservation_marks_the_session_live_with_mode_running() {
    let (mgr, agents) = (PtyManager::new(), agents());
    agents.record_activity("lv-new", "idle", None);

    assert_eq!(
        reserve_session_pty(&mgr, &agents, "lv-new", "pty-1").unwrap(),
        None
    );

    assert!(is_live(&agents, "lv-new"));
    assert_eq!(agents.session_activity("lv-new").unwrap().mode, "running");
}

#[test]
fn an_existing_reservation_leaves_the_live_set_unchanged() {
    let (mgr, agents) = (PtyManager::new(), agents());
    reserve_session_pty(&mgr, &agents, "lv-existing", "pty-1").unwrap();

    let second = reserve_session_pty(&mgr, &agents, "lv-existing", "pty-2").unwrap();
    assert_eq!(second, Some("pty-1".to_string()));

    agents.mark_dead_if("lv-existing", "pty-2");
    assert!(
        is_live(&agents, "lv-existing"),
        "the rejected pty-2 must not own the live entry"
    );
    agents.mark_dead_if("lv-existing", "pty-1");
    assert!(!is_live(&agents, "lv-existing"));
}

#[test]
fn releasing_the_current_pty_clears_the_reservation_and_liveness() {
    let (mgr, agents) = (PtyManager::new(), agents());
    reserve_session_pty(&mgr, &agents, "lv-release", "pty-1").unwrap();

    release_session_pty(&mgr, &agents, "lv-release", "pty-1");

    assert!(mgr.session_ptys.lock().unwrap().get("lv-release").is_none());
    assert!(!is_live(&agents, "lv-release"));
}

#[test]
fn releasing_an_older_pty_keeps_the_newer_reservation_and_liveness() {
    let (mgr, agents) = (PtyManager::new(), agents());
    reserve_session_pty(&mgr, &agents, "lv-older", "pty-2").unwrap();

    release_session_pty(&mgr, &agents, "lv-older", "pty-1");

    assert_eq!(
        mgr.session_ptys.lock().unwrap().get("lv-older"),
        Some(&"pty-2".to_string())
    );
    assert!(is_live(&agents, "lv-older"));
}

#[test]
fn killing_a_live_session_clears_its_reservation_and_liveness() {
    let (mgr, agents) = (PtyManager::new(), agents());
    reserve_session_pty(&mgr, &agents, "lv-kill", "pty-1").unwrap();

    kill_session_pty_inner(&mgr, &agents, "lv-kill").unwrap();

    assert!(!is_live(&agents, "lv-kill"));
    assert!(!mgr.has_session_pty("lv-kill"));
}

#[test]
fn a_kill_that_removed_an_older_pty_keeps_the_newer_pty_live() {
    let (mgr, agents) = (PtyManager::new(), agents());
    reserve_session_pty(&mgr, &agents, "lv-kill-race", "pty-1").unwrap();
    agents.mark_live("lv-kill-race", "pty-2");

    kill_session_pty_inner(&mgr, &agents, "lv-kill-race").unwrap();

    assert!(is_live(&agents, "lv-kill-race"));
}

#[test]
fn eof_of_the_current_pty_clears_liveness() {
    let agents = agents();
    agents.mark_live("lv-eof", "pty-1");

    on_agent_pty_eof(&agents, "lv-eof", "pty-1");

    assert!(!is_live(&agents, "lv-eof"));
}

#[test]
fn eof_of_an_older_pty_keeps_the_newer_pty_live() {
    let agents = agents();
    agents.mark_live("lv-eof-old", "pty-2");

    on_agent_pty_eof(&agents, "lv-eof-old", "pty-1");

    assert!(is_live(&agents, "lv-eof-old"));
}

#[test]
fn has_session_pty_is_true_while_reserved_and_false_after_release() {
    let (mgr, agents) = (PtyManager::new(), agents());
    assert!(!mgr.has_session_pty("lv-has"));

    reserve_session_pty(&mgr, &agents, "lv-has", "pty-1").unwrap();
    assert!(mgr.has_session_pty("lv-has"));

    release_session_pty(&mgr, &agents, "lv-has", "pty-1");
    assert!(!mgr.has_session_pty("lv-has"));
}

#[test]
fn resume_block_state_tracks_reservation_eof_and_release() {
    let (mgr, agents) = (PtyManager::new(), agents());

    reserve_session_pty(&mgr, &agents, "lv-block", "pty-1").unwrap();
    assert_eq!(mgr.resume_block_state(&agents, "lv-block"), (true, true));

    on_agent_pty_eof(&agents, "lv-block", "pty-1");
    assert_eq!(mgr.resume_block_state(&agents, "lv-block"), (false, true));

    release_session_pty(&mgr, &agents, "lv-block", "pty-1");
    assert_eq!(mgr.resume_block_state(&agents, "lv-block"), (false, false));
}
