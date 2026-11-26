// `ProgramStatusScanner::default()` is the constructor: the plan names no `new`.
use super::*;

fn scan(chunks: &[&[u8]]) -> Vec<String> {
    let mut scanner = ProgramStatusScanner::default();
    chunks
        .iter()
        .flat_map(|chunk| scanner.feed(chunk))
        .map(|found| match found {
            Osc7501::Query => "<query>".to_string(),
            Osc7501::Report(body) => String::from_utf8(body).unwrap(),
        })
        .collect()
}

fn set(body: &str) -> ProgramStatus {
    match parse_report(body.as_bytes()) {
        ReportAction::Set(status) => status,
        _ => panic!("expected a Set action for {body:?}"),
    }
}

fn status(state: RootState, kind: Option<&str>, msg: Option<&str>) -> ProgramStatus {
    ProgramStatus {
        state,
        kind: kind.map(str::to_string),
        msg: msg.map(str::to_string),
    }
}

#[test]
fn a_report_split_across_reads_is_joined() {
    let found = scan(&[b"\x1b]7501;state=wor", b"king:app=claude-code\x1b\\"]);
    assert_eq!(found, vec!["state=working:app=claude-code"]);
}

#[test]
fn an_oversized_body_is_discarded_and_the_next_report_is_kept() {
    let mut out = b"\x1b]7501;state=working:msg=".to_vec();
    out.extend(std::iter::repeat_n(b'A', 5000));
    out.extend_from_slice(b"\x1b\\\x1b]7501;state=done\x1b\\");
    assert_eq!(scan(&[&out]), vec!["state=done"]);
}

#[test]
fn a_body_of_exactly_max_body_bytes_is_kept_and_one_byte_more_is_dropped() {
    let body_of = |len: usize| {
        let mut body = b"state=working:msg=".to_vec();
        body.resize(len, b'A');
        body
    };
    let framed = |body: &[u8]| {
        let mut out = b"\x1b]7501;".to_vec();
        out.extend_from_slice(body);
        out.extend_from_slice(b"\x07");
        out
    };
    let at_cap = body_of(MAX_BODY);
    assert_eq!(
        scan(&[&framed(&at_cap)]),
        vec![String::from_utf8(at_cap).unwrap()]
    );
    assert!(scan(&[&framed(&body_of(MAX_BODY + 1))]).is_empty());
}

#[test]
fn an_escape_followed_by_another_byte_aborts_an_unterminated_body() {
    let found = scan(&[b"\x1b]7501;state=working\x1b[2J\x1b]7501;state=done\x07"]);
    assert_eq!(found, vec!["state=done"]);
}

#[test]
fn the_aborting_escape_is_scanned_again_as_a_prefix_start() {
    let found = scan(&[b"\x1b]7501;state=working\x1b]7501;state=done\x07"]);
    assert_eq!(found, vec!["state=done"]);
}

#[test]
fn can_and_sub_abort_a_body_without_swallowing_the_next_query() {
    for abort in [0x18u8, 0x1a] {
        let mut out = b"\x1b]7501;state=working".to_vec();
        out.push(abort);
        out.extend_from_slice(b"\x1b]7501;?\x07");
        assert_eq!(scan(&[&out]), vec!["<query>"], "abort byte {abort:#x}");
    }
}

#[test]
fn other_osc_codes_yield_nothing() {
    let found = scan(&[b"\x1b]8;;https://x\x1b\\\x1b]11;?\x07\x1b]75010;state=done\x07"]);
    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn bel_and_st_terminate_queries_and_reports_amid_other_output() {
    let found = scan(&[
        b"before\x1b]7501;?\x07mid\x1b]7501;?\x1b\\\x1b]7501;state=done\x07x\x1b]7501;state=idle\x1b\\after",
    ]);
    assert_eq!(
        found,
        vec!["<query>", "<query>", "state=done", "state=idle"]
    );
}

#[test]
fn a_query_split_inside_the_prefix_is_found_once() {
    let found = scan(&[b"\x1b]75", b"01;?\x07"]);
    assert_eq!(found, vec!["<query>"]);
}

#[test]
fn a_terminator_split_between_escape_and_backslash_is_joined() {
    let found = scan(&[b"\x1b]7501;state=done\x1b", b"\\"]);
    assert_eq!(found, vec!["state=done"]);
}

#[test]
fn a_working_report_has_no_kind_and_no_msg() {
    assert_eq!(
        set("state=working:app=claude-code"),
        status(RootState::Working, None, None)
    );
}

#[test]
fn a_blocked_report_keeps_kind_and_the_decoded_msg() {
    assert_eq!(
        set("state=blocked:app=claude-code:kind=permission:msg=QWxsb3cgQmFzaD8="),
        status(RootState::Blocked, Some("permission"), Some("Allow Bash?"))
    );
}

#[test]
fn control_characters_are_removed_from_msg() {
    let c0 = set("state=blocked:msg=QWxsb3cbWzMxbSBCYXNoPwc=");
    assert_eq!(c0.msg.as_deref(), Some("Allow[31m Bash?"));
    let c1 = set("state=blocked:msg=QWxsb3fCmyBCYXNoPw==");
    assert_eq!(c1.msg.as_deref(), Some("Allow Bash?"));
}

#[test]
fn kind_is_kept_only_for_blocked() {
    assert_eq!(
        set("state=working:kind=permission").kind,
        None,
        "working drops kind"
    );
    assert_eq!(
        set("state=done:kind=question").kind,
        None,
        "done drops kind"
    );
}

#[test]
fn every_known_state_parses() {
    let cases = [
        ("state=working", RootState::Working),
        ("state=blocked", RootState::Blocked),
        ("state=done", RootState::Done),
        ("state=idle", RootState::Idle),
        ("state=error", RootState::Error),
    ];
    for (body, expected) in cases {
        assert_eq!(set(body).state, expected, "{body}");
    }
}

#[test]
fn clear_without_id_is_a_clear() {
    assert!(matches!(parse_report(b"state=clear"), ReportAction::Clear));
    assert!(matches!(
        parse_report(b"state=clear:app=claude-code"),
        ReportAction::Clear
    ));
}

#[test]
fn reports_for_a_child_unknown_states_and_missing_states_are_ignored() {
    let ignored = [
        "state=working:app=claude-code:id=task-1",
        "state=clear:id=task-1",
        "state=paused:app=claude-code",
        "app=claude-code",
        "",
    ];
    for body in ignored {
        assert!(
            matches!(parse_report(body.as_bytes()), ReportAction::Ignore),
            "{body:?} must be ignored"
        );
    }
}

#[test]
fn an_invalid_msg_is_dropped_and_the_rest_of_the_report_is_kept() {
    assert_eq!(
        set("state=error:app=claude-code:msg=!!!"),
        status(RootState::Error, None, None)
    );
}

#[test]
fn root_state_names_are_the_wire_names() {
    assert_eq!(RootState::Working.as_str(), "working");
    assert_eq!(RootState::Blocked.as_str(), "blocked");
    assert_eq!(RootState::Done.as_str(), "done");
    assert_eq!(RootState::Idle.as_str(), "idle");
    assert_eq!(RootState::Error.as_str(), "error");
}
