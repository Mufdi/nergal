use super::{
    PlanReviewDecision, approved_plan_override, parse_plan_review_decision, plan_review_output,
    plan_review_output_with_plan, review_session_id,
};
use serde_json::json;

#[test]
fn empty_object_releases_the_hook() {
    assert_eq!(
        parse_plan_review_decision("{}").unwrap(),
        PlanReviewDecision::Released
    );
}

#[test]
fn released_as_string_releases_the_hook() {
    assert_eq!(
        parse_plan_review_decision("{\"released\":\"true\"}").unwrap(),
        PlanReviewDecision::Released
    );
}

#[test]
fn explicit_denial_without_message_uses_default() {
    assert_eq!(
        parse_plan_review_decision("{\"approved\":false}").unwrap(),
        PlanReviewDecision::Deny("Plan changes requested".into())
    );
}

#[test]
fn approval_without_override_echoes_tool_input() {
    let tool_input = json!({"plan": "# A", "planFilePath": "/p/a.md"});
    let out = plan_review_output(&PlanReviewDecision::Allow, &tool_input).unwrap();
    assert_eq!(
        out["hookSpecificOutput"]["decision"]["updatedInput"],
        tool_input
    );
}

#[test]
fn approved_plan_override_reads_the_plan_string() {
    assert_eq!(
        approved_plan_override("{\"approved\":true,\"plan\":\"# A edited\"}"),
        Some("# A edited".to_string())
    );
}

#[test]
fn approved_plan_override_is_none_without_plan_or_on_garbage() {
    assert_eq!(approved_plan_override("{\"approved\":true}"), None);
    assert_eq!(approved_plan_override("not json"), None);
    assert_eq!(approved_plan_override("{\"plan\":3}"), None);
}

#[test]
fn edited_plan_replaces_plan_and_keeps_other_fields() {
    let tool_input = json!({"plan": "# A", "planFilePath": "/p/a.md"});
    let out =
        plan_review_output_with_plan(&PlanReviewDecision::Allow, &tool_input, Some("# A edited"))
            .unwrap();
    assert_eq!(
        out["hookSpecificOutput"]["decision"]["updatedInput"],
        json!({"plan": "# A edited", "planFilePath": "/p/a.md"})
    );
}

#[test]
fn no_override_echoes_tool_input() {
    let tool_input = json!({"plan": "# A", "planFilePath": "/p/a.md"});
    let out = plan_review_output_with_plan(&PlanReviewDecision::Allow, &tool_input, None).unwrap();
    assert_eq!(
        out["hookSpecificOutput"]["decision"]["updatedInput"],
        tool_input
    );
}

#[test]
fn approval_without_object_tool_input_emits_nothing() {
    for ti in [json!(null), json!("x")] {
        assert!(plan_review_output_with_plan(&PlanReviewDecision::Allow, &ti, None).is_none());
        assert!(
            plan_review_output_with_plan(&PlanReviewDecision::Allow, &ti, Some("# A")).is_none()
        );
    }
}

#[test]
fn deny_ignores_the_override() {
    let tool_input = json!({"plan": "# A"});
    let out = plan_review_output_with_plan(
        &PlanReviewDecision::Deny("no".into()),
        &tool_input,
        Some("# B"),
    )
    .unwrap();
    assert_eq!(out["hookSpecificOutput"]["decision"]["behavior"], "deny");
    assert!(out["hookSpecificOutput"]["decision"]["updatedInput"].is_null());
}

#[test]
fn released_emits_nothing() {
    assert!(
        plan_review_output_with_plan(&PlanReviewDecision::Released, &json!({"plan": "x"}), None)
            .is_none()
    );
}

#[test]
fn empty_session_id_is_treated_as_missing() {
    assert_eq!(review_session_id(None), None);
    assert_eq!(review_session_id(Some(String::new())), None);
    assert_eq!(review_session_id(Some("s1".into())), Some("s1".into()));
}
