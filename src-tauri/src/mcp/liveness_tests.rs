use super::DaemonContext;
use super::directory::{get_session, list_sessions};
use super::messaging::send_to_session;
use super::worktree_sessions::{request_session_resume, resume_blocked_reason};
use crate::agents::AgentId;
use crate::agents::state::AgentRuntimeState;
use crate::config::{AgentWorktreesConfig, CrossSessionConfig};
use crate::db::Database;
use crate::models::{Session, SessionStatus};
use std::sync::{Arc, Mutex};

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

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

// A clean summary keeps get_session from spawning background generation.
fn add_marker(ctx: &DaemonContext, id: &str) {
    let now = now_secs();
    let g = ctx.db.lock().unwrap();
    g.set_session_summary(id, "recap", None, None, now).unwrap();
    g.set_session_transcript(id, &format!("/t/{id}.jsonl"), now)
        .unwrap();
}

fn cross_cfg() -> CrossSessionConfig {
    CrossSessionConfig {
        enabled: true,
        max_hops: 4,
        msg_budget: 30,
        deadline_secs: 1800,
    }
}

fn worktrees_cfg() -> AgentWorktreesConfig {
    AgentWorktreesConfig {
        enabled: true,
        ..Default::default()
    }
}

#[test]
fn a_registered_session_that_was_never_marked_live_reports_not_live() {
    let ctx = ctx();
    add_session(&ctx, "reg-only");
    add_marker(&ctx, "reg-only");
    ctx.agents
        .register_session("reg-only", AgentId::claude_code());

    let listed = list_sessions(&ctx, None).unwrap();
    let from_list = listed.iter().find(|d| d.session_id == "reg-only").unwrap();
    assert!(!from_list.is_live);
    assert!(!get_session(&ctx, "reg-only").unwrap().unwrap().is_live);
}

#[test]
fn a_killed_session_without_a_marker_leaves_the_listing_but_still_accepts_a_resume_request() {
    let ctx = ctx();
    add_session(&ctx, "no-marker");
    ctx.agents.mark_live("no-marker", "pty-1");
    ctx.agents.mark_dead_if("no-marker", "pty-1");

    let listed = list_sessions(&ctx, None).unwrap();
    assert!(listed.iter().all(|d| d.session_id != "no-marker"));
    assert!(get_session(&ctx, "no-marker").unwrap().is_none());

    let resume = request_session_resume(&ctx, &worktrees_cfg(), "requester", "no-marker", None);
    assert_eq!(resume["status"], "pending");
}

#[test]
fn a_not_live_descriptor_keeps_past_facts_and_drops_present_claims() {
    let ctx = ctx();
    add_session(&ctx, "was-live");
    add_marker(&ctx, "was-live");
    ctx.agents.mark_live("was-live", "pty-1");
    ctx.agents
        .record_activity("was-live", "needs_attention", Some("permission prompt"));
    ctx.agents.record_touched_file("was-live", "a.rs", "Edit");
    ctx.agents
        .set_session_last_message("was-live", Some("left off here".into()));
    ctx.agents.set_session_background(
        "was-live",
        vec![serde_json::json!({"id": "bg-1"})],
        vec![serde_json::json!({"id": "cron-1"})],
    );
    let recorded = ctx
        .agents
        .session_activity("was-live")
        .unwrap()
        .last_activity;
    ctx.agents.mark_dead_if("was-live", "pty-1");

    let d = get_session(&ctx, "was-live").unwrap().unwrap();

    assert!(!d.is_live);
    assert_eq!(d.mode, "idle");
    assert_eq!(d.waiting_for, None);
    assert!(d.background_tasks.is_empty());
    assert!(d.session_crons.is_empty());
    assert_ne!(recorded, 0);
    assert_eq!(d.last_activity, recorded);
    assert_eq!(d.last_assistant_message.as_deref(), Some("left off here"));
    assert_eq!(d.recently_touched_files.len(), 1);
    assert_eq!(d.recently_touched_files[0].path, "a.rs");
}

#[test]
fn send_to_a_registered_but_not_live_session_returns_inactive_target() {
    let ctx = ctx();
    add_session(&ctx, "send-sender");
    add_session(&ctx, "send-target");
    ctx.agents
        .register_session("send-target", AgentId::claude_code());

    let out =
        send_to_session(&ctx, &cross_cfg(), "send-sender", "send-target", "hi", None).unwrap();

    assert_eq!(out["status"], "inactive_target");
}

#[test]
fn a_resume_request_for_a_registered_but_not_live_session_is_pending_not_already_live() {
    let ctx = ctx();
    add_session(&ctx, "resume-target");
    ctx.agents
        .register_session("resume-target", AgentId::claude_code());

    let out = request_session_resume(&ctx, &worktrees_cfg(), "requester", "resume-target", None);

    assert_eq!(out["status"], "pending");
}

#[test]
fn resume_blocked_reason_matrix() {
    let live = resume_blocked_reason(true, true).unwrap();
    assert!(live.contains("message it directly"), "{live}");
    assert_eq!(resume_blocked_reason(true, false), Some(live));

    let open_tab = resume_blocked_reason(false, true).unwrap();
    assert!(open_tab.contains("close it"), "{open_tab}");

    assert_eq!(resume_blocked_reason(false, false), None);
}
