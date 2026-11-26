use super::*;

#[test]
fn reports_apply_only_to_an_agent_pty_that_is_armed() {
    for (agent_session, armed, expected) in [
        (true, true, true),
        (true, false, false),
        (false, true, false),
        (false, false, false),
    ] {
        assert_eq!(
            reports_apply(agent_session, armed),
            expected,
            "agent_session {agent_session} armed {armed}"
        );
    }
}
