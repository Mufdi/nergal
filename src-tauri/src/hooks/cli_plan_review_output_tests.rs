use super::{PlanReviewDecision, parse_plan_review_decision, plan_review_output};
use serde_json::json;

#[test]
fn allow_echoes_tool_input_as_updated_input() {
    let tool_input = json!({"plan": "# A", "planFilePath": "/p/a.md"});
    let out = plan_review_output(&PlanReviewDecision::Allow, &tool_input).unwrap();
    let hso = &out["hookSpecificOutput"];
    assert_eq!(hso["hookEventName"], "PermissionRequest");
    assert_eq!(hso["decision"]["behavior"], "allow");
    assert_eq!(hso["decision"]["updatedInput"], tool_input);
}

#[test]
fn deny_carries_message_and_no_updated_input() {
    let out = plan_review_output(
        &PlanReviewDecision::Deny("fix step 2".to_string()),
        &json!({"plan": "# A"}),
    )
    .unwrap();
    let decision = &out["hookSpecificOutput"]["decision"];
    assert_eq!(
        out["hookSpecificOutput"]["hookEventName"],
        "PermissionRequest"
    );
    assert_eq!(decision["behavior"], "deny");
    assert_eq!(decision["message"], "fix step 2");
    assert!(decision.get("updatedInput").is_none());
}

#[test]
fn released_prints_nothing() {
    assert!(plan_review_output(&PlanReviewDecision::Released, &json!({"plan": "# A"})).is_none());
}

#[test]
fn parse_released_sentinel() {
    assert_eq!(
        parse_plan_review_decision("{\"released\":true}").unwrap(),
        PlanReviewDecision::Released
    );
}

#[test]
fn parse_approved_true_is_allow() {
    assert_eq!(
        parse_plan_review_decision("{\"approved\":true}").unwrap(),
        PlanReviewDecision::Allow
    );
}

#[test]
fn parse_approved_false_is_deny_with_message() {
    assert_eq!(
        parse_plan_review_decision("{\"approved\":false,\"message\":\"m\"}").unwrap(),
        PlanReviewDecision::Deny("m".to_string())
    );
}
