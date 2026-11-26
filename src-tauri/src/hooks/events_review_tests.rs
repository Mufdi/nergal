use super::*;

#[test]
fn session_end_carries_its_reason_and_still_parses_without_one() {
    let with: HookEvent = serde_json::from_str(
        r#"{"hook_event_name":"SessionEnd","session_id":"se-1","reason":"clear"}"#,
    )
    .unwrap();
    let without: HookEvent =
        serde_json::from_str(r#"{"hook_event_name":"SessionEnd","session_id":"se-2"}"#).unwrap();

    match (with, without) {
        (HookEvent::SessionEnd { reason: r1, .. }, HookEvent::SessionEnd { reason: r2, .. }) => {
            assert_eq!((r1, r2), (Some("clear".to_string()), None))
        }
        _ => panic!("expected SessionEnd"),
    }
}

#[test]
fn only_a_top_level_session_end_that_is_not_clear_or_resume_disarms_the_protocol() {
    for (top_level, reason, expected) in [
        (true, Some("prompt_input_exit"), true),
        (true, Some("logout"), true),
        (true, Some("other"), true),
        (true, None, true),
        (true, Some("clear"), false),
        (true, Some("resume"), false),
        (false, Some("prompt_input_exit"), false),
        (false, Some("other"), false),
        (false, None, false),
        (false, Some("clear"), false),
        (false, Some("resume"), false),
    ] {
        assert_eq!(
            disarms_on_session_end(top_level, reason),
            expected,
            "top_level {top_level}, reason {reason:?}"
        );
    }
}
