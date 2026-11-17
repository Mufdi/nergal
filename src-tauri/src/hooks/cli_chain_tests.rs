use super::attach_agent_chain;
use crate::platform_proc::AgentChain;
use serde_json::{Value, json};

fn chain() -> AgentChain {
    AgentChain {
        entries: vec![(901, true), (500, false)],
        complete: true,
    }
}

#[test]
fn session_start_and_stop_carry_the_agent_chain() {
    for event in ["SessionStart", "Stop"] {
        let mut payload = json!({ "hook_event_name": event, "session_id": "aaaa-1111" });

        attach_agent_chain(&mut payload, &chain());

        assert_eq!(
            payload["agent_chain"],
            json!([{ "pid": 901, "claude": true }, { "pid": 500, "claude": false }]),
            "{event}"
        );
        assert_eq!(
            payload["agent_chain_complete"],
            Value::Bool(true),
            "{event}"
        );
        assert_eq!(payload["session_id"], "aaaa-1111", "{event}");
    }
}

#[test]
fn other_events_are_left_untouched() {
    let mut payload = json!({ "hook_event_name": "PreToolUse", "tool_name": "Bash" });
    let before = payload.clone();

    attach_agent_chain(&mut payload, &chain());

    assert_eq!(payload, before);
}
