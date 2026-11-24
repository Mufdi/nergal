use super::*;
use crate::hooks::events::{GithubPr, HookEvent};

fn pr(number: Option<u64>, kind: Option<&str>) -> Option<GithubPr> {
    Some(GithubPr {
        number,
        url: Some("u".into()),
        review_state: Some("pending".into()),
        kind: kind.map(String::from),
    })
}

#[test]
fn no_pr_signature_is_none() {
    assert_eq!(pr_signature(&None), "none");
}

#[test]
fn github_pr_signature_is_pr_number() {
    assert_eq!(pr_signature(&pr(Some(12), None)), "pr:12");
}

#[test]
fn merge_request_signature_is_none() {
    assert_eq!(pr_signature(&pr(Some(12), Some("mr"))), "none");
}

#[test]
fn review_state_does_not_change_signature() {
    let mut a = pr(Some(12), None);
    let b = pr(Some(12), None);
    if let Some(p) = a.as_mut() {
        p.review_state = Some("approved".into());
    }
    assert_eq!(pr_signature(&a), pr_signature(&b));
}

#[test]
fn agent_status_deserializes_github_pr() {
    let json = r#"{"hook_event_name":"AgentStatus","session_id":"s1","github_pr":{"number":12,"url":"u","review_state":"pending"}}"#;
    let ev: HookEvent = serde_json::from_str(json).unwrap();
    match ev {
        HookEvent::AgentStatus { github_pr, .. } => {
            assert_eq!(
                github_pr,
                Some(GithubPr {
                    number: Some(12),
                    url: Some("u".into()),
                    review_state: Some("pending".into()),
                    kind: None,
                })
            );
        }
        _ => panic!("expected AgentStatus"),
    }
}

#[test]
fn agent_status_github_pr_defaults_to_none() {
    let json = r#"{"hook_event_name":"AgentStatus","session_id":"s1","model_id":"m"}"#;
    let ev: HookEvent = serde_json::from_str(json).unwrap();
    match ev {
        HookEvent::AgentStatus { github_pr, .. } => assert_eq!(github_pr, None),
        _ => panic!("expected AgentStatus"),
    }
}

#[test]
fn agent_status_payload_serializes_github_pr() {
    let payload = agent_status_payload(AgentStatusEmit {
        session_id: "s1",
        agent_id: None,
        model_id: None,
        model_name: None,
        session_started_at: None,
        context_used_pct: None,
        context_window_size: None,
        rate_5h_pct: None,
        rate_5h_resets_at: None,
        rate_7d_pct: None,
        rate_7d_resets_at: None,
        effort_level: None,
        prompt_cache: None,
        github_pr: pr(Some(7), None),
    });
    let json = serde_json::to_string(&payload).unwrap();
    assert!(json.contains(r#""github_pr":{"number":7"#), "{json}");
}
