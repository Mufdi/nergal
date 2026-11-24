use super::{closes_review, release_plan_review};
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
fn exit_plan_mode_from_owner_main_agent_closes() {
    assert!(closes_review(&post_tool("ExitPlanMode", None), true, None));
}

#[test]
fn exit_plan_mode_from_subagent_owner_closes() {
    assert!(closes_review(
        &post_tool("ExitPlanMode", Some("a1")),
        true,
        Some("a1")
    ));
}

#[test]
fn exit_plan_mode_from_a_different_agent_does_not_close() {
    assert!(!closes_review(
        &post_tool("ExitPlanMode", Some("a2")),
        true,
        Some("a1")
    ));
    assert!(!closes_review(
        &post_tool("ExitPlanMode", Some("a2")),
        true,
        None
    ));
    assert!(!closes_review(
        &post_tool("ExitPlanMode", None),
        true,
        Some("a1")
    ));
}

#[test]
fn other_tool_post_use_does_not_close() {
    assert!(!closes_review(&post_tool("Bash", None), true, None));
}

#[test]
fn nested_agent_cli_event_never_closes() {
    assert!(!closes_review(
        &post_tool("ExitPlanMode", None),
        false,
        None
    ));
    assert!(!closes_review(&stop(), false, None));
    assert!(!closes_review(
        &subagent_stop(Some("a1")),
        false,
        Some("a1")
    ));
}

#[test]
fn stop_closes_only_a_main_agent_review() {
    assert!(closes_review(&stop(), true, None));
    assert!(!closes_review(&stop(), true, Some("a1")));
}

#[test]
fn subagent_stop_closes_only_the_owning_subagent() {
    assert!(closes_review(&subagent_stop(Some("a1")), true, Some("a1")));
    assert!(!closes_review(&subagent_stop(Some("a2")), true, Some("a1")));
    assert!(!closes_review(&subagent_stop(Some("a1")), true, None));
    assert!(!closes_review(&subagent_stop(None), true, None));
}

#[test]
fn session_end_always_closes() {
    let ev = HookEvent::SessionEnd {
        session_id: "s1".into(),
    };
    assert!(closes_review(&ev, true, None));
    assert!(closes_review(&ev, true, Some("a1")));
}

#[test]
fn unrelated_events_do_not_close() {
    let ev = HookEvent::UserPromptSubmit {
        session_id: "s1".into(),
    };
    assert!(!closes_review(&ev, true, None));
}

#[cfg(unix)]
mod release {
    use super::release_plan_review;
    use std::ffi::CString;
    use std::fs::OpenOptions;
    use std::io::Read;
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::OpenOptionsExt;
    use std::time::{Duration, Instant};

    #[test]
    fn release_writes_exactly_released_true_to_the_reader() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("plan.fifo");
        let c = CString::new(path.as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(c.as_ptr(), 0o600) }, 0);
        let mut reader = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NONBLOCK)
            .open(&path)
            .unwrap();

        release_plan_review(Some(path.clone()));

        let mut out = Vec::new();
        let mut buf = [0u8; 256];
        let give_up = Instant::now() + Duration::from_secs(1);
        while Instant::now() < give_up {
            match reader.read(&mut buf) {
                Ok(0) if !out.is_empty() => break,
                Ok(0) => std::thread::sleep(Duration::from_millis(1)),
                Ok(n) => out.extend_from_slice(&buf[..n]),
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(1))
                }
                Err(e) => panic!("reader error: {e}"),
            }
        }
        assert_eq!(out, b"{\"released\":true}");
    }
}
