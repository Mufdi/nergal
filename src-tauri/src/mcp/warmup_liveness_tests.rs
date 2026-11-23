use super::DaemonContext;
use super::messaging::send_to_session;
use crate::agents::state::AgentRuntimeState;
use crate::config::CrossSessionConfig;
use crate::db::Database;
use crate::models::{Session, SessionStatus};
use std::sync::{Arc, Mutex};

fn ctx() -> DaemonContext {
    let db = Arc::new(Mutex::new(Database::open_in_memory().unwrap()));
    db.lock()
        .unwrap()
        .create_workspace("ws1", "ws", "/tmp/repo")
        .unwrap();
    DaemonContext {
        db,
        agents: AgentRuntimeState::bootstrap().unwrap(),
        delivery: Arc::new(super::delivery::NoopDelivery),
        worktree_gate: Default::default(),
    }
}

fn add_session(ctx: &DaemonContext, id: &str) {
    let s = Session {
        id: id.to_string(),
        name: id.to_string(),
        workspace_id: "ws1".into(),
        worktree_path: None,
        worktree_branch: None,
        merge_target: None,
        status: SessionStatus::Idle,
        created_at: 0,
        updated_at: 0,
        agent_id: "claude-code".into(),
        agent_internal_session_id: None,
        agent_capabilities: Vec::new(),
        worktree_pending_delete: false,
        pinned_note_paths: Vec::new(),
        launch_options: None,
        env_shells: Vec::new(),
        active_clickup_task_id: None,
        pinned_clickup_task_ids: Vec::new(),
        active_linear_issue_id: None,
        pinned_linear_issue_ids: Vec::new(),
    };
    ctx.db.lock().unwrap().create_session(&s).unwrap();
}

fn cross_cfg() -> CrossSessionConfig {
    CrossSessionConfig {
        enabled: true,
        max_hops: 4,
        msg_budget: 30,
        deadline_secs: 1800,
    }
}

#[test]
fn a_send_before_the_pty_reservation_is_refused_and_queues_after_it() {
    let ctx = ctx();
    add_session(&ctx, "warm-sender");
    add_session(&ctx, "warm-target");
    ctx.agents.record_activity("warm-target", "running", None);

    let before =
        send_to_session(&ctx, &cross_cfg(), "warm-sender", "warm-target", "hi", None).unwrap();
    assert_eq!(before["status"], "inactive_target");

    ctx.agents.mark_live("warm-target", "pty-1");

    let after =
        send_to_session(&ctx, &cross_cfg(), "warm-sender", "warm-target", "hi", None).unwrap();
    assert_eq!(after["status"], "queued");
}
