use super::*;

fn snapshot_with(pr: Option<serde_json::Value>) -> serde_json::Value {
    let mut snapshot = serde_json::json!({
        "session_id": "sess-abc",
        "model": { "id": "m", "display_name": "M" },
        "context_window": { "used_percentage": 12, "context_window_size": 200000 }
    });
    if let Some(pr) = pr {
        snapshot["pr"] = pr;
    }
    snapshot
}

#[test]
fn pr_object_maps_to_github_pr_with_null_kind() {
    let pr = serde_json::json!({ "number": 1234, "url": "u", "review_state": "pending" });
    let out = statusline_to_agent_status(&snapshot_with(Some(pr)), "n");
    assert_eq!(out["github_pr"]["number"], 1234);
    assert_eq!(out["github_pr"]["url"], "u");
    assert_eq!(out["github_pr"]["review_state"], "pending");
    assert!(out["github_pr"]["kind"].is_null());
}

#[test]
fn pr_kind_is_carried_through() {
    let pr = serde_json::json!({ "number": 7, "url": "u", "kind": "mr" });
    let out = statusline_to_agent_status(&snapshot_with(Some(pr)), "n");
    assert_eq!(out["github_pr"]["kind"], "mr");
}

#[test]
fn missing_pr_becomes_null_github_pr() {
    let out = statusline_to_agent_status(&snapshot_with(None), "n");
    assert!(
        out.get("github_pr").is_some(),
        "github_pr key must be emitted"
    );
    assert!(out["github_pr"].is_null());
}

#[test]
fn non_object_pr_becomes_null_github_pr() {
    for pr in [
        serde_json::json!("12"),
        serde_json::json!(12),
        serde_json::json!(null),
    ] {
        let out = statusline_to_agent_status(&snapshot_with(Some(pr.clone())), "n");
        assert!(out["github_pr"].is_null(), "pr {pr} must map to null");
    }
}
