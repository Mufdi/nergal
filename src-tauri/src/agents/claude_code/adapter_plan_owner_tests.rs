use super::ClaudeCodeAdapter;
use std::path::PathBuf;

#[test]
fn nothing_registered_has_no_owner_entry() {
    let a = ClaudeCodeAdapter::new();
    assert_eq!(a.pending_plan_owner("s1"), None);
}

#[test]
fn plain_register_is_owned_by_the_main_agent() {
    let a = ClaudeCodeAdapter::new();
    a.register_pending_plan_fifo("s1", PathBuf::from("/p/a"));
    assert_eq!(a.pending_plan_owner("s1"), Some(None));
}

#[test]
fn owned_register_stores_the_subagent_owner() {
    let a = ClaudeCodeAdapter::new();
    a.register_pending_plan_fifo_owned("s1", PathBuf::from("/p/a"), Some("agent-7".into()));
    assert_eq!(
        a.pending_plan_owner("s1"),
        Some(Some("agent-7".to_string()))
    );
}

#[test]
fn owner_is_per_session() {
    let a = ClaudeCodeAdapter::new();
    a.register_pending_plan_fifo_owned("s1", PathBuf::from("/p/a"), Some("agent-7".into()));
    assert_eq!(a.pending_plan_owner("s2"), None);
}
