use super::*;
use crate::terminal::program_status::{ProgramStatus, ReportAction, RootState};

// Cross-module tests (mcp::messaging) need a hook activity stamped in the past, which the
// wall-clock `record_activity` cannot produce; `session_activity` is private to this module tree.
impl AgentRuntimeState {
    pub(crate) fn set_activity_for_test(
        &self,
        sid: &str,
        mode: &str,
        waiting_for: Option<&str>,
        last_activity: u64,
    ) {
        self.session_activity.insert(
            sid.to_string(),
            SessionActivity {
                mode: mode.to_string(),
                last_activity,
                waiting_for: waiting_for.map(str::to_string),
            },
        );
    }
}

fn status(state: RootState, kind: Option<&str>, msg: Option<&str>) -> ProgramStatus {
    ProgramStatus {
        state,
        kind: kind.map(str::to_string),
        msg: msg.map(str::to_string),
    }
}

fn set(state: RootState) -> ReportAction {
    ReportAction::Set(status(state, None, None))
}

fn live(sid: &str, pty: &str) -> AgentRuntimeState {
    let agents = AgentRuntimeState::bootstrap().unwrap();
    agents.mark_live(sid, pty);
    agents
}

fn activity(mode: &str, waiting_for: Option<&str>, last_activity: u64) -> SessionActivity {
    SessionActivity {
        mode: mode.to_string(),
        last_activity,
        waiting_for: waiting_for.map(str::to_string),
    }
}

fn owned(state: RootState, kind: Option<&str>, msg: Option<&str>, set_at: u64) -> OwnedStatus {
    OwnedStatus {
        pty_id: "pty-1".to_string(),
        status: status(state, kind, msg),
        set_at,
    }
}

fn combined(
    hook: Option<SessionActivity>,
    root: Option<OwnedStatus>,
) -> (String, Option<String>, Option<u64>) {
    combined_mode(hook.as_ref(), root.as_ref())
}

fn described(
    mode: &str,
    waiting_for: Option<&str>,
    last: u64,
) -> (String, Option<String>, Option<u64>) {
    (
        mode.to_string(),
        waiting_for.map(str::to_string),
        Some(last),
    )
}

// -- apply_program_status --

#[test]
fn a_status_is_a_change_only_when_it_differs_from_the_stored_one() {
    let agents = live("s", "pty-1");

    let first = agents
        .apply_program_status("s", "pty-1", set(RootState::Working), 1000)
        .unwrap();
    assert_eq!(first.previous, None);
    assert_eq!(first.current, Some(status(RootState::Working, None, None)));

    assert!(
        agents
            .apply_program_status("s", "pty-1", set(RootState::Working), 1001)
            .is_none()
    );

    let next = agents
        .apply_program_status("s", "pty-1", set(RootState::Done), 1002)
        .unwrap();
    assert_eq!(next.previous, Some(status(RootState::Working, None, None)));
    assert_eq!(next.current, Some(status(RootState::Done, None, None)));
}

#[test]
fn a_different_kind_or_msg_alone_is_a_change() {
    let agents = live("s", "pty-1");
    let blocked =
        |msg: &str| ReportAction::Set(status(RootState::Blocked, Some("permission"), Some(msg)));
    agents.apply_program_status("s", "pty-1", blocked("Allow Bash?"), 1000);

    let change = agents
        .apply_program_status("s", "pty-1", blocked("Allow Edit?"), 1001)
        .unwrap();
    assert_eq!(
        change.current,
        Some(status(
            RootState::Blocked,
            Some("permission"),
            Some("Allow Edit?")
        ))
    );
}

#[test]
fn clear_removes_the_status_and_clearing_an_empty_session_is_not_a_change() {
    let agents = live("s", "pty-1");
    assert!(
        agents
            .apply_program_status("s", "pty-1", ReportAction::Clear, 1000)
            .is_none()
    );

    agents.apply_program_status("s", "pty-1", set(RootState::Working), 1001);
    let removed = agents
        .apply_program_status("s", "pty-1", ReportAction::Clear, 1002)
        .unwrap();
    assert_eq!(
        removed.previous,
        Some(status(RootState::Working, None, None))
    );
    assert_eq!(removed.current, None);
    assert!(agents.program_status("s").is_none());
}

#[test]
fn an_ignored_report_changes_nothing() {
    let agents = live("s", "pty-1");
    agents.apply_program_status("s", "pty-1", set(RootState::Done), 1000);
    assert!(
        agents
            .apply_program_status("s", "pty-1", ReportAction::Ignore, 1001)
            .is_none()
    );
    assert_eq!(
        agents.program_status("s").unwrap().status.state,
        RootState::Done
    );
}

#[test]
fn a_late_report_from_a_pty_that_is_not_live_is_ignored() {
    let agents = live("s", "pty-b");
    agents.apply_program_status("s", "pty-b", set(RootState::Working), 1000);

    let late = agents.apply_program_status("s", "pty-a", set(RootState::Done), 1001);

    assert!(late.is_none());
    let stored = agents.program_status("s").unwrap();
    assert_eq!(stored.status.state, RootState::Working);
    assert_eq!(stored.pty_id, "pty-b");
}

#[test]
fn the_live_pty_replaces_a_stale_entry_owned_by_an_old_pty() {
    let agents = live("s", "pty-a");
    agents.apply_program_status("s", "pty-a", set(RootState::Blocked), 1000);
    agents.mark_live("s", "pty-b");

    let change = agents.apply_program_status("s", "pty-b", set(RootState::Idle), 1001);

    assert!(change.is_some());
    let stored = agents.program_status("s").unwrap();
    assert_eq!(stored.status.state, RootState::Idle);
    assert_eq!(stored.pty_id, "pty-b");
}

#[test]
fn eof_of_an_old_pty_keeps_the_new_ptys_status() {
    let agents = live("s", "pty-b");
    agents.apply_program_status("s", "pty-b", set(RootState::Working), 1000);

    assert!(agents.clear_program_status_if("s", "pty-a").is_none());

    assert_eq!(
        agents.program_status("s").unwrap().status.state,
        RootState::Working
    );
}

#[test]
fn eof_of_the_owner_removes_the_status_and_reports_the_removal() {
    let agents = live("s", "pty-b");
    agents.apply_program_status("s", "pty-b", set(RootState::Working), 1000);

    let removed = agents.clear_program_status_if("s", "pty-b").unwrap();

    assert_eq!(
        removed.previous,
        Some(status(RootState::Working, None, None))
    );
    assert_eq!(removed.current, None);
    assert!(agents.program_status("s").is_none());
}

#[test]
fn an_identical_report_keeps_the_original_set_time() {
    let agents = live("s", "pty-1");
    agents.apply_program_status("s", "pty-1", set(RootState::Working), 1000);
    agents.apply_program_status("s", "pty-1", set(RootState::Working), 1005);
    assert_eq!(agents.program_status("s").unwrap().set_at, 1000);
}

#[test]
fn a_status_never_creates_or_changes_the_hook_activity() {
    let agents = live("s", "pty-1");
    agents.apply_program_status("s", "pty-1", set(RootState::Working), 1000);
    assert!(
        agents.session_activity("s").is_none(),
        "no hook activity is invented"
    );

    agents.set_activity_for_test("s", "running", None, 900);
    agents.apply_program_status("s", "pty-1", set(RootState::Idle), 1001);
    let hook = agents.session_activity("s").unwrap();
    assert_eq!(
        (hook.mode.as_str(), hook.last_activity, hook.waiting_for),
        ("running", 900, None)
    );
}

// -- combined_mode --

#[test]
fn working_after_stop_reports_running() {
    assert_eq!(
        combined(
            Some(activity("idle", None, 500)),
            Some(owned(RootState::Working, None, None, 1000))
        ),
        described("running", None, 1000)
    );
}

#[test]
fn blocked_without_kind_or_msg_reports_a_dialog() {
    assert_eq!(
        combined(
            Some(activity("running", None, 900)),
            Some(owned(RootState::Blocked, None, None, 1000))
        ),
        described("needs_attention", Some("dialog"), 1000)
    );
}

#[test]
fn the_blocked_msg_becomes_waiting_for() {
    assert_eq!(
        combined(
            None,
            Some(owned(
                RootState::Blocked,
                Some("permission"),
                Some("Allow Bash?"),
                1000
            ))
        ),
        described("needs_attention", Some("Allow Bash?"), 1000)
    );
}

#[test]
fn a_long_blocked_msg_is_cut_to_200_characters() {
    let ascii = "x".repeat(300);
    let accented = "é".repeat(300);
    for (msg, expected) in [(ascii, "x".repeat(200)), (accented, "é".repeat(200))] {
        let (_, waiting_for, _) = combined(
            None,
            Some(owned(RootState::Blocked, None, Some(&msg), 1000)),
        );
        assert_eq!(waiting_for, Some(expected));
    }
}

#[test]
fn without_a_msg_the_kind_names_what_the_session_waits_for() {
    for (kind, expected) in [
        ("permission", "permission"),
        ("question", "question"),
        ("auth", "login"),
    ] {
        let (_, waiting_for, _) = combined(
            None,
            Some(owned(RootState::Blocked, Some(kind), None, 1000)),
        );
        assert_eq!(waiting_for.as_deref(), Some(expected), "kind {kind}");
    }
}

#[test]
fn done_idle_and_error_report_idle_even_over_a_running_hook_mode() {
    for state in [RootState::Done, RootState::Idle, RootState::Error] {
        let label = state.as_str();
        assert_eq!(
            combined(
                Some(activity("running", None, 900)),
                Some(owned(state, None, None, 1000))
            ),
            described("idle", None, 1000),
            "root {label}"
        );
    }
}

#[test]
fn a_background_agent_wait_wins_over_a_done_status() {
    assert_eq!(
        combined(
            Some(activity(
                "needs_attention",
                Some("background agent: needs input"),
                800
            )),
            Some(owned(RootState::Done, None, None, 1000))
        ),
        described(
            "needs_attention",
            Some("background agent: needs input"),
            1000
        )
    );
}

#[test]
fn a_working_root_with_a_hook_needing_attention_stays_needs_attention() {
    assert_eq!(
        combined(
            Some(activity("needs_attention", Some("permission prompt"), 800)),
            Some(owned(RootState::Working, None, None, 1000))
        ),
        described("needs_attention", Some("permission prompt"), 1000)
    );
}

#[test]
fn a_blocked_root_takes_waiting_for_from_its_own_msg_over_the_hook_value() {
    let (mode, waiting_for, _) = combined(
        Some(activity("needs_attention", Some("permission prompt"), 800)),
        Some(owned(
            RootState::Blocked,
            Some("permission"),
            Some("Allow Bash?"),
            1000,
        )),
    );
    assert_eq!(mode, "needs_attention");
    assert_eq!(waiting_for.as_deref(), Some("Allow Bash?"));
}

#[test]
fn last_activity_is_the_later_of_the_hook_and_the_root_status() {
    assert_eq!(
        combined(
            Some(activity("idle", None, 2000)),
            Some(owned(RootState::Working, None, None, 1000))
        ),
        described("running", None, 2000)
    );
}

#[test]
fn without_a_root_status_the_hook_mode_and_waiting_for_are_kept() {
    assert_eq!(
        combined(Some(activity("running", None, 900)), None),
        described("running", None, 900)
    );
    assert_eq!(
        combined(
            Some(activity("needs_attention", Some("permission prompt"), 700)),
            None
        ),
        described("needs_attention", Some("permission prompt"), 700)
    );
}
