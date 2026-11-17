use super::{is_top_level_chain, parse_agent_chain, record_cc_conversation_id, record_stop_marker};
use crate::db::Database;
use crate::hooks::events::HookEvent;
use crate::models::{Session, SessionStatus};
use crate::platform_proc::AgentChain;
use serde_json::{Value, json};

const SHELL: u32 = 500;

fn chain(entries: &[(u32, bool)], complete: bool) -> AgentChain {
    AgentChain {
        entries: entries.to_vec(),
        complete,
    }
}

fn raw(chain: Value, complete: Option<Value>) -> Value {
    let mut v = json!({ "hook_event_name": "SessionStart", "agent_chain": chain });
    if let Some(c) = complete {
        v["agent_chain_complete"] = c;
    }
    v
}

#[test]
fn a_well_formed_chain_is_parsed_with_its_completeness() {
    let v = raw(
        json!([{ "pid": 901, "claude": true }, { "pid": 500, "claude": false }]),
        Some(json!(true)),
    );

    let parsed = parse_agent_chain(&v).unwrap();

    assert_eq!(parsed.entries, vec![(901, true), (500, false)]);
    assert!(parsed.complete);
}

#[test]
fn a_chain_without_a_completeness_flag_is_incomplete() {
    let v = raw(json!([{ "pid": 901, "claude": true }]), None);
    assert!(!parse_agent_chain(&v).unwrap().complete);

    let non_bool = raw(json!([{ "pid": 901, "claude": true }]), Some(json!("yes")));
    assert!(!parse_agent_chain(&non_bool).unwrap().complete);
}

#[test]
fn a_malformed_chain_is_treated_as_absent() {
    let cases = [
        ("string pid", json!([{ "pid": "901", "claude": true }])),
        ("negative pid", json!([{ "pid": -5, "claude": true }])),
        (
            "pid above u32",
            json!([{ "pid": 4294967296u64, "claude": true }]),
        ),
        (
            "non-boolean claude",
            json!([{ "pid": 901, "claude": "yes" }]),
        ),
        ("missing claude", json!([{ "pid": 901 }])),
        (
            "one bad item among good",
            json!([{ "pid": 1, "claude": true }, { "pid": "x", "claude": false }]),
        ),
        ("not an array", json!({ "pid": 901, "claude": true })),
    ];
    for (name, chain) in cases {
        assert!(
            parse_agent_chain(&raw(chain, Some(json!(true)))).is_none(),
            "{name}"
        );
    }
    assert!(parse_agent_chain(&json!({ "hook_event_name": "SessionStart" })).is_none());
}

#[test]
fn top_level_classification_follows_the_claude_runs_below_the_shell() {
    let cases: [(&str, AgentChain, bool); 6] = [
        (
            "direct child of the shell",
            chain(&[(901, true), (500, false)], false),
            true,
        ),
        (
            "behind a bash wrapper",
            chain(&[(902, true), (650, false), (500, false)], false),
            true,
        ),
        (
            "npx launcher counts as one run",
            chain(&[(903, true), (700, true), (500, false)], false),
            true,
        ),
        (
            "nested via a tool shell",
            chain(
                &[(7400, true), (7310, false), (901, true), (500, false)],
                false,
            ),
            false,
        ),
        (
            "complete chain that never reaches the shell",
            chain(&[(8800, true), (8700, false), (1, false)], true),
            false,
        ),
        (
            "truncated chain that never reaches the shell",
            chain(&[(8800, true), (8700, false), (1, false)], false),
            true,
        ),
    ];
    for (name, c, expected) in cases {
        assert_eq!(
            is_top_level_chain(Some(&c), Some(SHELL)),
            expected,
            "{name}"
        );
    }
}

#[test]
fn without_a_chain_or_a_known_shell_the_event_is_top_level() {
    let nested = chain(
        &[(7400, true), (7310, false), (901, true), (500, false)],
        true,
    );

    assert!(is_top_level_chain(None, Some(SHELL)));
    assert!(is_top_level_chain(Some(&nested), None));
}

fn seed(db: &Database, id: &str, agent_id: &str, internal: Option<&str>) {
    if db.find_session(id).unwrap().is_none() {
        let _ = db.create_workspace("w", "ws", "/repo");
    }
    let s = Session {
        id: id.to_string(),
        name: id.to_string(),
        workspace_id: "w".into(),
        worktree_path: None,
        worktree_branch: None,
        merge_target: None,
        status: SessionStatus::Idle,
        created_at: 100,
        updated_at: 100,
        agent_id: agent_id.to_string(),
        agent_internal_session_id: internal.map(String::from),
        agent_capabilities: Vec::new(),
        pinned_note_paths: Vec::new(),
        launch_options: None,
        env_shells: Vec::new(),
        active_clickup_task_id: None,
        pinned_clickup_task_ids: Vec::new(),
        active_linear_issue_id: None,
        pinned_linear_issue_ids: Vec::new(),
    };
    db.create_session(&s).unwrap();
}

fn event(name: &str, conversation_id: &str) -> HookEvent {
    serde_json::from_value(json!({ "hook_event_name": name, "session_id": conversation_id }))
        .unwrap()
}

fn stored(db: &Database, id: &str) -> Option<String> {
    db.find_session(id)
        .unwrap()
        .unwrap()
        .agent_internal_session_id
}

#[test]
fn session_start_records_the_conversation_id() {
    let db = Database::open_in_memory().unwrap();
    seed(&db, "s1", "claude-code", None);

    record_cc_conversation_id(&db, "s1", &event("SessionStart", "aaaa-1111"));

    assert_eq!(stored(&db, "s1").as_deref(), Some("aaaa-1111"));
}

#[test]
fn a_new_conversation_after_clear_overwrites_the_id() {
    let db = Database::open_in_memory().unwrap();
    seed(&db, "s1", "claude-code", Some("aaaa-1111"));

    record_cc_conversation_id(&db, "s1", &event("SessionStart", "bbbb-2222"));

    assert_eq!(stored(&db, "s1").as_deref(), Some("bbbb-2222"));
}

#[test]
fn stop_does_not_write_the_conversation_id() {
    let db = Database::open_in_memory().unwrap();
    seed(&db, "s1", "claude-code", Some("aaaa-1111"));

    record_cc_conversation_id(&db, "s1", &event("Stop", "cccc-3333"));

    assert_eq!(stored(&db, "s1").as_deref(), Some("aaaa-1111"));
}

#[test]
fn another_agents_session_is_not_written() {
    let db = Database::open_in_memory().unwrap();
    seed(&db, "p1", "pi", Some("pi-uuid"));

    record_cc_conversation_id(&db, "p1", &event("SessionStart", "dddd-4444"));

    assert_eq!(stored(&db, "p1").as_deref(), Some("pi-uuid"));
}

#[test]
fn an_invalid_conversation_id_is_ignored() {
    let db = Database::open_in_memory().unwrap();
    seed(&db, "s1", "claude-code", Some("aaaa-1111"));

    record_cc_conversation_id(&db, "s1", &event("SessionStart", "../x"));

    assert_eq!(stored(&db, "s1").as_deref(), Some("aaaa-1111"));
}

fn marker(db: &Database, id: &str) -> (String, u64) {
    let t = db.get_session_transcript(id).unwrap().unwrap();
    (t.transcript_path, t.last_stop_at)
}

#[test]
fn a_top_level_stop_moves_the_transcript_marker() {
    let db = Database::open_in_memory().unwrap();
    seed(&db, "s1", "claude-code", None);
    db.set_session_transcript("s1", "/p/old.jsonl", 50).unwrap();

    record_stop_marker(&db, "s1", Some("/p/new.jsonl"), true);

    let (path, stop_at) = marker(&db, "s1");
    assert_eq!(path, "/p/new.jsonl");
    assert!(stop_at > 50);
}

#[test]
fn a_nested_stop_does_not_move_the_transcript_marker() {
    let db = Database::open_in_memory().unwrap();
    seed(&db, "s1", "claude-code", None);
    db.set_session_transcript("s1", "/p/old.jsonl", 50).unwrap();

    record_stop_marker(&db, "s1", Some("/p/nested.jsonl"), false);

    assert_eq!(marker(&db, "s1"), ("/p/old.jsonl".to_string(), 50));
}

#[test]
fn a_transcript_path_that_is_not_jsonl_is_ignored() {
    let db = Database::open_in_memory().unwrap();
    seed(&db, "s1", "claude-code", None);
    db.set_session_transcript("s1", "/p/old.jsonl", 50).unwrap();

    record_stop_marker(&db, "s1", Some("/home/u/.ssh/id_ed25519"), true);

    assert_eq!(marker(&db, "s1"), ("/p/old.jsonl".to_string(), 50));
}
