use super::*;

fn live_ids(state: &AgentRuntimeState) -> Vec<String> {
    state.live_session_ids().into_iter().collect()
}

#[test]
fn mark_live_puts_the_session_in_the_live_set_and_mark_dead_removes_it() {
    let state = AgentRuntimeState::bootstrap().unwrap();
    state.mark_live("live-a", "pty-a-1");
    assert_eq!(live_ids(&state), vec!["live-a".to_string()]);
    state.mark_dead("live-a");
    assert!(live_ids(&state).is_empty());
}

#[test]
fn mark_dead_if_with_another_pty_id_keeps_the_session_live() {
    let state = AgentRuntimeState::bootstrap().unwrap();
    state.mark_live("live-b", "pty-b-2");
    state.mark_dead_if("live-b", "pty-b-1");
    assert_eq!(live_ids(&state), vec!["live-b".to_string()]);
}

#[test]
fn mark_dead_if_with_the_same_pty_id_removes_the_session() {
    let state = AgentRuntimeState::bootstrap().unwrap();
    state.mark_live("live-c", "pty-c-1");
    state.mark_dead_if("live-c", "pty-c-1");
    assert!(live_ids(&state).is_empty());
}

#[test]
fn register_session_alone_never_makes_a_session_live() {
    let state = AgentRuntimeState::bootstrap().unwrap();
    state.register_session("live-d", AgentId::claude_code());
    assert!(state.live_session_ids().is_empty());
}

#[test]
fn a_late_hook_registration_after_the_kill_leaves_the_session_not_live() {
    let state = AgentRuntimeState::bootstrap().unwrap();
    state.mark_live("live-e", "pty-e-1");
    state.mark_dead_if("live-e", "pty-e-1");
    state.register_session("live-e", AgentId::claude_code());
    assert!(state.live_session_ids().is_empty());
}

#[test]
fn forget_session_removes_a_live_session() {
    let state = AgentRuntimeState::bootstrap().unwrap();
    state.mark_live("live-f", "pty-f-1");
    state.forget_session("live-f");
    assert!(state.live_session_ids().is_empty());
}
