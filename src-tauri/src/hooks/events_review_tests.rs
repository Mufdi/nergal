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
fn only_a_session_end_that_is_not_clear_disarms_the_protocol() {
    assert!(!disarms_on_session_end(Some("clear")));
    for reason in [
        Some("prompt_input_exit"),
        Some("logout"),
        Some("other"),
        None,
    ] {
        assert!(disarms_on_session_end(reason), "reason {reason:?}");
    }
}
