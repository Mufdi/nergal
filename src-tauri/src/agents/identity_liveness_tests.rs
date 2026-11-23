use super::*;

#[test]
fn a_live_sessions_nergal_id_resolves() {
    let state = AgentRuntimeState::bootstrap().unwrap();
    state.register_session("id-live", AgentId::claude_code());
    state.mark_live("id-live", "pty-1");

    assert_eq!(
        state.resolve_session_hint("id-live").as_deref(),
        Some("id-live")
    );
}

#[test]
fn a_killed_session_still_in_the_adapter_cache_is_unidentified() {
    let state = AgentRuntimeState::bootstrap().unwrap();
    state.register_session("id-killed", AgentId::claude_code());
    state.mark_live("id-killed", "pty-1");
    state.mark_dead_if("id-killed", "pty-1");

    assert!(state.resolve("id-killed").is_some());
    assert!(state.resolve_session_hint("id-killed").is_none());
}

#[test]
fn a_cc_id_mapped_to_a_live_session_resolves() {
    let state = AgentRuntimeState::bootstrap().unwrap();
    state.register_session("id-cc-live", AgentId::claude_code());
    state.mark_live("id-cc-live", "pty-1");
    state.map_cc_session("cc-live", "id-cc-live");

    assert_eq!(
        state.resolve_session_hint("cc-live").as_deref(),
        Some("id-cc-live")
    );
}

#[test]
fn a_cc_id_mapped_to_a_killed_session_is_unidentified() {
    let state = AgentRuntimeState::bootstrap().unwrap();
    state.register_session("id-cc-killed", AgentId::claude_code());
    state.mark_live("id-cc-killed", "pty-1");
    state.map_cc_session("cc-killed", "id-cc-killed");
    state.mark_dead_if("id-cc-killed", "pty-1");

    assert!(state.resolve_session_hint("cc-killed").is_none());
}
