use super::{PtyManager, cc_resume_arg};

#[test]
fn continue_resolves_to_the_picked_conversation_id() {
    assert_eq!(
        cc_resume_arg(Some("continue"), Some("aaaa-1111".into())).as_deref(),
        Some("aaaa-1111")
    );
}

#[test]
fn continue_without_a_picked_id_starts_fresh() {
    assert_eq!(cc_resume_arg(Some("continue"), None), None);
}

#[test]
fn resume_pick_and_explicit_ids_pass_through_unchanged() {
    assert_eq!(
        cc_resume_arg(Some("resume_pick"), Some("ignored".into())).as_deref(),
        Some("resume_pick")
    );
    assert_eq!(
        cc_resume_arg(Some("abc-123"), None).as_deref(),
        Some("abc-123")
    );
    assert_eq!(cc_resume_arg(None, None), None);
}

#[test]
fn agent_shell_pid_is_none_for_an_unknown_session() {
    assert_eq!(PtyManager::new().agent_shell_pid("no-such-session"), None);
}
