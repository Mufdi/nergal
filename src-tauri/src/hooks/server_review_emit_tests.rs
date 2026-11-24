use super::{emits_review_closed, review_decision_for};
use crate::hooks::events::HookEvent;
use serde_json::json;

fn post_tool(tool: &str, agent: Option<&str>) -> HookEvent {
    HookEvent::PostToolUse {
        session_id: "s1".into(),
        tool_name: tool.into(),
        tool_input: json!({}),
        tool_result: None,
        agent_id: agent.map(String::from),
        agent_type: None,
    }
}

fn stop() -> HookEvent {
    HookEvent::Stop {
        session_id: "s1".into(),
        stop_reason: None,
        transcript_path: None,
        background_tasks: vec![],
        session_crons: vec![],
        last_assistant_message: None,
    }
}

fn subagent_stop(agent: Option<&str>) -> HookEvent {
    HookEvent::SubagentStop {
        session_id: "s1".into(),
        agent_id: agent.map(String::from),
        agent_type: None,
        last_assistant_message: None,
        effort: json!(null),
        agent_transcript_path: None,
    }
}

#[test]
fn registered_main_agent_owner_emits() {
    assert!(emits_review_closed(
        &post_tool("ExitPlanMode", None),
        true,
        Some(None)
    ));
}

#[test]
fn registered_subagent_owner_emits_only_for_that_subagent() {
    assert!(emits_review_closed(
        &post_tool("ExitPlanMode", Some("a1")),
        true,
        Some(Some("a1"))
    ));
    assert!(!emits_review_closed(
        &post_tool("ExitPlanMode", Some("a2")),
        true,
        Some(Some("a1"))
    ));
    assert!(!emits_review_closed(
        &post_tool("ExitPlanMode", None),
        true,
        Some(Some("a1"))
    ));
}

#[test]
fn registration_owner_mismatch_does_not_emit() {
    assert!(!emits_review_closed(
        &post_tool("ExitPlanMode", Some("a1")),
        true,
        Some(None)
    ));
}

// The panel already claimed the registration for a delivered Revise, so the
// terminal approval must still clear the stale annotations.
#[test]
fn unregistered_main_agent_exit_plan_mode_still_emits() {
    assert!(emits_review_closed(
        &post_tool("ExitPlanMode", None),
        true,
        None
    ));
}

#[test]
fn unregistered_subagent_exit_plan_mode_does_not_emit() {
    assert!(!emits_review_closed(
        &post_tool("ExitPlanMode", Some("a1")),
        true,
        None
    ));
}

#[test]
fn non_top_level_never_emits() {
    assert!(!emits_review_closed(
        &post_tool("ExitPlanMode", None),
        false,
        None
    ));
    assert!(!emits_review_closed(
        &post_tool("ExitPlanMode", None),
        false,
        Some(None)
    ));
}

#[test]
fn other_tools_do_not_emit() {
    assert!(!emits_review_closed(&post_tool("Bash", None), true, None));
    assert!(!emits_review_closed(
        &post_tool("Bash", None),
        true,
        Some(None)
    ));
}

#[test]
fn releasing_events_never_emit() {
    let end = HookEvent::SessionEnd {
        session_id: "s1".into(),
    };
    for owner in [None, Some(None), Some(Some("a1"))] {
        assert!(!emits_review_closed(&stop(), true, owner));
        assert!(!emits_review_closed(
            &subagent_stop(Some("a1")),
            true,
            owner
        ));
        assert!(!emits_review_closed(&end, true, owner));
    }
}

#[test]
fn top_level_plan_review_keeps_its_decision() {
    assert_eq!(
        review_decision_for(true, Some("approve".into())),
        Some("approve".into())
    );
    assert_eq!(review_decision_for(true, None), None);
}

#[test]
fn nested_plan_review_opens_no_live_review() {
    assert_eq!(review_decision_for(false, Some("approve".into())), None);
    assert_eq!(review_decision_for(false, None), None);
}
